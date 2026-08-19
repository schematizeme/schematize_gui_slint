//! Linhas e paginação da lista de skills (Mercado/Instaladas): monta `SkillRow`,
//! recomputa cabeçalhos de seção, página e o status agregado.

use crate::prelude::*;

// ---------------------------------------------------------------------------
// aba Gerenciar — lista os SLUGS das skills instaladas escaneando o diretório
// de skills (`~/.claude/skills/schematize-<slug>/` com SKILL.md). Cobre tanto as
// skills do catálogo quanto as criadas pelo usuário (que não estão no catálogo).
// Retorna Vec<String> (Send) — seguro pra rodar em thread e postar via event loop.
// ---------------------------------------------------------------------------
pub(crate) fn installed_skill_slugs() -> Vec<String> {
    let dir = util::skills_dir();
    let mut out: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue };
            if let Some(slug) = name.strip_prefix("schematize-") {
                if p.join("SKILL.md").is_file() {
                    out.push(slug.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Monta um `ModelRc<SharedString>` a partir de uma lista de Strings (roda na UI).
pub(crate) fn strings_model(v: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(
        v.into_iter().map(SharedString::from).collect::<Vec<SharedString>>(),
    )))
}

// ---------------------------------------------------------------------------
// Estado derivado (missing/outdated/current/loading) + rótulo traduzido.
// ---------------------------------------------------------------------------
pub(crate) fn compute_state(installed: &Option<String>, latest: &Option<String>) -> (String, String) {
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
pub(crate) fn category_of(it: &Item) -> &str {
    if it.category.is_empty() { "language" } else { it.category.as_str() }
}

/// Cabeçalho de categoria de UMA página (page 0 = Instaladas, 1 = Marketplace).
/// `count` é preenchido/atualizado por `recompute_headers` (esconde vazios).
pub(crate) fn header_row(label: &str, cat: &str, page: i32) -> SkillRow {
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
        disp: -1,
        forked: false,
        rating: SharedString::new(),
    }
}

/// Linha inicial de uma skill: instalada lida do disco (rápido), latest ainda
/// "…" (resolvido depois, assíncrono). Estado derivado do que já se sabe.
/// `forked` = a skill oficial virou fork editável (marca [fork] + habilita Comparar).
pub(crate) fn skill_row(it: &Item, forked: bool) -> SkillRow {
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
        disp: -1,
        forked,
        rating: SharedString::new(),
    }
}

/// Conjunto dos slugs atualmente FORKADOS (lido do estado uma vez).
pub(crate) fn forked_slugs() -> HashSet<String> {
    skills::load_state()
        .skills
        .iter()
        .filter(|(_, e)| e.forked)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Ordena os itens em grupos (base, language, external). Por categoria emite
/// DOIS cabeçalhos (Instaladas page=0 e Marketplace page=1) seguidos das skills;
/// a página ativa mostra o cabeçalho certo e as skills cujo estado casa (o Slint
/// filtra por `state`). Devolve o Item por linha (None nos cabeçalhos).
pub(crate) fn build_rows(items: &[Item]) -> (Vec<SkillRow>, Vec<Option<Item>>) {
    let groups = [
        ("base", t("gui.cat_base")),
        ("language", t("gui.cat_language")),
        ("external", t("gui.cat_external")),
    ];
    let forked = forked_slugs();
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
            rows.push(skill_row(it, forked.contains(it.slug.as_str())));
            row_items.push(Some(it.clone()));
        }
    }
    (rows, row_items)
}

/// Atualiza o marcador `forked` da linha de uma skill (por slug) no modelo do app —
/// chamado após uma edição que forka uma skill oficial, pra o badge [fork] e o botão
/// Comparar aparecerem sem recarregar a lista inteira. Opera sobre `app.global::<Sk>().get_rows()`
/// (roda no event loop; nada de Rc cruzando thread).
pub(crate) fn mark_row_forked(app: &AppWindow, slug: &str, forked: bool) {
    let rows = app.global::<Sk>().get_rows();
    for i in 0..rows.row_count() {
        if let Some(mut r) = rows.row_data(i) {
            if !r.is_header && r.slug == slug && r.forked != forked {
                r.forked = forked;
                rows.set_row_data(i, r);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reconta os cabeçalhos: cada cabeçalho (page, categoria) ganha o nº de skills
// que estão AGORA na sua página (Instaladas = state != "missing"; Marketplace =
// state == "missing"). count==0 → o Slint esconde o cabeçalho. Roda no event
// loop (só usa o modelo do app; nada de dados !Send).
// ---------------------------------------------------------------------------
pub(crate) fn recompute_headers(app: &AppWindow) {
    let rows = app.global::<Sk>().get_rows();
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
    recompute_pagination(app);
}

// ---------------------------------------------------------------------------
// Paginação do Mercado: numera (disp) as skills VISÍVEIS na página-tab ativa em
// ordem sequencial; -1 nas que não pertencem à página. O Slint mostra só as
// cujo `disp` cai na janela `[mkt-page*20, +20)`. Total → controla o Pager.
// ---------------------------------------------------------------------------
pub(crate) fn recompute_pagination(app: &AppWindow) {
    let tab = app.get_active_tab();
    let rows = app.global::<Sk>().get_rows();
    let n = rows.row_count();
    let mut idx = 0i32;
    for i in 0..n {
        let Some(mut r) = rows.row_data(i) else { continue };
        if r.is_header {
            continue;
        }
        let missing = r.state == "missing";
        let on_page = (tab == 0 && !missing) || (tab == 1 && missing);
        let new_disp = if on_page {
            let d = idx;
            idx += 1;
            d
        } else {
            -1
        };
        if r.disp != new_disp {
            r.disp = new_disp;
            rows.set_row_data(i, r);
        }
    }
    app.global::<Mp>().set_total(idx);
}

// ---------------------------------------------------------------------------
// Status global (contagem de pendências) — mesma regra do egui.
// ---------------------------------------------------------------------------
pub(crate) fn update_status(app: &AppWindow) {
    let rows = app.global::<Sk>().get_rows();
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
    app.global::<Sk>().set_status(status.into());
}

/// Coleta ops (idx, install?, Item) das linhas que casam com o predicado.
/// `install=true` → instalar/atualizar; `install=false` → remover.
pub(crate) fn collect_ops(
    app: &AppWindow,
    row_items: &Rc<Vec<Option<Item>>>,
    install: bool,
    pred: impl Fn(&SkillRow) -> bool,
) -> Vec<(usize, bool, Item)> {
    let rows = app.global::<Sk>().get_rows();
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
pub(crate) fn is_missing(r: &SkillRow) -> bool {
    r.state == "missing"
}

/// Instalada (pertence a Instaladas) — qualquer estado que não seja "missing".
pub(crate) fn is_installed(r: &SkillRow) -> bool {
    r.state != "missing"
}

/// Instalada E desatualizada (installed Some E latest > installed). É o ÚNICO
/// alvo de "Atualizar tudo"/"Atualizar selecionadas": jamais instala nova.
pub(crate) fn is_outdated(r: &SkillRow) -> bool {
    r.state == "outdated"
}

/// Tamanho da página das listas paginadas (mercado, histórico do DB, commits).
pub(crate) const PAGE: usize = 20;

/// Uma skill (por slug) está instalada AGORA? (lê o modelo de linhas de skills.)
pub(crate) fn slug_installed(model: &VecModel<SkillRow>, slug: &str) -> bool {
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
pub(crate) fn row_idx_of_slug(row_items: &[Option<Item>], slug: &str) -> Option<usize> {
    row_items
        .iter()
        .position(|m| m.as_ref().map(|it| it.slug == slug).unwrap_or(false))
}
