//! Integração com o SISTEMA: ambiente gráfico, relançar o app, abrir pastas/editor,
//! achar binários no PATH e disparar um terminal externo. Tudo que sai do processo.

use crate::prelude::*;

// ---------------------------------------------------------------------------
// Detecção de ambiente gráfico (Wayland vs X11 + desktop). Só loga — o backend
// winit do Slint escolhe sozinho o transporte certo em runtime.
// ---------------------------------------------------------------------------
pub(crate) fn detect_display_env() {
    let wayland = std::env::var("WAYLAND_DISPLAY").ok().filter(|s| !s.is_empty());
    let x11 = std::env::var("DISPLAY").ok().filter(|s| !s.is_empty());
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into());

    let server = match (&wayland, &x11) {
        (Some(w), _) => format!("Wayland (WAYLAND_DISPLAY={w})"),
        (None, Some(d)) => format!("X11 (DISPLAY={d})"),
        (None, None) => "NENHUM display detectado (headless/sandbox)".into(),
    };
    eprintln!("[env] servidor gráfico : {server}");
    eprintln!("[env] XDG_SESSION_TYPE : {}", if session.is_empty() { "?".into() } else { session });
    eprintln!("[env] desktop          : {desktop}");
    eprintln!("[env] idioma i18n      : {}", i18n::current_code());
    eprintln!("[env] backend Slint    : winit (default) — cobre Wayland E X11; renderer femtovg (OpenGL/GLES)");
    if wayland.is_none() && x11.is_none() {
        eprintln!("[env] AVISO: sem display, a janela não abre. Este incremento valida COMPILAÇÃO; a exibição precisa de um servidor Wayland/X11.");
    }
}

// ---------------------------------------------------------------------------
// Logo da janela — MESMA marca do egui (`schematize::appicon::rgba(256)`),
// convertida num `slint::Image` pra alimentar a propriedade `icon` do Window.
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Relança o app numa janela NOVA e encerra este processo. CONSERTO do bug do
// "reiniciar" (pós self-update) que só fechava e não reabria: fazemos um spawn
// DESACOPLADO do binário atual (nova sessão de processos via `process_group(0)`
// + stdio em /dev/null) ANTES do `exit(0)`, então a janela nova sobe sozinha e
// sobrevive à saída deste. Chamado pelo callback `restart` do Slint.
// ---------------------------------------------------------------------------
pub(crate) fn restart_app() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(&exe);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.process_group(0); // grupo próprio → não morre com o processo atual
        let _ = cmd.spawn(); // best-effort: se falhar, ainda saímos limpo
    }
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Abre um caminho no gerenciador de arquivos do sistema (xdg-open <path>).
// ---------------------------------------------------------------------------
pub(crate) fn open_path_in_files(root: &Path) {
    util::open_url(&root.to_string_lossy());
}

// ---------------------------------------------------------------------------
// Abre o projeto no VSCode: `code <root>` se o binário existe; senão cai no
// esquema `vscode://file/<root>` (best-effort via xdg-open).
// ---------------------------------------------------------------------------
pub(crate) fn open_in_vscode(root: &Path) {
    let root_s = root.to_string_lossy().into_owned();
    if which_bin("code")
        && std::process::Command::new("code")
            .arg(&root_s)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    {
        return;
    }
    util::open_url(&format!("vscode://file/{root_s}"));
}

/// Localiza o binário `schematize` (CLI) pra montar o comando do terminal:
/// primeiro um irmão do executável atual; senão o do PATH.
pub(crate) fn schematize_bin() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("schematize");
            if cand.is_file() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    "schematize".into()
}

/// Um binário existe no PATH?
pub(crate) fn which_bin(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Abre um terminal gráfico rodando `inner` (bash -c). Mesmo padrão do gui.rs (egui):
/// cobre konsole/gnome-terminal/xfce4-terminal/x-terminal-emulator/kitty/alacritty/tilix/xterm.
pub(crate) fn launch_terminal(inner: &str) -> bool {
    let cands: &[(&str, &[&str])] = &[
        ("x-terminal-emulator", &["-e"]),
        ("konsole", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("xfce4-terminal", &["-x"]),
        ("tilix", &["-e"]),
        ("kitty", &[]),
        ("alacritty", &["-e"]),
        ("xterm", &["-e"]),
    ];
    for (term, pre) in cands {
        if which_bin(term)
            && std::process::Command::new(term)
                .args(*pre)
                .arg("bash")
                .arg("-c")
                .arg(inner)
                .spawn()
                .is_ok()
        {
            return true;
        }
    }
    false
}

/// Seta o app_id (Wayland) / WM_CLASS (X11) da janela = "schematize-gui" ANTES de criá-la, instalando
/// um backend winit com `with_window_attributes_hook`. No Wayland (KDE/GNOME) o compositor IGNORA o
/// ícone-buffer da janela e casa pelo app_id ao `schematize-gui.desktop` pra achar o ícone — sem isto
/// o dock mostra um fallback genérico ("W"). Linux só; macOS/Windows pegam o ícone do bundle/.exe.
#[cfg(target_os = "linux")]
pub(crate) fn set_window_app_id() {
    use i_slint_backend_winit::winit::platform::wayland::WindowAttributesExtWayland;
    use i_slint_backend_winit::winit::platform::x11::WindowAttributesExtX11;
    let built = i_slint_backend_winit::Backend::builder()
        .with_window_attributes_hook(|attrs| {
            // `general` = app_id (Wayland) / res_class (X11). Bate com schematize-gui.desktop.
            let attrs = WindowAttributesExtWayland::with_name(attrs, "schematize-gui", "schematize-gui");
            WindowAttributesExtX11::with_name(attrs, "schematize-gui", "schematize-gui")
        })
        .build();
    if let Ok(backend) = built {
        let _ = slint::platform::set_platform(Box::new(backend));
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn set_window_app_id() {}

/// Ícone da janela desenhado em código (`schematize::appicon::rgba`) — resiliente: não depende de
/// arquivo (não some nem quebra o build), e sai nítido em qualquer tamanho (antialiasing no lib).
pub(crate) fn make_app_icon() -> slint::Image {
    let (rgba, w, h) = schematize::appicon::rgba(256);
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, w, h);
    slint::Image::from_rgba8(buf)
}
