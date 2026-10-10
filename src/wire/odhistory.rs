//! Fiação do histórico de COMMITS na aba Overdev: carga e paginação.
//!
//! Os snapshots do DB (ver/restaurar) saíram na E6 M5: são a aba «Histórico» da janela do overdev.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let od_current = cx.od_current.clone();
    let od_commits_all = cx.od_commits_all.clone();
    let od_commits_model = cx.od_commits_model.clone();
    // ==================== Overdev — histórico de commits ====================
    // recarrega o histórico do projeto atual (chamado ao entrar na tela / reload).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let ca = od_commits_all.clone();
        let cm = od_commits_model.clone();
        app.global::<Od>().on_refresh_history(move || {
            if let Some(app) = weak.upgrade() {
                let p = cur.borrow().clone();
                refresh_od_history(&app, &ca, &cm, p.as_deref());
            }
        });
    }
    // paginação dos commits.
    {
        let weak = app.as_weak();
        let all = od_commits_all.clone();
        let m = od_commits_model.clone();
        app.global::<Od>().on_commit_page_prev(move || {
            if let Some(app) = weak.upgrade() {
                let p = (app.global::<Od>().get_commit_page() - 1).max(0);
                app.global::<Od>().set_commit_page(p);
                m.set_vec(commit_rows_page(&all.borrow(), p));
            }
        });
    }
    {
        let weak = app.as_weak();
        let all = od_commits_all.clone();
        let m = od_commits_model.clone();
        app.global::<Od>().on_commit_page_next(move || {
            if let Some(app) = weak.upgrade() {
                let p = app.global::<Od>().get_commit_page() + 1;
                if (p as usize) * PAGE < all.borrow().len() {
                    app.global::<Od>().set_commit_page(p);
                    m.set_vec(commit_rows_page(&all.borrow(), p));
                }
            }
        });
    }
}
