//! Histórico do overdev na aba Overdev: snapshots do DB local e commits do git,
//! ambos PAGINADOS no Rust (a UI recebe uma página por vez).

/// Tamanho da página das listas paginadas desta aba (snapshots e commits).
///
/// **Veio do `skillrows.rs`, que saiu na E5.** Lá ele era compartilhado com as listas de
/// skills — um `PAGE` só para três telas de assuntos diferentes. Com as skills fora, ele não
/// tinha mais por que morar longe de quem o usa: a paginação desta aba é decisão desta aba.
pub(crate) const PAGE: usize = 20;

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
pub(crate) fn commit_rows_page(all: &[crate::gitlog::Commit], page: i32) -> Vec<CommitRow> {
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
    commits_all: &RefCell<Vec<crate::gitlog::Commit>>,
    commits_model: &VecModel<CommitRow>,
    proj: Option<&Path>,
) {
    match proj {
        Some(p) => {
            let snaps = overdevdb::history(p, 50).unwrap_or_default();
            // Dois documentos, duas perguntas: o snapshot vem do `overdevdb` local, o
            // commit vem do BINÁRIO do git (a E2 tirou o `githist` deste crate). Sem o app
            // instalado, a lista de commits sai vazia e os snapshots continuam na tela.
            let (up, commits) = crate::gitlog::ler(p);
            app.global::<Od>().set_upstream_line(fmt_upstream(up).into());
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
