//! Trabalho de skills FORA do event loop: resolve versão, instala/desinstala/atualiza
//! em thread e espelha o resultado na UI via `invoke_from_event_loop`.

use crate::prelude::*;

// ---------------------------------------------------------------------------
// thread→UI: posta a atualização de versões (installed + latest) de uma linha.
// ---------------------------------------------------------------------------
pub(crate) fn post_versions(
    weak: Weak<AppWindow>,
    idx: usize,
    installed: Option<String>,
    latest: Option<String>,
) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.global::<Sk>().get_rows();
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
pub(crate) fn post_row_busy(weak: Weak<AppWindow>, idx: usize, label: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.global::<Sk>().get_rows();
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
pub(crate) fn post_row_result(
    weak: Weak<AppWindow>,
    idx: usize,
    install: bool,
    res: Result<String, String>,
) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.global::<Sk>().get_rows();
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
pub(crate) fn post_batch_done(weak: Weak<AppWindow>, ok: usize, err: usize) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<Sk>().set_busy(false);
            let toast = tf("gui.batch_done", &[("ok", &ok.to_string()), ("err", &err.to_string())]);
            app.global::<Sk>().set_status(toast.into());
            recompute_headers(&app);
        }
    });
}

// ---------------------------------------------------------------------------
// Resolução assíncrona do latest de UMA skill (rede). Detached: reusa install/
// check. Re-lê a instalada (barato) pra refletir mudanças de disco.
// ---------------------------------------------------------------------------
pub(crate) fn spawn_resolve(weak: Weak<AppWindow>, idx: usize, item: Item) {
    std::thread::spawn(move || {
        let installed = skills::installed_version(&item);
        let latest = skills::resolve_latest(&item).ok();
        post_versions(weak, idx, installed, latest);
    });
}

/// Dispara a resolução do latest de todas as skills em paralelo (uma thread por
/// skill; são poucas). Antes, zera a coluna latest de volta pra "…".
pub(crate) fn kick_resolve_all(weak: &Weak<AppWindow>, row_items: &Rc<Vec<Option<Item>>>) {
    if let Some(app) = weak.upgrade() {
        let rows = app.global::<Sk>().get_rows();
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
// Notas do marketplace: busca TODAS as médias numa thread (1 request) e preenche
// a coluna `rating` de cada linha de skill por slug. Sem nota (count 0 / None) →
// `format_rating` devolve "" e o badge some. Falha de rede = HashMap vazio → só
// não mostra nota (a UI nunca trava; nada de bloqueio no event loop).
// ---------------------------------------------------------------------------
pub(crate) fn kick_market_ratings(weak: Weak<AppWindow>) {
    std::thread::spawn(move || {
        let ratings = market::market_ratings_all(); // HashMap<String,(f32,u32)> (Send)
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                let rows = app.global::<Sk>().get_rows();
                for i in 0..rows.row_count() {
                    if let Some(mut r) = rows.row_data(i) {
                        if r.is_header {
                            continue;
                        }
                        let rating = market::format_rating(ratings.get(r.slug.as_str()).copied());
                        if r.rating.as_str() != rating {
                            r.rating = rating.into();
                            rows.set_row_data(i, r);
                        }
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Ações em massa/paralelo (espelha o run_batch do egui). ops = (idx, install?, Item).
// ---------------------------------------------------------------------------
pub(crate) fn run_batch(weak: Weak<AppWindow>, ops: Vec<(usize, bool, Item)>) {
    if ops.is_empty() {
        return;
    }
    if let Some(app) = weak.upgrade() {
        if app.global::<Sk>().get_busy() {
            return; // já tem lote rodando
        }
        app.global::<Sk>().set_busy(true);
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

/// Estado do modal de instalação do Marketplace, guardado no lado Rust (o Slint
/// carrega só o visual). Preenchido ao abrir; lido no confirmar.
#[derive(Default, Clone)]
pub(crate) struct ModalState {
    pub(crate) skill_idx: usize, // linha da skill sendo instalada
    pub(crate) rec_slug: String, // slug da recomendada a oferecer ("" = nenhuma)
    pub(crate) env_lang: String, // linguagem do environment a oferecer ("" = nenhum)
}
