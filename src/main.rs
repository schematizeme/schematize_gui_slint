//! schematize — aba Skills em Slint (1º incremento REAL da migração egui→Slint).
//!
//! O quê: a aba Skills como GESTOR de verdade. Reusa a LÓGICA do crate irmão
//! `schematize` (sem a GUI egui): catálogo (`registry::catalog`), versões e
//! ações (`skills::installed_version` / `resolve_latest` / `install` / `remove`),
//! e i18n (`schematize::i18n`, 11 locales). O visual é Slint (ver `ui/app.slint`).
//!
//! Assíncrono: `resolve_latest` e as ações (instalar/remover) são REDE/IO — rodam
//! em threads e devolvem resultado à UI via `slint::invoke_from_event_loop` +
//! `Weak<AppWindow>::upgrade` (o padrão do Slint pra thread→UI). O event loop
//! nunca bloqueia. As ações em massa disparam em PARALELO (thread::scope),
//! espelhando o `run_batch` do egui; o lib serializa o `state.json` (STATE_LOCK).
//!
//! Escopo deste incremento: SÓ a aba Skills funcional. Overdev/Grafo ficam como
//! placeholders "em breve" na barra de abas (próximos incrementos).

use schematize::i18n::{self, t, tf};
use schematize::registry::{self, Item};
use schematize::{environments, skills, util};
use slint::{Model, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

slint::include_modules!(); // gera AppWindow, SkillRow, Theme, L a partir de ui/app.slint

// ---------------------------------------------------------------------------
// Detecção de ambiente gráfico (Wayland vs X11 + desktop). Só loga — o backend
// winit do Slint escolhe sozinho o transporte certo em runtime.
// ---------------------------------------------------------------------------
fn detect_display_env() {
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
// i18n: injeta TODOS os rótulos estáticos da UI no `global L` do .slint. Nada de
// texto hardcoded no Slint — as strings vêm de `schematize::i18n` (11 locales).
// ---------------------------------------------------------------------------
fn install_i18n(app: &AppWindow) {
    let l = app.global::<L>();
    l.set_window_title("schematize".into());
    l.set_subtitle(t("app.tagline").into());
    l.set_theme_light(t("gui.theme_light").into());
    l.set_theme_dark(t("gui.theme_dark").into());
    l.set_check(t("gui.check").into());
    l.set_update_all(t("gui.update_all").into());
    l.set_update_installed_only(t("gui.update_installed_only").into());
    l.set_update_sel(t("gui.update_sel").into());
    l.set_install_sel_market(t("gui.install_sel_market").into());
    l.set_remove_sel(t("gui.remove_sel").into());
    l.set_sel_label(t("gui.sel_label").into());
    l.set_sel_all(t("gui.sel_all").into());
    l.set_sel_pending(t("gui.sel_pending").into());
    l.set_sel_none(t("gui.sel_none").into());
    l.set_col_skill(t("gui.col_skill").into());
    l.set_col_author(t("gui.col_author").into());
    l.set_col_installed(t("gui.col_installed").into());
    l.set_col_latest(t("gui.col_latest").into());
    l.set_col_state(t("gui.col_state").into());
    l.set_col_actions(t("gui.col_actions").into());
    // Tooltip do selo verificado (só o check + hover; sem texto ao lado).
    l.set_verified(t("gui.verified_badge").into());
    l.set_tab_installed(t("gui.tab_installed").into());
    l.set_tab_marketplace(t("gui.tab_marketplace").into());
    l.set_tab_overdev(t("gui.tab_overdev").into());
    l.set_tab_graph(t("gui.tab_graph").into());
    l.set_coming_soon(t("gui.coming_soon").into());
    l.set_act_install(t("gui.install").into());
    l.set_act_update(t("gui.update").into());
    l.set_act_remove(t("gui.uninstall").into());
    // aba Environments
    l.set_tab_environments(t("gui.tab_environments").into());
    l.set_env_intro(t("gui.env_intro").into());
    l.set_env_method(t("gui.env_method").into());
    l.set_env_no_methods(t("gui.env_no_methods").into());
    // modal de instalação do Marketplace
    l.set_mp_recommends_note(t("gui.mp_recommends_note").into());
    l.set_mp_env_note(t("gui.mp_env_note").into());
    l.set_mp_confirm(t("gui.mp_confirm").into());
    l.set_mp_cancel(t("gui.mp_cancel").into());
}

// ---------------------------------------------------------------------------
// Estado derivado (missing/outdated/current/loading) + rótulo traduzido.
// ---------------------------------------------------------------------------
fn compute_state(installed: &Option<String>, latest: &Option<String>) -> (String, String) {
    match (installed, latest) {
        // Não instalada — mesmo com latest desconhecido, dá pra instalar.
        (None, _) => ("missing".into(), t("common.not_installed")),
        // Instalada, mas ainda resolvendo o latest (rede) → spinner.
        (Some(_), None) => ("loading".into(), "…".into()),
        (Some(i), Some(l)) if i == l => ("current".into(), t("common.current")),
        // Desatualizada: "UPDATE (X→Y)".
        (Some(i), Some(l)) => ("outdated".into(), format!("{} ({}→{})", t("common.update"), i, l)),
    }
}

// ---------------------------------------------------------------------------
// Montagem inicial do modelo (cabeçalhos de categoria + skills). Retorna as
// linhas E o Item alinhado a cada linha (None nos cabeçalhos), pra as ações.
// ---------------------------------------------------------------------------
/// Categoria normalizada de um item (vazio → "language").
fn category_of(it: &Item) -> &str {
    if it.category.is_empty() { "language" } else { it.category.as_str() }
}

/// Cabeçalho de categoria de UMA página (page 0 = Instaladas, 1 = Marketplace).
/// `count` é preenchido/atualizado por `recompute_headers` (esconde vazios).
fn header_row(label: &str, cat: &str, page: i32) -> SkillRow {
    SkillRow {
        is_header: true,
        header_label: label.into(),
        page,
        count: 0,
        category: cat.into(),
        slug: SharedString::new(),
        author: SharedString::new(),
        author_url: SharedString::new(),
        installed: SharedString::new(),
        latest: SharedString::new(),
        state: SharedString::new(),
        state_label: SharedString::new(),
        verified: false,
        selected: false,
        busy: false,
        op_label: SharedString::new(),
        op_error: false,
    }
}

/// Linha inicial de uma skill: instalada lida do disco (rápido), latest ainda
/// "…" (resolvido depois, assíncrono). Estado derivado do que já se sabe.
fn skill_row(it: &Item) -> SkillRow {
    let author = it.sponsor.as_ref().map(|s| s.name.clone()).unwrap_or_default();
    let author_url = it.sponsor.as_ref().map(|s| s.url.clone()).unwrap_or_default();
    let installed = skills::installed_version(it);
    let latest: Option<String> = None; // resolvido assíncrono após subir a janela
    let (state, state_label) = compute_state(&installed, &latest);
    SkillRow {
        is_header: false,
        header_label: SharedString::new(),
        page: 0,
        count: 0,
        category: category_of(it).into(),
        slug: it.slug.clone().into(),
        author: author.into(),
        author_url: author_url.into(),
        installed: installed.unwrap_or_else(|| "—".into()).into(),
        latest: "…".into(),
        state: state.into(),
        state_label: state_label.into(),
        verified: it.verified,
        selected: false,
        busy: false,
        op_label: SharedString::new(),
        op_error: false,
    }
}

/// Ordena os itens em grupos (base, language, external). Por categoria emite
/// DOIS cabeçalhos (Instaladas page=0 e Marketplace page=1) seguidos das skills;
/// a página ativa mostra o cabeçalho certo e as skills cujo estado casa (o Slint
/// filtra por `state`). Devolve o Item por linha (None nos cabeçalhos).
fn build_rows(items: &[Item]) -> (Vec<SkillRow>, Vec<Option<Item>>) {
    let groups = [
        ("base", t("gui.cat_base")),
        ("language", t("gui.cat_language")),
        ("external", t("gui.cat_external")),
    ];
    let mut rows = Vec::new();
    let mut row_items: Vec<Option<Item>> = Vec::new();
    for (cat, label) in groups {
        let group: Vec<&Item> = items.iter().filter(|it| category_of(it) == cat).collect();
        if group.is_empty() {
            continue;
        }
        // page 0 = Instaladas, page 1 = Marketplace — só um aparece por vez.
        rows.push(header_row(&label, cat, 0));
        row_items.push(None);
        rows.push(header_row(&label, cat, 1));
        row_items.push(None);
        for it in group {
            rows.push(skill_row(it));
            row_items.push(Some(it.clone()));
        }
    }
    (rows, row_items)
}

// ---------------------------------------------------------------------------
// Reconta os cabeçalhos: cada cabeçalho (page, categoria) ganha o nº de skills
// que estão AGORA na sua página (Instaladas = state != "missing"; Marketplace =
// state == "missing"). count==0 → o Slint esconde o cabeçalho. Roda no event
// loop (só usa o modelo do app; nada de dados !Send).
// ---------------------------------------------------------------------------
fn recompute_headers(app: &AppWindow) {
    let rows = app.get_rows();
    let n = rows.row_count();
    for i in 0..n {
        let Some(mut h) = rows.row_data(i) else { continue };
        if !h.is_header {
            continue;
        }
        let mut count = 0;
        for j in 0..n {
            if let Some(r) = rows.row_data(j) {
                if r.is_header || r.category != h.category {
                    continue;
                }
                let missing = r.state == "missing";
                // page 0 = Instaladas (não-missing); page 1 = Marketplace (missing).
                if (h.page == 1 && missing) || (h.page == 0 && !missing) {
                    count += 1;
                }
            }
        }
        if h.count != count {
            h.count = count;
            rows.set_row_data(i, h);
        }
    }
}

// ---------------------------------------------------------------------------
// Status global (contagem de pendências) — mesma regra do egui.
// ---------------------------------------------------------------------------
fn update_status(app: &AppWindow) {
    let rows = app.get_rows();
    let mut pending = 0usize;
    for i in 0..rows.row_count() {
        if let Some(r) = rows.row_data(i) {
            if !r.is_header && (r.state == "missing" || r.state == "outdated") {
                pending += 1;
            }
        }
    }
    let status = if pending == 0 {
        t("gui.all_uptodate")
    } else {
        tf("gui.n_pending", &[("n", &pending.to_string())])
    };
    app.set_status(status.into());
}

// ---------------------------------------------------------------------------
// thread→UI: posta a atualização de versões (installed + latest) de uma linha.
// ---------------------------------------------------------------------------
fn post_versions(weak: Weak<AppWindow>, idx: usize, installed: Option<String>, latest: Option<String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                let (state, label) = compute_state(&installed, &latest);
                r.installed = installed.clone().unwrap_or_else(|| "—".into()).into();
                r.latest = latest.clone().unwrap_or_else(|| "?".into()).into();
                r.state = state.into();
                r.state_label = label.into();
                rows.set_row_data(idx, r);
            }
            update_status(&app);
            recompute_headers(&app);
        }
    });
}

/// thread→UI: marca uma linha como ocupada (operação em andamento) com rótulo.
fn post_row_busy(weak: Weak<AppWindow>, idx: usize, label: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                r.busy = true;
                r.op_label = label.into();
                r.op_error = false;
                rows.set_row_data(idx, r);
            }
        }
    });
}

/// thread→UI: resultado de uma operação numa linha. Instalar → instalada=latest
/// (o release baixado É o latest) e estado "current"; remover → não instalada.
/// Erro → mantém e mostra o rótulo em warn.
fn post_row_result(weak: Weak<AppWindow>, idx: usize, install: bool, res: Result<String, String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                r.busy = false;
                match res {
                    Ok(ver) => {
                        r.op_label = SharedString::new();
                        r.op_error = false;
                        if install {
                            r.installed = ver.clone().into();
                            r.latest = ver.into();
                            r.state = "current".into();
                            r.state_label = t("common.current").into();
                        } else {
                            r.installed = "—".into();
                            r.state = "missing".into();
                            r.state_label = t("common.not_installed").into();
                        }
                    }
                    Err(e) => {
                        r.op_label = tf("err.prefix", &[("e", &e)]).into();
                        r.op_error = true;
                    }
                }
                rows.set_row_data(idx, r);
            }
            update_status(&app);
            recompute_headers(&app);
        }
    });
}

/// thread→UI: fim do lote — solta o `busy` global e mostra o toast final.
fn post_batch_done(weak: Weak<AppWindow>, ok: usize, err: usize) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            app.set_busy(false);
            let toast = tf("gui.batch_done", &[("ok", &ok.to_string()), ("err", &err.to_string())]);
            app.set_status(toast.into());
            recompute_headers(&app);
        }
    });
}

// ---------------------------------------------------------------------------
// Resolução assíncrona do latest de UMA skill (rede). Detached: reusa install/
// check. Re-lê a instalada (barato) pra refletir mudanças de disco.
// ---------------------------------------------------------------------------
fn spawn_resolve(weak: Weak<AppWindow>, idx: usize, item: Item) {
    std::thread::spawn(move || {
        let installed = skills::installed_version(&item);
        let latest = skills::resolve_latest(&item).ok();
        post_versions(weak, idx, installed, latest);
    });
}

/// Dispara a resolução do latest de todas as skills em paralelo (uma thread por
/// skill; são poucas). Antes, zera a coluna latest de volta pra "…".
fn kick_resolve_all(weak: &Weak<AppWindow>, row_items: &Rc<Vec<Option<Item>>>) {
    if let Some(app) = weak.upgrade() {
        let rows = app.get_rows();
        for (idx, maybe) in row_items.iter().enumerate() {
            if let Some(it) = maybe {
                if let Some(mut r) = rows.row_data(idx) {
                    r.latest = "…".into();
                    if r.installed != "—" {
                        r.state = "loading".into();
                        r.state_label = "…".into();
                    }
                    rows.set_row_data(idx, r);
                }
                spawn_resolve(weak.clone(), idx, it.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ações em massa/paralelo (espelha o run_batch do egui). ops = (idx, install?, Item).
// ---------------------------------------------------------------------------
fn run_batch(weak: Weak<AppWindow>, ops: Vec<(usize, bool, Item)>) {
    if ops.is_empty() {
        return;
    }
    if let Some(app) = weak.upgrade() {
        if app.get_busy() {
            return; // já tem lote rodando
        }
        app.set_busy(true);
    }
    std::thread::spawn(move || {
        let ok = AtomicUsize::new(0);
        let err = AtomicUsize::new(0);
        // marca cada linha como ocupada antes de começar
        for (idx, install, _) in &ops {
            let label = if *install { t("gui.installing") } else { t("gui.removing") };
            post_row_busy(weak.clone(), *idx, label);
        }
        // executa TODAS em paralelo; o lib serializa o state.json (STATE_LOCK).
        std::thread::scope(|sc| {
            for (idx, install, item) in &ops {
                let weak = weak.clone();
                let ok = &ok;
                let err = &err;
                sc.spawn(move || {
                    let res: Result<String, String> = if *install {
                        skills::install(item)
                    } else {
                        skills::remove(item).map(|_| String::new())
                    };
                    if res.is_ok() {
                        ok.fetch_add(1, Ordering::SeqCst);
                    } else {
                        err.fetch_add(1, Ordering::SeqCst);
                    }
                    post_row_result(weak, *idx, *install, res);
                });
            }
        });
        post_batch_done(weak, ok.load(Ordering::SeqCst), err.load(Ordering::SeqCst));
    });
}

/// Coleta ops (idx, install?, Item) das linhas que casam com o predicado.
/// `install=true` → instalar/atualizar; `install=false` → remover.
fn collect_ops(
    app: &AppWindow,
    row_items: &Rc<Vec<Option<Item>>>,
    install: bool,
    pred: impl Fn(&SkillRow) -> bool,
) -> Vec<(usize, bool, Item)> {
    let rows = app.get_rows();
    let mut ops = Vec::new();
    for (idx, maybe) in row_items.iter().enumerate() {
        if let Some(it) = maybe {
            if let Some(r) = rows.row_data(idx) {
                if !r.is_header && pred(&r) {
                    ops.push((idx, install, it.clone()));
                }
            }
        }
    }
    ops
}

/// NÃO instalada (pertence ao Marketplace).
fn is_missing(r: &SkillRow) -> bool {
    r.state == "missing"
}
/// Instalada (pertence a Instaladas) — qualquer estado que não seja "missing".
fn is_installed(r: &SkillRow) -> bool {
    r.state != "missing"
}
/// Instalada E desatualizada (installed Some E latest > installed). É o ÚNICO
/// alvo de "Atualizar tudo"/"Atualizar selecionadas": jamais instala nova.
fn is_outdated(r: &SkillRow) -> bool {
    r.state == "outdated"
}

// ===========================================================================
// ENVIRONMENTS — gestão dos runtimes de linguagem (aba 2).
// A GUI só MONTA o comando e ABRE UM TERMINAL rodando `schematize env …`; o plano
// exato + consentimento (e o sudo) acontecem no terminal (honesto). NUNCA executa
// o instalador de environment de dentro do processo da GUI.
// ===========================================================================

/// Rótulo de status de um environment — mesmas chaves i18n que o `list()` do CLI usa.
fn env_status_label(le: &environments::LangEnv) -> String {
    if let Some(m) = le.installed {
        tf("env.installed_via", &[("method", m.slug())])
    } else if le.runtime_present {
        t("env.installed")
    } else {
        t("env.not_installed")
    }
}

/// Constrói uma linha da aba Environments a partir do status do lib.
fn env_row(le: &environments::LangEnv) -> EnvRow {
    let methods: Vec<SharedString> = le.methods_available.iter().map(|m| m.slug().into()).collect();
    let method_sel = methods.first().cloned().unwrap_or_default();
    EnvRow {
        lang: le.lang.into(),
        display: le.display.into(),
        methods: ModelRc::from(Rc::new(VecModel::from(methods))),
        method_sel,
        installed: le.is_installed(),
        status_label: env_status_label(le).into(),
        op_label: SharedString::new(),
    }
}

/// Constrói o modelo inteiro da aba Environments a partir de `environments::status()`.
fn build_env_rows() -> Vec<EnvRow> {
    environments::status().iter().map(env_row).collect()
}

/// Localiza o binário `schematize` (CLI) pra montar o comando do terminal:
/// primeiro um irmão do executável atual; senão o do PATH.
fn schematize_bin() -> String {
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
fn which_bin(cmd: &str) -> bool {
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
fn launch_terminal(inner: &str) -> bool {
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

/// Monta o comando do terminal p/ `schematize env <action> <lang> --method <m>`.
/// SEM `--yes`: o CLI mostra o plano e PEDE confirmação ali dentro (consentimento honesto).
fn env_terminal_inner(bin: &str, action: &str, lang: &str, method: &str) -> String {
    format!(
        "echo '── schematize env {action} {lang} ({method}) ──'; echo; \
         {bin} env {action} {lang} --method {method}; \
         echo; read -n1 -s -r -p '…'",
        action = action,
        lang = lang,
        method = method,
        bin = bin
    )
}

/// Dispara o terminal p/ uma ação de environment e devolve o rótulo transitório a exibir
/// na linha (terminal aberto, ou instrução manual quando nenhum terminal foi encontrado).
fn run_env_action(action: &str, lang: &str, method: &str) -> String {
    let bin = schematize_bin();
    let inner = env_terminal_inner(&bin, action, lang, method);
    if launch_terminal(&inner) {
        t("gui.env_terminal_opened")
    } else {
        let cmd = format!("{bin} env {action} {lang} --method {method}");
        tf("gui.env_no_terminal", &[("cmd", &cmd)])
    }
}

/// Uma skill (por slug) está instalada AGORA? (lê o modelo de linhas de skills.)
fn slug_installed(model: &VecModel<SkillRow>, slug: &str) -> bool {
    for i in 0..model.row_count() {
        if let Some(r) = model.row_data(i) {
            if !r.is_header && r.slug == slug {
                return r.state != "missing";
            }
        }
    }
    false
}

/// Índice da linha de uma skill (por slug) no vetor de itens alinhado ao modelo.
fn row_idx_of_slug(row_items: &[Option<Item>], slug: &str) -> Option<usize> {
    row_items
        .iter()
        .position(|m| m.as_ref().map(|it| it.slug == slug).unwrap_or(false))
}

/// Estado do modal de instalação do Marketplace, guardado no lado Rust (o Slint
/// carrega só o visual). Preenchido ao abrir; lido no confirmar.
#[derive(Default, Clone)]
struct ModalState {
    skill_idx: usize,   // linha da skill sendo instalada
    rec_slug: String,   // slug da recomendada a oferecer ("" = nenhuma)
    env_lang: String,   // linguagem do environment a oferecer ("" = nenhum)
}

fn main() -> Result<(), slint::PlatformError> {
    detect_display_env();

    let items = registry::catalog();
    eprintln!("[catalog] {} skills (via schematize::registry::catalog)", items.len());
    let (rows, row_items) = build_rows(&items);
    let row_items = Rc::new(row_items);
    let model = Rc::new(VecModel::from(rows));

    let app = AppWindow::new()?;
    install_i18n(&app);
    app.set_rows(ModelRc::from(model.clone()));
    update_status(&app);
    recompute_headers(&app); // esconde cabeçalhos de página sem itens

    // Página inicial: Instaladas (0). Se NADA estiver instalado, abre no
    // Marketplace (1) — senão o usuário cai numa lista vazia.
    if !model.iter().any(|r| !r.is_header && is_installed(&r)) {
        app.set_active_tab(1);
    }

    // Resolve o latest de todas as skills assim que a janela sobe (não bloqueia).
    kick_resolve_all(&app.as_weak(), &row_items);

    // ---- aba Environments: modelo + índices auxiliares p/ o modal ----
    // Sonda a máquina UMA vez (local, rápido pra command -v). O refresh re-sonda.
    let env_status = environments::status();
    let env_model = Rc::new(VecModel::from(
        env_status.iter().map(env_row).collect::<Vec<EnvRow>>(),
    ));
    app.set_env_rows(ModelRc::from(env_model.clone()));
    // lang → métodos disponíveis (slugs), pra o modal montar os chips sem re-sondar.
    let env_methods: Rc<std::collections::HashMap<String, Vec<String>>> = Rc::new(
        env_status
            .iter()
            .map(|le| {
                (
                    le.lang.to_string(),
                    le.methods_available.iter().map(|m| m.slug().to_string()).collect(),
                )
            })
            .collect(),
    );
    // conjunto das 7 linguagens que TÊM environment (pra decidir a oferta no modal).
    let env_langs: Rc<std::collections::HashSet<String>> =
        Rc::new(env_status.iter().map(|le| le.lang.to_string()).collect());
    // estado do modal de instalação (lado Rust).
    let modal = Rc::new(RefCell::new(ModalState::default()));

    // ---- toggle de seleção de uma linha ----
    {
        let model = model.clone();
        app.on_toggle(move |idx| {
            let i = idx as usize;
            if let Some(mut row) = model.row_data(i) {
                row.selected = !row.selected;
                model.set_row_data(i, row);
            }
        });
    }

    // ---- selecionar todas (da PÁGINA ativa) ----
    // Instaladas (tab 0) → todas as instaladas; Marketplace (tab 1) → todas as
    // não-instaladas. Não toca em linhas da outra página.
    {
        let weak = app.as_weak();
        let model = model.clone();
        app.on_select_all(move || {
            let tab = weak.upgrade().map(|a| a.get_active_tab()).unwrap_or(0);
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if r.is_header {
                        continue;
                    }
                    let on_page = if tab == 1 { is_missing(&r) } else { is_installed(&r) };
                    if on_page && !r.selected {
                        r.selected = true;
                        model.set_row_data(i, r);
                    }
                }
            }
        });
    }
    // ---- selecionar pendentes (só Instaladas): instaladas-DESATUALIZADAS ----
    {
        let model = model.clone();
        app.on_select_pending(move || {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if !r.is_header {
                        r.selected = is_outdated(&r); // nunca marca uma não-instalada
                        model.set_row_data(i, r);
                    }
                }
            }
        });
    }
    {
        let model = model.clone();
        app.on_select_none(move || {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if !r.is_header && r.selected {
                        r.selected = false;
                        model.set_row_data(i, r);
                    }
                }
            }
        });
    }

    // ---- abrir o site do autor (sponsor.url) ----
    {
        let model = model.clone();
        app.on_open_author(move |idx| {
            if let Some(r) = model.row_data(idx as usize) {
                if !r.author_url.is_empty() {
                    util::open_url(&r.author_url);
                }
            }
        });
    }

    // ---- Marketplace: ação por-linha INSTALAR ----
    // Skill de linguagem (ou skill com recommends) → abre o MODAL: oferece instalar
    // a recomendada (base) junto E, opcionalmente, o environment da linguagem (via
    // terminal). Skill sem nada a oferecer → instala direto (um clique).
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        let model = model.clone();
        let env_langs = env_langs.clone();
        let env_methods = env_methods.clone();
        let modal = modal.clone();
        app.on_row_install(move |idx| {
            let i = idx as usize;
            let Some(Some(it)) = row_items.get(i) else {
                return;
            };
            // recomendada a oferecer: 1ª recomendada que NÃO está instalada.
            let rec_slug = it
                .recommends
                .iter()
                .find(|s| !slug_installed(&model, s.as_str()))
                .cloned()
                .unwrap_or_default();
            // environment a oferecer: se o slug da skill é uma das 7 linguagens.
            let env_lang = if env_langs.contains(it.slug.as_str()) {
                it.slug.clone()
            } else {
                String::new()
            };
            // Nada a oferecer → instala direto, sem modal.
            if rec_slug.is_empty() && env_lang.is_empty() {
                run_batch(weak.clone(), vec![(i, true, it.clone())]);
                return;
            }
            let Some(app) = weak.upgrade() else { return };
            *modal.borrow_mut() = ModalState {
                skill_idx: i,
                rec_slug: rec_slug.clone(),
                env_lang: env_lang.clone(),
            };
            app.set_mp_title(tf("gui.mp_install_title", &[("slug", &it.slug)]).into());
            app.set_mp_idx(i as i32);
            // dependência opcional (base recomendada) — NUNCA marcada por padrão.
            let rec_show = !rec_slug.is_empty();
            app.set_mp_rec_show(rec_show);
            app.set_mp_rec_check(false);
            if rec_show {
                app.set_mp_rec_label(tf("gui.mp_with_recommended", &[("slug", &rec_slug)]).into());
            }
            // environment opcional — NUNCA marcado por padrão.
            let env_show = !env_lang.is_empty();
            app.set_mp_env_show(env_show);
            app.set_mp_env_check(false);
            if env_show {
                app.set_mp_env_label(tf("gui.mp_with_env", &[("lang", &env_lang)]).into());
                let methods: Vec<SharedString> = env_methods
                    .get(&env_lang)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|m| m.into())
                    .collect();
                let sel = methods.first().cloned().unwrap_or_default();
                app.set_mp_methods(ModelRc::from(Rc::new(VecModel::from(methods))));
                app.set_mp_method_sel(sel);
            }
            app.set_mp_open(true);
        });
    }

    // ---- Instaladas: ação por-linha ATUALIZAR ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_row_update(move |idx| {
            let i = idx as usize;
            if let Some(Some(it)) = row_items.get(i) {
                run_batch(weak.clone(), vec![(i, true, it.clone())]);
            }
        });
    }

    // ---- Instaladas: ação por-linha DESINSTALAR ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_row_remove(move |idx| {
            let i = idx as usize;
            if let Some(Some(it)) = row_items.get(i) {
                run_batch(weak.clone(), vec![(i, false, it.clone())]);
            }
        });
    }

    // ---- Marketplace: INSTALAR selecionadas (só as não-instaladas) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_install_selected(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, true, |r| r.selected && is_missing(r));
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- Instaladas: ATUALIZAR selecionadas (só instaladas-desatualizadas) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_update_selected(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, true, |r| r.selected && is_outdated(r));
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- Instaladas: DESINSTALAR selecionadas (só instaladas) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_remove_selected(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, false, |r| r.selected && is_installed(r));
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- Instaladas: ATUALIZAR TUDO ----
    // GARANTIA: só instaladas-DESATUALIZADAS (is_outdated ⟺ installed Some E
    // latest > installed). JAMAIS instala uma skill não instalada.
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_update_all(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, true, is_outdated);
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- rechecar versões (re-resolve latest) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_check(move || {
            kick_resolve_all(&weak, &row_items);
        });
    }

    // ==================== aba Environments ====================

    // escolher o método (chip) de uma linha de environment.
    {
        let env_model = env_model.clone();
        app.on_env_pick_method(move |idx, method| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                r.method_sel = method;
                env_model.set_row_data(i, r);
            }
        });
    }
    // instalar o environment da linha → abre TERMINAL com `schematize env install`.
    {
        let env_model = env_model.clone();
        app.on_env_install(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                if r.method_sel.is_empty() {
                    return;
                }
                let label = run_env_action("install", &r.lang.to_string(), &r.method_sel.to_string());
                r.op_label = label.into();
                env_model.set_row_data(i, r);
            }
        });
    }
    // desinstalar o environment da linha → abre TERMINAL com `schematize env remove`.
    {
        let env_model = env_model.clone();
        app.on_env_remove(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                if r.method_sel.is_empty() {
                    return;
                }
                let label = run_env_action("remove", &r.lang.to_string(), &r.method_sel.to_string());
                r.op_label = label.into();
                env_model.set_row_data(i, r);
            }
        });
    }
    // recarregar o status (re-sonda a máquina). Síncrono (local/rápido; evita !Send).
    {
        let env_model = env_model.clone();
        app.on_env_refresh(move || {
            env_model.set_vec(build_env_rows());
        });
    }

    // ==================== modal de instalação (Marketplace) ====================

    {
        let weak = app.as_weak();
        app.on_mp_toggle_rec(move || {
            if let Some(a) = weak.upgrade() {
                a.set_mp_rec_check(!a.get_mp_rec_check());
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_mp_toggle_env(move || {
            if let Some(a) = weak.upgrade() {
                a.set_mp_env_check(!a.get_mp_env_check());
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_mp_pick_method(move |m| {
            if let Some(a) = weak.upgrade() {
                a.set_mp_method_sel(m);
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_mp_cancel(move || {
            if let Some(a) = weak.upgrade() {
                a.set_mp_open(false);
            }
        });
    }
    // confirmar: instala a skill in-process (+ a base marcada, no MESMO lote paralelo)
    // e, se marcado, dispara o environment num TERMINAL (fora do processo).
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        let modal = modal.clone();
        let env_model = env_model.clone();
        app.on_mp_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let st = modal.borrow().clone();
            // lote in-process: a skill + (recomendada SÓ se o usuário marcou).
            let mut ops: Vec<(usize, bool, Item)> = Vec::new();
            if let Some(Some(it)) = row_items.get(st.skill_idx) {
                ops.push((st.skill_idx, true, it.clone()));
            }
            if app.get_mp_rec_check() && !st.rec_slug.is_empty() {
                if let Some(ridx) = row_idx_of_slug(&row_items, &st.rec_slug) {
                    if let Some(Some(rit)) = row_items.get(ridx) {
                        ops.push((ridx, true, rit.clone()));
                    }
                }
            }
            // environment opcional → terminal (só se marcado + método escolhido).
            let do_env = app.get_mp_env_check() && !st.env_lang.is_empty();
            let env_method = app.get_mp_method_sel().to_string();
            app.set_mp_open(false);
            run_batch(weak.clone(), ops);
            if do_env && !env_method.is_empty() {
                let label = run_env_action("install", &st.env_lang, &env_method);
                app.set_status(SharedString::from(label.clone()));
                // reflete a msg no card correspondente da aba Environments.
                for i in 0..env_model.row_count() {
                    if let Some(mut r) = env_model.row_data(i) {
                        if r.lang == st.env_lang {
                            r.op_label = label.clone().into();
                            env_model.set_row_data(i, r);
                            break;
                        }
                    }
                }
            }
        });
    }

    app.run()
}
