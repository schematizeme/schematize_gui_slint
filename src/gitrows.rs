//! Linhas da tela Git — contas, estado local dos repositórios e repos do serviço.
//!
//! O quê: traduz `gitcontas::{contas::Conta, repos::{EstadoLocal, Remoto}}` pras
//! structs da fronteira. Onde: usado por `wire::git`. Puro, exceto
//! `account_rows`, que consulta o `~/.ssh/config` pra saber se o alias existe —
//! e isso é informação de AÇÃO, não enfeite: sem o alias o push sai pela chave
//! errada (ou falha), então tem de aparecer na linha.

use crate::prelude::*;
use schematize::gitcontas::{
    aplicar,
    contas::{Auth, Conta},
    repos::{EstadoLocal, Remoto},
};

/// Contas cadastradas → linhas.
pub(crate) fn account_rows(v: &[Conta]) -> Vec<GitAccountRow> {
    v.iter()
        .map(|c| {
            let (auth, is_ssh) = match &c.auth {
                Auth::Ssh { chave } => (format!("ssh:{chave}"), true),
                Auth::Gh => ("gh".to_string(), false),
            };
            GitAccountRow {
                rotulo: c.rotulo.clone().into(),
                usuario: c.usuario.clone().into(),
                email: c.email.clone().into(),
                servico: c.servico.clone().into(),
                auth: auth.into(),
                is_ssh,
                alias_ok: is_ssh && aplicar::alias_configurado(c),
                op_label: SharedString::new(),
                op_error: false,
            }
        })
        .collect()
}

/// Estado local dos repositórios → linhas.
///
/// `conta` mostra o rótulo da conta cadastrada em uso; quando NENHUMA casa, mostra
/// `? <e-mail>` com `known_account = false` — a UI pinta em warn, porque é
/// exatamente o caso em que o commit sai com o autor errado.
pub(crate) fn proj_rows(v: &[EstadoLocal]) -> Vec<GitProjRow> {
    v.iter()
        .enumerate()
        .map(|(i, e)| {
            let conhecida = e.conta.is_some();
            GitProjRow {
                idx: i as i32,
                nome: e.nome.clone().into(),
                raiz: e.raiz.display().to_string().into(),
                conta: match &e.conta {
                    Some(r) => r.clone().into(),
                    None if e.email.is_empty() => {
                        tor("gui.git_no_identity", "sem identidade").into()
                    }
                    None => format!("? {}", e.email).into(),
                },
                known_account: conhecida,
                remoto: e.remoto.clone().unwrap_or_default().into(),
                unpushed: e.nao_enviados as i32,
                sujo: e.sujo,
            }
        })
        .collect()
}

/// Quantos projetos têm commit que só existe nesta máquina.
pub(crate) fn em_risco(v: &[EstadoLocal]) -> i32 {
    v.iter().filter(|e| e.nao_enviados > 0).count() as i32
}

/// Repositórios do serviço → linhas.
pub(crate) fn repo_rows(v: &[Remoto]) -> Vec<GitRepoRow> {
    v.iter()
        .map(|r| GitRepoRow {
            caminho: r.caminho.clone().into(),
            privado: r.privado,
            descricao: r.descricao.clone().into(),
            atualizado: r.atualizado.clone().into(),
        })
        .collect()
}

/// Commits de um repositório → linhas (reusa o `CommitRow` do histórico do overdev).
pub(crate) fn commit_rows(v: &[githist::Commit]) -> Vec<CommitRow> {
    v.iter()
        .map(|c| CommitRow {
            short: c.short.clone().into(),
            date: c.date.clone().into(),
            author: c.author.clone().into(),
            subject: c.subject.clone().into(),
            pushed: c.pushed,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado(nome: &str, conta: Option<&str>, email: &str, nao: usize) -> EstadoLocal {
        EstadoLocal {
            nome: nome.into(),
            raiz: PathBuf::from(format!("/dev/{nome}")),
            conta: conta.map(|s| s.to_string()),
            email: email.into(),
            remoto: Some("git@github.com:org/x.git".into()),
            nao_enviados: nao,
            sujo: false,
        }
    }

    /// Identidade que não casa com conta cadastrada tem de ficar VISÍVEL como tal —
    /// é o estado que produz commit com o autor errado.
    #[test]
    fn identidade_desconhecida_e_marcada() {
        let v = vec![estado("a", Some("pessoal"), "eu@x", 0), estado("b", None, "outro@y", 3)];
        let rows = proj_rows(&v);
        assert!(rows[0].known_account);
        assert_eq!(rows[0].conta, "pessoal");
        assert!(!rows[1].known_account, "e-mail fora do cadastro não é conta conhecida");
        assert!(rows[1].conta.contains("outro@y"), "mostra QUAL identidade está em uso");
    }

    /// O índice da linha aponta pra posição na lista completa (arg das ações).
    #[test]
    fn indice_acompanha_a_lista() {
        let v = vec![estado("a", None, "", 0), estado("b", None, "", 0), estado("c", None, "", 0)];
        let rows = proj_rows(&v);
        assert_eq!(rows.iter().map(|r| r.idx).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    /// "Em risco" conta projeto com commit não enviado — é o número do aviso.
    #[test]
    fn conta_o_que_nao_saiu_da_maquina() {
        let v = vec![estado("a", None, "", 0), estado("b", None, "", 2), estado("c", None, "", 7)];
        assert_eq!(em_risco(&v), 2);
    }

    /// A linha da conta NUNCA carrega segredo — só o nome do arquivo de chave.
    #[test]
    fn linha_de_conta_nao_carrega_segredo() {
        let c = Conta {
            rotulo: "pessoal".into(),
            usuario: "eu".into(),
            email: "eu@x".into(),
            servico: "github.com".into(),
            auth: Auth::Ssh { chave: "id_ed25519".into() },
        };
        let r = &account_rows(&[c])[0];
        assert_eq!(r.auth, "ssh:id_ed25519");
        assert!(r.is_ssh);
        let tudo = format!("{} {} {} {}", r.rotulo, r.usuario, r.email, r.auth);
        assert!(!tudo.contains("BEGIN"), "nada de material de chave na UI");
    }
}
