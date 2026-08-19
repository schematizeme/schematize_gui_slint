//! Fiação do histórico do overdev na aba Overdev: paginação dos snapshots do DB
//! local e dos commits do git, ver/restaurar snapshot.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let od_current = cx.od_current.clone();
    let od_snaps_all = cx.od_snaps_all.clone();
    let od_snaps_model = cx.od_snaps_model.clone();
    let od_commits_all = cx.od_commits_all.clone();
    let od_commits_model = cx.od_commits_model.clone();
    // ==================== Overdev — histórico DB + commits ====================
    // recarrega o histórico do projeto atual (chamado ao entrar na tela / reload).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let sa = od_snaps_all.clone();
        let sm = od_snaps_model.clone();
        let ca = od_commits_all.clone();
        let cm = od_commits_model.clone();
        app.global::<Od>().on_refresh_history(move || {
            if let Some(app) = weak.upgrade() {
                let p = cur.borrow().clone();
                refresh_od_history(&app, &sa, &sm, &ca, &cm, p.as_deref());
            }
        });
    }
    // paginação do histórico do DB.
    {
        let weak = app.as_weak();
        let all = od_snaps_all.clone();
        let m = od_snaps_model.clone();
        app.global::<Od>().on_snap_page_prev(move || {
            if let Some(app) = weak.upgrade() {
                let p = (app.global::<Od>().get_snap_page() - 1).max(0);
                app.global::<Od>().set_snap_page(p);
                m.set_vec(snap_rows_page(&all.borrow(), p));
            }
        });
    }
    {
        let weak = app.as_weak();
        let all = od_snaps_all.clone();
        let m = od_snaps_model.clone();
        app.global::<Od>().on_snap_page_next(move || {
            if let Some(app) = weak.upgrade() {
                let p = app.global::<Od>().get_snap_page() + 1;
                if (p as usize) * PAGE < all.borrow().len() {
                    app.global::<Od>().set_snap_page(p);
                    m.set_vec(snap_rows_page(&all.borrow(), p));
                }
            }
        });
    }
    // Ver: conteúdo do snapshot num visor read-only.
    {
        let weak = app.as_weak();
        app.global::<Od>().on_snap_view(move |id| {
            let Some(app) = weak.upgrade() else { return };
            match overdevdb::get(id as i64) {
                Ok(content) => {
                    app.global::<Od>().set_snap_view_title(format!("snapshot #{id}").into());
                    app.global::<Od>().set_snap_view_content(content.into());
                }
                Err(e) => {
                    app.global::<Od>().set_snap_view_title(format!("snapshot #{id}").into());
                    app.global::<Od>().set_snap_view_content(e.into());
                }
            }
            app.global::<Od>().set_snap_view_open(true);
        });
    }
    // Restaurar: pede confirmação.
    {
        let weak = app.as_weak();
        app.global::<Od>().on_snap_restore_request(move |id| {
            let Some(app) = weak.upgrade() else { return };
            app.global::<Od>().set_snap_confirm_id(id);
            app.global::<Od>().set_snap_confirm_msg(
                format!("{} #{id}?", tor("gui.od_restore_confirm", "Restaurar o snapshot")).into(),
            );
            app.global::<Od>().set_snap_confirm_open(true);
        });
    }
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_snap_restore_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let id = app.global::<Od>().get_snap_confirm_id();
            app.global::<Od>().set_snap_confirm_open(false);
            let root = cur.borrow().clone();
            if let (Some(p), true) = (root, id >= 0) {
                match overdevdb::restore(id as i64, &p) {
                    Ok(dest) => app.global::<Od>().set_run_status(
                        format!("{} {}", tor("gui.od_restored", "restaurado:"), dest.display()).into(),
                    ),
                    Err(e) => app.global::<Od>().set_run_status(e.into()),
                }
                // recarrega overdev (checklist) + histórico refletindo o disco.
                app.global::<Od>().invoke_reload();
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Od>().on_snap_restore_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_snap_confirm_open(false);
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
