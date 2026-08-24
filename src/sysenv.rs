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
/// Desacopla o processo filho do atual — ele sobrevive ao fechamento desta janela.
///
/// O quê: `process_group(0)` em Unix; NO-OP em Windows. Onde: `restart_app` e o relançamento
/// da GUI por projeto (`wire::overdev`).
///
/// Por que existe: os dois chamadores usavam `cmd.process_group(0)` direto, com o trait
/// importado sem `#[cfg(unix)]` no prelude — o que quebra o build de Windows. Concentrar aqui
/// evita que o próximo `spawn` repita o erro.
///
/// **Limite honesto:** em Windows isto NÃO desacopla nada. O equivalente é
/// `CREATE_NEW_PROCESS_GROUP`/`DETACHED_PROCESS` via `CommandExt` do Windows, que é outro
/// trabalho; até lá, o filho morre junto com o pai naquela plataforma.
pub(crate) fn desacopla_processo(cmd: &mut std::process::Command) {
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(not(unix))]
    let _ = cmd;
}

pub(crate) fn restart_app() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(&exe);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        desacopla_processo(&mut cmd); // grupo próprio → não morre com o processo atual
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

/// Nomes do binário do CLI: o canônico primeiro, e o do interregno como rede.
///
/// Houve uma janela curta em que o app se chamou Overflow. Máquina que instalou ali
/// pode ter só aquele binário — e sem achá-lo o botão que abre o terminal não faria
/// nada, sem dizer por quê.
pub(crate) const CLI_BINS: [&str; 2] = ["schematize", "overflow"];

/// Localiza o binário do CLI pra montar o comando do terminal: primeiro um irmão do
/// executável atual (instalação da casa põe os dois lado a lado), senão o do PATH.
pub(crate) fn schematize_bin() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for nome in CLI_BINS {
                let cand = dir.join(nome);
                if cand.is_file() {
                    return cand.to_string_lossy().into_owned();
                }
            }
        }
    }
    // Nada ao lado: escolhe pelo PATH, canônico primeiro. Sem nada, devolve o canônico —
    // o erro que o usuário vê passa a ser "schematize: not found", que é acionável.
    CLI_BINS
        .iter()
        .find(|n| which_bin(n))
        .unwrap_or(&CLI_BINS[0])
        .to_string()
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

/// Seta o app_id (Wayland) / WM_CLASS (X11) da janela ANTES de criá-la, instalando
/// um backend winit com `with_window_attributes_hook`. No Wayland (KDE/GNOME) o compositor IGNORA o
/// ícone-buffer da janela e casa pelo app_id ao `<nome>.desktop` pra achar o ícone — sem isto
/// o dock mostra um fallback genérico ("W"). Linux só; macOS/Windows pegam o ícone do bundle/.exe.
/// Qual app_id anunciar, dado o nome do executável em uso. PURA e testada.
///
/// Vale um teste porque a falha é silenciosa e chata: app_id que não casa com nenhum
/// `.desktop` não quebra nada — só faz o dock mostrar um ícone genérico, e ninguém
/// liga o sintoma à causa. Já aconteceu neste app.
///
/// Um binário `overflow-gui` remanescente do interregno anuncia o id dele, que é o
/// que casa com o `.desktop` que aquela instalação escreveu. Tudo o mais cai no
/// canônico.
// Só o caminho Wayland/X11 consome isto — em Windows vira aviso de código morto, e ruído
// esconde sinal no log do CI.
#[cfg(unix)]
pub(crate) fn app_id_de(exe: Option<&str>) -> &'static str {
    match exe {
        Some("overflow-gui") => "overflow-gui",
        _ => "schematize-gui",
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn set_window_app_id() {
    use i_slint_backend_winit::winit::platform::wayland::WindowAttributesExtWayland;
    use i_slint_backend_winit::winit::platform::x11::WindowAttributesExtX11;
    // O app_id sai do NOME DO PRÓPRIO EXECUTÁVEL, não de uma constante.
    //
    // São dois binários (`overflow-gui` e `schematize-gui`) e dois `.desktop`. O
    // ambiente casa a janela ao lançador PELO app_id pra achar o ícone — se o binário
    // antigo anunciasse o app_id novo, o `.desktop` antigo deixaria de casar e o dock
    // voltaria ao ícone genérico. Cada nome anuncia o seu, e os dois ficam certos.
    let id: &'static str = app_id_de(
        std::env::current_exe().ok().as_deref().and_then(|p| p.file_name()).and_then(|s| s.to_str()),
    );
    let built = i_slint_backend_winit::Backend::builder()
        .with_window_attributes_hook(move |attrs| {
            // `general` = app_id (Wayland) / res_class (X11). Bate com <id>.desktop.
            let attrs = WindowAttributesExtWayland::with_name(attrs, id, id);
            WindowAttributesExtX11::with_name(attrs, id, id)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada binário anuncia o SEU app_id: é o que mantém os dois `.desktop` casando
    /// com seus ícones durante a coexistência dos nomes.
    #[test]
    fn app_id_acompanha_o_nome_do_binario() {
        assert_eq!(app_id_de(Some("schematize-gui")), "schematize-gui");
        assert_eq!(app_id_de(Some("overflow-gui")), "overflow-gui");
        // Qualquer outra coisa (renomeado à mão, rodado do target/) cai no CANÔNICO.
        assert_eq!(app_id_de(Some("schematize-gui-old")), "schematize-gui");
        assert_eq!(app_id_de(None), "schematize-gui");
    }
}
