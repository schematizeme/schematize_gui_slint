//! Fiação da lista de skills: seleção de linhas, ações por linha e em massa
//! (instalar/atualizar/desinstalar), rechecagem de versões e paginação do Mercado.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let row_items = cx.row_items.clone();
    let model = cx.model.clone();
    let modal = cx.modal.clone();
    let env_methods = cx.env_methods.clone();
    let env_langs = cx.env_langs.clone();
    // ---- relançar o app (janela nova) — conserto do restart pós self-update ----
    // O callback existe e está ligado ao helper CORRETO (spawn desacoplado antes
    // de sair). Hoje o self-update NÃO está fiado nesta GUI Slint (mora no módulo
    // egui do lib, fora do alcance daqui), então nada dispara `restart()` ainda;
    // quando o self-update for portado pra cá, é só invocar `root.restart()`.
    app.global::<App>().on_restart(move || restart_app());

    // ---- toggle de seleção de uma linha ----
    {
        let model = model.clone();
        app.global::<Sk>().on_toggle(move |idx| {
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
        app.global::<Sk>().on_select_all(move || {
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
        app.global::<Sk>().on_select_pending(move || {
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
        app.global::<Sk>().on_select_none(move || {
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
        app.global::<Sk>().on_open_author(move |idx| {
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
        app.global::<Sk>().on_row_install(move |idx| {
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
            app.global::<Mp>().set_title(tf("gui.mp_install_title", &[("slug", &it.slug)]).into());
            app.global::<Mp>().set_idx(i as i32);
            // dependência opcional (base recomendada) — NUNCA marcada por padrão.
            let rec_show = !rec_slug.is_empty();
            app.global::<Mp>().set_rec_show(rec_show);
            app.global::<Mp>().set_rec_check(false);
            if rec_show {
                app.global::<Mp>().set_rec_label(tf("gui.mp_with_recommended", &[("slug", &rec_slug)]).into());
            }
            // environment opcional — NUNCA marcado por padrão.
            let env_show = !env_lang.is_empty();
            app.global::<Mp>().set_env_show(env_show);
            app.global::<Mp>().set_env_check(false);
            if env_show {
                app.global::<Mp>().set_env_label(tf("gui.mp_with_env", &[("lang", &env_lang)]).into());
                let methods: Vec<SharedString> = env_methods
                    .get(&env_lang)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|m| m.into())
                    .collect();
                let sel = methods.first().cloned().unwrap_or_default();
                app.global::<Mp>().set_methods(ModelRc::from(Rc::new(VecModel::from(methods))));
                app.global::<Mp>().set_method_sel(sel);
            }
            app.global::<Mp>().set_open(true);
        });
    }

    // ---- Instaladas: ação por-linha ATUALIZAR ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.global::<Sk>().on_row_update(move |idx| {
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
        app.global::<Sk>().on_row_remove(move |idx| {
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
        app.global::<Sk>().on_install_selected(move || {
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
        app.global::<Sk>().on_update_selected(move || {
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
        app.global::<Sk>().on_remove_selected(move || {
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
        app.global::<Sk>().on_update_all(move || {
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
        app.global::<Sk>().on_check(move || {
            kick_resolve_all(&weak, &row_items);
            kick_market_ratings(weak.clone());
        });
    }

    // ==================== Paginação do Mercado ====================
    // Recomputa os índices de exibição (disp) quando a sub-aba muda.
    {
        let weak = app.as_weak();
        app.global::<Mp>().on_recompute(move || {
            if let Some(app) = weak.upgrade() {
                recompute_pagination(&app);
            }
        });
    }

}
