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

// O `market_bin` (o binário HEADLESS do gestor) saiu daqui junto com a lista do mercado: era
// só ela que o chamava, e a lista é da janela do market agora (ADR-0012). Quem precisa do
// gestor headless é aquela janela, que o resolve com a mesma regra do lado de lá.

/// Nome da JANELA do market — a que a aba do Mercado deste hub passa a abrir (ADR-0012).
///
/// O crate dela ainda se chama `schematize-updater-gui` por ser o endereço histórico; o
/// binário é este. Nome de binário que não bate com o dono já deixou o update do deployer
/// morto por um release inteiro, e nada dá erro nesse caso.
pub(crate) const MARKET_GUI_BINS: [&str; 1] = ["schematize-market-gui"];

/// Nome da JANELA do deployer — a que as telas de Chaves SSH e de Hosts passam a abrir.
///
/// O deployer é um app à parte desde o ADR-0010, e as telas dele saíram deste hub junto com os
/// módulos `sshkeys`/`vps`/`mcp` que as alimentavam. O que fica aqui é o caminho até lá.
pub(crate) const DEPLOYER_GUI_BINS: [&str; 1] = ["schematize-deployer-gui"];

/// Nome da JANELA do database — a que a tela de Banco de dados passa a abrir.
///
/// O `schematize-database` é um app à parte desde o ADR-0018 (E1 da extradição), e a janela
/// dele mora no MESMO repo do CLI dele (ADR-0020). Por isso o nome tem o sufixo `-gui` e não
/// um repo próprio por trás: os dois binários saem do mesmo commit.
pub(crate) const DATABASE_GUI_BINS: [&str; 1] = ["schematize-database-gui"];

/// Nome da JANELA do git — a que a tela de Git passa a abrir (ADR-0019, ADR-0020).
pub(crate) const GIT_GUI_BINS: [&str; 1] = ["schematize-git-gui"];

/// Nome da JANELA do optimizer — a que a tela de Disco passa a abrir (E3 da extradição).
pub(crate) const OPTIMIZER_GUI_BINS: [&str; 1] = ["schematize-optimizer-gui"];

/// Nome da JANELA das skills — a que a aba de Skills passa a abrir (E5, ADR-0012 F4).
pub(crate) const SKILLS_GUI_BINS: [&str; 1] = ["schematize-skills-gui"];

/// Nome da JANELA do overdev — a que a aba Overdev passa a abrir (E6 M5, ADR-0018). É o
/// SEGUNDO binário do repo do CLI dele (ADR-0020), então sai do mesmo commit que o
/// `schematize-overdev`.
pub(crate) const OVERDEV_GUI_BINS: [&str; 1] = ["schematize-overdev-gui"];

/// **O quê:** localiza um binário irmão para montar comando de terminal — primeiro ao lado do
/// executável atual, senão no `$PATH`, senão devolve o nome canônico.
///
/// **Onde:** [`schematize_bin`] e [`market_bin`].
///
/// ## Por que o "ao lado" vem PRIMEIRO, e por que esta função é uma só
///
/// A janela aberta pelo lançador do desktop recebe um **PATH mínimo** — sem `~/.cargo/bin`,
/// que é onde a instalação da casa põe tudo. Um comando montado com o nome puro morre ali com
/// `command not found`, e o terminal que a GUI abre HERDA esse PATH, então o erro chega ao
/// usuário como uma linha crua do bash em vez de algo acionável.
///
/// Foi o que aconteceu: `run_app_install` cravava `schematize-market install <app>` com o nome
/// puro, enquanto o caminho de LINGUAGEM, ao lado, já resolvia o binário. A correção de então
/// foi pontual num dos dois — e o outro seguiu quebrado até alguém clicar nele.
///
/// Por isso a resolução é UMA função e não duas parecidas: com duas, consertar uma continua
/// deixando a outra para trás, que é exatamente a forma deste bug.
fn bin_irmao(nomes: &[&str]) -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for nome in nomes {
                let cand = dir.join(nome);
                if cand.is_file() {
                    return cand.to_string_lossy().into_owned();
                }
            }
        }
    }
    // Nada ao lado: escolhe pelo PATH, canônico primeiro. Sem nada, devolve o canônico — o
    // erro que o usuário vê passa a ser "<nome>: not found", que ao menos nomeia o que falta.
    nomes.iter().find(|n| which_bin(n)).unwrap_or(&nomes[0]).to_string()
}

/// Localiza o binário do CLI pra montar o comando do terminal: primeiro um irmão do
/// executável atual (instalação da casa põe os dois lado a lado), senão o do PATH.
pub(crate) fn schematize_bin() -> String {
    bin_irmao(&CLI_BINS)
}

/// **O quê:** o caminho do binário do MARKET, ou `None` se ele não estiver instalado.
///
/// **Onde:** [`crate::marketlink::ler`], que pergunta ao market quais métodos de instalação
/// existem nesta máquina.
///
/// **`Option`, e não o nome canônico como o `bin_irmao` devolve**, pela mesma razão do
/// [`market_gui_bin`]: o destino aqui é um `Command::new`, não um comando de terminal. Sem o
/// market, o que se quer é degradar em silêncio (o modal não oferece o environment) — e não
/// tentar executar um nome que não existe para descobrir isso pelo erro.
pub(crate) fn market_bin_opcional() -> Option<String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let c = dir.join("schematize-market");
            if c.is_file() {
                return Some(c.to_string_lossy().into_owned());
            }
        }
    }
    if which_bin("schematize-market") {
        return Some("schematize-market".to_string());
    }
    None
}

/// **O quê:** o caminho ABSOLUTO da janela de um app, ou `None` se ela não estiver instalada.
///
/// **Onde:** as três telas delegadas — Mercado, Chaves SSH e Hosts.
///
/// **Por que `Option` e não a mesma regra do [`bin_irmao`].** Aquela devolve o nome canônico
/// quando não acha nada, porque o destino dela é um comando de terminal, onde
/// `schematize-market: not found` ao menos NOMEIA o que falta. Aqui o destino é um `spawn`
/// silencioso: o processo não sobe, nada aparece, e a pessoa fica olhando um botão que não faz
/// nada. Saber que ela não está instalada é o que permite a tela OFERECER instalá-la, em vez
/// de fingir que o clique funcionou.
///
/// **Por que UMA função para as duas janelas:** com duas parecidas, um conserto numa deixa a
/// outra para trás — a forma exata do bug que este repo já teve no `run_app_install`, onde o
/// caminho de app cravava o nome puro do gestor enquanto o de linguagem, ao lado, resolvia.
fn gui_de_app(nomes: &[&str]) -> Option<String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for nome in nomes {
                let cand = dir.join(nome);
                if cand.is_file() {
                    return Some(cand.to_string_lossy().into_owned());
                }
            }
        }
    }
    let path = std::env::var("PATH").unwrap_or_default();
    nomes.iter().find_map(|n| achar_no_path(n, &path)).map(|p| p.to_string_lossy().into_owned())
}

/// **O quê:** o caminho da janela do MARKET. **Onde:** a aba do Mercado.
pub(crate) fn market_gui_bin() -> Option<String> {
    gui_de_app(&MARKET_GUI_BINS)
}

/// **O quê:** o caminho da janela do DEPLOYER. **Onde:** as telas de Chaves SSH e de Hosts.
pub(crate) fn deployer_gui_bin() -> Option<String> {
    gui_de_app(&DEPLOYER_GUI_BINS)
}

/// **O quê:** o caminho da janela do DATABASE. **Onde:** a tela de Banco de dados.
pub(crate) fn database_gui_bin() -> Option<String> {
    gui_de_app(&DATABASE_GUI_BINS)
}

/// **O quê:** o caminho da janela do GIT. **Onde:** a tela de Git.
pub(crate) fn git_gui_bin() -> Option<String> {
    gui_de_app(&GIT_GUI_BINS)
}

/// **O quê:** o caminho da janela do OPTIMIZER. **Onde:** a tela de Disco.
pub(crate) fn optimizer_gui_bin() -> Option<String> {
    gui_de_app(&OPTIMIZER_GUI_BINS)
}

/// **O quê:** o caminho do binário HEADLESS das skills, ou `None` se não está instalado.
///
/// **Onde:** [`crate::skillactions`], que lê os botões declarados pelas skills.
pub(crate) fn skills_bin() -> Option<String> {
    gui_de_app(&["schematize-skills"])
}

/// **O quê:** o caminho do binário HEADLESS do git, ou `None` se ele não está instalado.
///
/// **Onde:** [`crate::gitlog`], que lê o histórico de commits da aba Overdev.
///
/// **É `Option` e não o nome canônico**, ao contrário do [`bin_irmao`]: aqui o destino é um
/// `spawn` silencioso cujo resultado alimenta uma lista secundária. Sem o app, a lista sai
/// vazia e o resto da aba continua — que é o piso 10. Um nome que não resolve daria o mesmo
/// resultado com um erro de "not found" no meio do caminho.
pub(crate) fn git_bin() -> Option<String> {
    gui_de_app(&["schematize-git"])
}

/// **O quê:** o caminho da janela das SKILLS. **Onde:** a aba de Skills.
pub(crate) fn skills_gui_bin() -> Option<String> {
    gui_de_app(&SKILLS_GUI_BINS)
}

/// **O quê:** procura `nome` nos diretórios de `path_var` (formato do `$PATH`) e devolve o
/// caminho ABSOLUTO de um arquivo executável, ou `None`. PURA — recebe o PATH em vez de lê-lo.
///
/// **Onde:** [`gui_de_app`] (com o `$PATH` do processo) e os testes (com um PATH fabricado).
///
/// **Por que absoluto.** O `Command::new("nome")` resolve pelo PATH de QUEM spawna, e a janela
/// aberta pelo lançador do desktop tem um PATH mínimo. Resolver aqui, uma vez, e passar o
/// caminho inteiro ao `spawn` tira essa dúvida do clique: ou achou, e abre; ou não achou, e a
/// tela diz que falta — em vez de um botão que falha calado.
///
/// **Por que recebe o PATH.** Mexer em `std::env` num teste afeta os outros que rodam em
/// paralelo; com a string como argumento, o teste "app ausente" não depende da máquina onde
/// roda. Diretório relativo ou vazio no PATH é ignorado: significaria "o cwd", e o cwd de um
/// hub aberto pelo desktop não é lugar de onde se quer executar nada.
pub(crate) fn achar_no_path(nome: &str, path_var: &str) -> Option<PathBuf> {
    path_var
        .split(':')
        .filter(|d| Path::new(d).is_absolute())
        .map(|d| Path::new(d).join(nome))
        .find(|c| e_executavel(c))
}

/// **O quê:** `c` é um arquivo regular com bit de execução (Unix) — ou só um arquivo (outros
/// sistemas, onde a extensão manda). **Onde:** [`achar_no_path`].
fn e_executavel(c: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(c) else { return false };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// **O quê:** o caminho do binário HEADLESS do overdev, ou `None` se o app não está instalado.
///
/// **Onde:** [`crate::overdevcli`] — a porta única do hub para o `schematize-overdev` (leituras
/// `--json` e ações). `Option` e não o nome canônico: o destino é um `spawn` cujo motivo de falha
/// precisa chegar à tela («o app não está instalado» + o comando), não um `not found` cru.
pub(crate) fn overdev_bin() -> Option<String> {
    gui_de_app(&["schematize-overdev"])
}

/// **O quê:** o caminho da janela do OVERDEV. **Onde:** a aba Overdev.
pub(crate) fn overdev_gui_bin() -> Option<String> {
    gui_de_app(&OVERDEV_GUI_BINS)
}

/// **O quê:** abre a janela do overdev SOBRE um projeto, desacoplada deste processo. `false`
/// se não deu.
///
/// **Onde:** os botões da faixa da aba Overdev. A janela trabalha sobre um projeto, e o hub é
/// quem sabe qual é (a barra do cabeçalho). O contrato dela (`schematize-overdev-gui`) é
/// `--dir <projeto>` e `--aba perguntas|checklist|historico|execucao|caixa|notas`; o cwd também vai no projeto,
/// para a janela que ignorar a flag ainda abrir no lugar certo.
///
/// **`aba` vazia = sem `--aba`** (a janela abre na inicial). **Sem projeto**, sem `--dir`: ela
/// decide o que fazer sem ele.
pub(crate) fn abrir_gui_no_projeto(
    bin: Option<String>,
    projeto: Option<&Path>,
    aba: Option<&str>,
) -> bool {
    let Some(bin) = bin else { return false };
    let mut cmd = std::process::Command::new(bin);
    if let Some(dir) = projeto.filter(|d| d.is_dir()) {
        cmd.arg("--dir").arg(dir).current_dir(dir);
    }
    if let Some(a) = aba.filter(|a| !a.is_empty()) {
        cmd.arg("--aba").arg(a);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    desacopla_processo(&mut cmd);
    cmd.spawn().is_ok()
}

/// **O quê:** abre a janela de um app, desacoplada deste processo. `false` se não deu.
///
/// **Onde:** os botões das três telas delegadas. `aba` é a flag que faz a janela abrir onde a
/// pessoa pediu — sem ela, quem clica em "Chaves SSH" cai na primeira aba da janela e tem de
/// clicar de novo, um clique a mais para chegar onde já tinha pedido para ir.
///
/// **`stdin`/`stdout`/`stderr` em `null` e sem `wait`:** a janela é outro app, não um filho
/// deste. Herdar os descritores faria a saída dela se misturar à deste hub; esperar por ela
/// travaria o event loop enquanto estivesse aberta.
pub(crate) fn abrir_gui(bin: Option<String>, aba: Option<&str>) -> bool {
    let Some(bin) = bin else { return false };
    let mut cmd = std::process::Command::new(bin);
    if let Some(a) = aba {
        cmd.arg(a);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
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
        std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str()),
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

    /// Diretório temporário ÚNICO por teste (rodam em paralelo).
    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gui-sysenv-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("criar tmp do teste");
        d
    }

    /// Cria um script em `dir/nome` com `corpo`, com ou sem bit de execução.
    #[cfg(unix)]
    fn script(dir: &Path, nome: &str, corpo: &str, exec: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(nome);
        std::fs::write(&p, corpo).expect("gravar script");
        let modo = if exec { 0o755 } else { 0o644 };
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(modo)).expect("chmod");
        p
    }

    /// **App ausente é ESTADO, não erro:** com um PATH sem o binário, a resposta é `None` — e é
    /// isso que faz a aba mostrar «não instalado» em vez de um botão que não abre nada.
    #[test]
    #[cfg(unix)]
    fn path_sem_o_binario_nao_acha() {
        let vazio = tmp("vazio");
        let outro = tmp("outro");
        script(&outro, "outra-coisa", "#!/bin/sh\n", true);
        let path = format!("{}:{}", vazio.display(), outro.display());
        assert_eq!(achar_no_path("schematize-overdev-gui", &path), None);
        assert_eq!(achar_no_path("schematize-overdev-gui", ""), None, "PATH vazio");
    }

    /// Achou: o caminho devolvido é ABSOLUTO e aponta para o arquivo — nunca o nome puro.
    #[test]
    #[cfg(unix)]
    fn path_com_o_binario_devolve_o_caminho_absoluto() {
        let d = tmp("presente");
        let esperado = script(&d, "schematize-overdev-gui", "#!/bin/sh\n", true);
        let path = format!("/nao/existe:{}", d.display());
        let achou = achar_no_path("schematize-overdev-gui", &path).expect("devia achar");
        assert!(achou.is_absolute());
        assert_eq!(achou, esperado);
    }

    /// Arquivo SEM bit de execução não é app instalado: o `spawn` falharia com «permission
    /// denied», e a tela ofereceria abrir o que não abre. E PATH relativo (`.`, vazio) não vale.
    #[test]
    #[cfg(unix)]
    fn sem_bit_de_execucao_e_path_relativo_nao_contam() {
        let d = tmp("semexec");
        script(&d, "schematize-overdev-gui", "#!/bin/sh\n", false);
        assert_eq!(achar_no_path("schematize-overdev-gui", &d.display().to_string()), None);
        // Relativo: o mesmo arquivo executável, mas apontado por «.» e por entrada vazia.
        let r = tmp("relativo");
        script(&r, "schematize-overdev-gui", "#!/bin/sh\n", true);
        // `.` e a entrada vazia significam «o cwd», e o cwd de um hub aberto pelo desktop não
        // é lugar de onde se quer executar nada. O teste não mexe no cwd do processo.
        assert_eq!(achar_no_path("schematize-overdev-gui", ".:"), None);
    }

    /// A janela abre com `--dir`, `--aba` e o cwd NO PROJETO — é assim que ela sabe sobre qual trabalhar. O script
    /// grava o `pwd` que recebeu; o teste espera o arquivo (o spawn é assíncrono).
    #[test]
    #[cfg(unix)]
    fn a_janela_abre_com_o_cwd_no_projeto() {
        let bin_dir = tmp("bin");
        let projeto = tmp("projeto");
        let saida = tmp("saida").join("cwd.txt");
        let corpo = format!("#!/bin/sh\n{{ pwd; echo \"$@\"; }} > '{}'\n", saida.display());
        let bin = script(&bin_dir, "schematize-overdev-gui", &corpo, true);
        let ok = abrir_gui_no_projeto(
            Some(bin.display().to_string()),
            Some(&projeto),
            Some("perguntas"),
        );
        assert!(ok, "o spawn devia dar certo");
        let mut lido = String::new();
        for _ in 0..100 {
            if let Ok(t) = std::fs::read_to_string(&saida) {
                if !t.trim().is_empty() {
                    lido = t;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let real = std::fs::canonicalize(&projeto).expect("canonicalize");
        let mut linhas = lido.lines();
        let cwd = linhas.next().expect("cwd gravado");
        assert_eq!(std::fs::canonicalize(cwd).expect("cwd existe"), real);
        // O contrato da janela: `--dir <projeto> --aba <aba>`.
        let args = linhas.next().expect("args gravados");
        assert_eq!(args, format!("--dir {} --aba perguntas", projeto.display()));
    }

    /// Sem binário não há o que abrir: `false`, sem panicar — a tela volta ao estado «ausente».
    #[test]
    fn abrir_sem_binario_devolve_false() {
        assert!(!abrir_gui_no_projeto(None, None, None));
        assert!(!abrir_gui_no_projeto(
            Some("/nao/existe/schematize-overdev-gui".into()),
            None,
            None
        ));
    }
}
