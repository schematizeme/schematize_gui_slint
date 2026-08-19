//! Histórico do overdev na aba Overdev: snapshots do DB local e commits do git,
//! ambos PAGINADOS no Rust (a UI recebe uma página por vez).

use crate::prelude::*;

/// Uma página do histórico do DB (metadados → SnapRow).
pub(crate) fn snap_rows_page(all: &[overdevdb::SnapshotMeta], page: i32) -> Vec<SnapRow> {
    let start = (page.max(0) as usize) * PAGE;
    all.iter()
        .skip(start)
        .take(PAGE)
        .map(|m| SnapRow {
            id: m.id as i32,
            file: m.file.clone().into(),
            date: fmt_ts(m.ts).into(),
            size: fmt_size(m.size).into(),
            hash: m.hash.chars().take(8).collect::<String>().into(),
        })
        .collect()
}

/// Uma página do histórico de commits (Commit → CommitRow).
pub(crate) fn commit_rows_page(all: &[githist::Commit], page: i32) -> Vec<CommitRow> {
    let start = (page.max(0) as usize) * PAGE;
    all.iter()
        .skip(start)
        .take(PAGE)
        .map(|c| CommitRow {
            short: c.short.clone().into(),
            date: c.date.clone().into(),
            author: c.author.clone().into(),
            subject: c.subject.clone().into(),
            pushed: c.pushed,
        })
        .collect()
}

/// (Re)carrega o histórico do DB do overdev + os commits do projeto `proj` nos
/// modelos, reseta a paginação e escreve a linha de upstream. None → limpa tudo.
/// Síncrono (sqlite/git locais e rápidos — mesma escolha do env status).
pub(crate) fn refresh_od_history(
    app: &AppWindow,
    snaps_all: &RefCell<Vec<overdevdb::SnapshotMeta>>,
    snaps_model: &VecModel<SnapRow>,
    commits_all: &RefCell<Vec<githist::Commit>>,
    commits_model: &VecModel<CommitRow>,
    proj: Option<&Path>,
) {
    match proj {
        Some(p) => {
            let snaps = overdevdb::history(p, 50).unwrap_or_default();
            let commits = githist::commits(p, 50);
            app.global::<Od>().set_upstream_line(fmt_upstream(githist::upstream(p)).into());
            app.global::<Od>().set_snap_total(snaps.len() as i32);
            app.global::<Od>().set_commit_total(commits.len() as i32);
            app.global::<Od>().set_snap_page(0);
            app.global::<Od>().set_commit_page(0);
            snaps_model.set_vec(snap_rows_page(&snaps, 0));
            commits_model.set_vec(commit_rows_page(&commits, 0));
            *snaps_all.borrow_mut() = snaps;
            *commits_all.borrow_mut() = commits;
        }
        None => {
            app.global::<Od>().set_upstream_line(SharedString::new());
            app.global::<Od>().set_snap_total(0);
            app.global::<Od>().set_commit_total(0);
            app.global::<Od>().set_snap_page(0);
            app.global::<Od>().set_commit_page(0);
            snaps_model.set_vec(Vec::new());
            commits_model.set_vec(Vec::new());
            snaps_all.borrow_mut().clear();
            commits_all.borrow_mut().clear();
        }
    }
}
