//! Histórico de COMMITS na aba Overdev, PAGINADO no Rust (a UI recebe uma página por vez).
//! Os snapshots do DB saíram na E6 M5 (aba «Histórico» da janela do overdev).

/// Tamanho da página das listas paginadas desta aba (commits).
///
/// **Veio do `skillrows.rs`, que saiu na E5.** Lá ele era compartilhado com as listas de
/// skills — um `PAGE` só para três telas de assuntos diferentes. Com as skills fora, ele não
/// tinha mais por que morar longe de quem o usa: a paginação desta aba é decisão desta aba.
pub(crate) const PAGE: usize = 20;

use crate::prelude::*;

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

/// (Re)carrega os commits do projeto `proj` no modelo, reseta a paginação e escreve a linha
/// de upstream. None → limpa tudo. Síncrono (git local, rápido).
pub(crate) fn refresh_od_history(
    app: &AppWindow,
    commits_all: &RefCell<Vec<crate::gitlog::Commit>>,
    commits_model: &VecModel<CommitRow>,
    proj: Option<&Path>,
) {
    match proj {
        Some(p) => {
            // O commit vem do BINÁRIO do git (a E2 tirou o `githist` deste crate). Sem o app
            // instalado, a lista sai vazia e o resto da aba continua.
            let (up, commits) = crate::gitlog::ler(p);
            app.global::<Od>().set_upstream_line(fmt_upstream(up).into());
            app.global::<Od>().set_commit_total(commits.len() as i32);
            app.global::<Od>().set_commit_page(0);
            commits_model.set_vec(commit_rows_page(&commits, 0));
            *commits_all.borrow_mut() = commits;
        }
        None => {
            app.global::<Od>().set_upstream_line(SharedString::new());
            app.global::<Od>().set_commit_total(0);
            app.global::<Od>().set_commit_page(0);
            commits_model.set_vec(Vec::new());
            commits_all.borrow_mut().clear();
        }
    }
}
