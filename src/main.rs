//! SPIKE — aba Skills do schematize em Slint (prova de conceito visual).
//!
//! O quê: uma janela Slint que replica a **aba Skills** da GUI atual em egui
//! (`schematize_cli_rs/src/gui.rs`): lista de skills agrupada por categoria,
//! com checkbox, selo de verificado, autor, versões e estado. O objetivo é
//! avaliar o salto visual — NÃO é o binário que shippa (esse continua egui).
//!
//! Dados: lê o `catalog.json` do crate irmão por serde (sem depender do crate
//! `schematize` — evita ciclos/features). Se não achar o arquivo em disco, cai
//! num snapshot embutido em build-time (`include_str!`). NÃO edita o outro crate.
//!
//! Instalar/atualizar são NO-OP aqui (só logam + atualizam o status/"toast") —
//! o foco do spike é o VISUAL, não a mecânica (essa já existe no crate real).

use serde::Deserialize;
use slint::{Model, ModelRc, VecModel, SharedString};
use std::rc::Rc;

slint::include_modules!(); // gera AppWindow, SkillRow, Theme a partir de ui/app.slint

// ---------------------------------------------------------------------------
// Espelho mínimo do schema de registry.rs (só o que a tela usa).
// ---------------------------------------------------------------------------
#[derive(Clone, Deserialize)]
struct Sponsor {
    name: String,
    url: String,
}

#[derive(Clone, Deserialize)]
struct Item {
    slug: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    sponsor: Option<Sponsor>,
    #[serde(default)]
    verified: bool,
}

#[derive(Deserialize)]
struct Catalog {
    skills: Vec<Item>,
}

/// Snapshot do catálogo embutido em build-time — fallback se o arquivo em disco
/// não for encontrado (rodando o binário fora da árvore). Read-only; não altera
/// o crate schematize_cli_rs.
const EMBEDDED_CATALOG: &str = include_str!("../../schematize_cli_rs/catalog.json");

/// Carrega os itens do catálogo: tenta o arquivo em disco (fonte viva), cai no
/// snapshot embutido. Caminhos candidatos relativos ao cwd de execução comum.
fn load_items() -> Vec<Item> {
    let candidates = [
        "../schematize_cli_rs/catalog.json",
        "schematize_cli_rs/catalog.json",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../schematize_cli_rs/catalog.json"),
    ];
    for path in candidates {
        if let Ok(body) = std::fs::read_to_string(path) {
            if let Ok(cat) = serde_json::from_str::<Catalog>(&body) {
                if !cat.skills.is_empty() {
                    eprintln!("[catalog] lido de disco: {path} ({} skills)", cat.skills.len());
                    return cat.skills;
                }
            }
        }
    }
    let cat: Catalog = serde_json::from_str(EMBEDDED_CATALOG).expect("snapshot embutido inválido");
    eprintln!("[catalog] arquivo não encontrado em disco — usando snapshot embutido ({} skills)", cat.skills.len());
    cat.skills
}

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
    eprintln!("[env] backend Slint    : winit (default) — cobre Wayland E X11; renderer femtovg (OpenGL/GLES)");
    if wayland.is_none() && x11.is_none() {
        eprintln!("[env] AVISO: sem display, a janela não abre. Este spike valida COMPILAÇÃO; a exibição precisa de um servidor Wayland/X11.");
    }
}

// ---------------------------------------------------------------------------
// Montagem do modelo da lista (cabeçalhos de categoria + skills).
// ---------------------------------------------------------------------------
fn header(label: &str) -> SkillRow {
    SkillRow {
        is_header: true,
        header_label: label.into(),
        slug: SharedString::new(),
        author: SharedString::new(),
        author_url: SharedString::new(),
        installed: SharedString::new(),
        latest: SharedString::new(),
        state: SharedString::new(),
        state_label: SharedString::new(),
        verified: false,
        selected: false,
    }
}

/// Converte um Item do catálogo numa linha da tela. Versões instaladas/latest
/// são placeholders no spike ("—") — a resolução real vive no crate schematize.
fn skill_row(it: &Item) -> SkillRow {
    let author = it.sponsor.as_ref().map(|s| s.name.clone()).unwrap_or_default();
    let author_url = it.sponsor.as_ref().map(|s| s.url.clone()).unwrap_or_default();
    // Sem acesso à instalação real: tudo "não instalado" para exercitar o visual.
    SkillRow {
        is_header: false,
        header_label: SharedString::new(),
        slug: it.slug.clone().into(),
        author: author.into(),
        author_url: author_url.into(),
        installed: "—".into(),
        latest: "—".into(),
        state: "missing".into(),
        state_label: "Instalar".into(),
        verified: it.verified,
        selected: false,
    }
}

/// Ordena os itens em grupos (base, language, external) e intercala cabeçalhos.
fn build_rows(items: &[Item]) -> Vec<SkillRow> {
    let groups = [
        ("base", "Base & Arquitetura"),
        ("language", "Linguagens"),
        ("external", "Ferramentas externas"),
    ];
    let mut out = Vec::new();
    for (cat, label) in groups {
        let group: Vec<&Item> = items
            .iter()
            .filter(|it| {
                let c = if it.category.is_empty() { "language" } else { it.category.as_str() };
                c == cat
            })
            .collect();
        if group.is_empty() {
            continue;
        }
        out.push(header(label));
        out.extend(group.iter().map(|it| skill_row(it)));
    }
    out
}

fn main() -> Result<(), slint::PlatformError> {
    detect_display_env();

    let items = load_items();
    let rows: Vec<SkillRow> = build_rows(&items);
    let model = Rc::new(VecModel::from(rows));

    let app = AppWindow::new()?;
    app.set_rows(ModelRc::from(model.clone()));
    app.set_status("Pronto — spike visual (ações são no-op)".into());

    // Toggle de seleção: alterna a linha e atualiza o contador no status.
    {
        let weak = app.as_weak();
        let model_ref = model.clone();
        app.on_toggle(move |idx| {
            let i = idx as usize;
            if let Some(mut row) = model_ref.row_data(i) {
                row.selected = !row.selected;
                model_ref.set_row_data(i, row);
            }
            let n = model_ref.iter().filter(|r| !r.is_header && r.selected).count();
            if let Some(app) = weak.upgrade() {
                app.set_status(format!("{n} selecionada(s)").into());
            }
        });
    }

    // Instalar selecionadas — NO-OP: só loga e mostra "toast" no status.
    {
        let weak = app.as_weak();
        let model_ref = model.clone();
        app.on_install_selected(move || {
            let picked: Vec<String> = model_ref
                .iter()
                .filter(|r| !r.is_header && r.selected)
                .map(|r| r.slug.to_string())
                .collect();
            eprintln!("[acao] instalar selecionadas (no-op): {picked:?}");
            if let Some(app) = weak.upgrade() {
                let msg = if picked.is_empty() {
                    "Nenhuma skill selecionada".to_string()
                } else {
                    format!("(spike) instalaria: {}", picked.join(", "))
                };
                app.set_status(msg.into());
            }
        });
    }

    // Atualizar tudo — NO-OP.
    {
        let weak = app.as_weak();
        app.on_update_all(move || {
            eprintln!("[acao] atualizar tudo (no-op)");
            if let Some(app) = weak.upgrade() {
                app.set_status("(spike) atualizaria todas as skills".into());
            }
        });
    }

    app.run()
}
