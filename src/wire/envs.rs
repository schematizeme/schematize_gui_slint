//! Fiação da aba Environments e do modal de instalação do Mercado (a skill mais
//! a base recomendada e o environment da linguagem, num passo só).
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let row_items = cx.row_items.clone();
    let modal = cx.modal.clone();
    let env_model = cx.env_model.clone();
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
                // Linguagem exige método escolhido; ferramenta ("tool") não tem seletor.
                if r.category != "tool" && r.method_sel.is_empty() {
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
                // Linguagem exige método; ferramenta não (o CLI ignora `--method`).
                if r.category != "tool" && r.method_sel.is_empty() {
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

}
