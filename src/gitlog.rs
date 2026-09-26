//! O histórico de commits do projeto — lido do BINÁRIO `schematize-git`, não de um crate.
//!
//! **O quê:** roda `schematize-git log --json` na raiz do projeto e devolve os commits e o
//! upstream que a aba Overdev desenha.
//!
//! **Onde:** [`crate::odhistory::refresh_od_history`].
//!
//! ## Por que isto existe, e por que é subprocesso
//!
//! A E2 da extradição tirou o `githist` do crate do hub. A tela de Git foi junto — mas a aba
//! **Overdev** também lia commits, para mostrar ao lado dos snapshots o que já foi enviado.
//!
//! Havia duas saídas. Manter o `githist` no crate até a E6 seria adiar a extradição por causa
//! de um consumidor secundário, e deixar o hub com uma cópia do domínio que o ADR-0019 já
//! tirou dele. Perguntar ao binário é o que TODAS as outras fronteiras deste hub fazem desde
//! o ADR-0010, e custa um subprocesso por atualização da aba — o mesmo que a leitura de
//! ambiente já custava.
//!
//! **Ausência do app não derruba nada** (piso 10): sem `schematize-git` instalado, o histórico
//! sai vazio e o resto da aba Overdev continua funcionando. Snapshot e commit são duas
//! perguntas, e perder a resposta boa por causa da que falhou seria trocar informação por nada.

/// Um commit, no mínimo que a aba desenha.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Commit {
    pub(crate) short: String,
    pub(crate) author: String,
    pub(crate) date: String,
    pub(crate) subject: String,
    /// Já foi enviado? É a única coisa que a tela colore.
    pub(crate) pushed: bool,
}

/// O ramo e a distância dele para o remoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Upstream {
    pub(crate) branch: String,
    pub(crate) remote: Option<String>,
    pub(crate) ahead: usize,
    pub(crate) behind: usize,
}

/// **O quê:** o documento do `log --json`, já em `(upstream, commits)`.
///
/// **Onde:** [`crate::odhistory::refresh_od_history`].
///
/// **Devolve vazio em vez de erro**, e é a escolha certa AQUI: esta é uma lista secundária da
/// aba Overdev, e um projeto que não é repositório git é um caso normal, não um defeito. O
/// erro de verdade — app ausente — já aparece na aba de Git, que é dele.
pub(crate) fn ler(raiz: &std::path::Path) -> (Option<Upstream>, Vec<Commit>) {
    let Some(bin) = crate::sysenv::git_bin() else { return (None, Vec::new()) };
    let saida = std::process::Command::new(bin)
        .args(["log", "--limite", "50", "--json"])
        .current_dir(raiz)
        .stdin(std::process::Stdio::null())
        .output();
    let Ok(o) = saida else { return (None, Vec::new()) };
    if !o.status.success() {
        return (None, Vec::new());
    }
    interpretar(&String::from_utf8_lossy(&o.stdout))
}

/// **O quê:** o mesmo que [`ler`], a partir do TEXTO. PURA, e é onde os testes entram.
///
/// **Onde:** [`ler`]. Separada porque o comportamento com documento truncado, vazio, hostil ou
/// de outra versão tem de ser afirmável sem ter o app instalado na máquina de quem roda a suíte.
pub(crate) fn interpretar(texto: &str) -> (Option<Upstream>, Vec<Commit>) {
    let Some(doc) = json_minimo(texto) else { return (None, Vec::new()) };
    (doc.0, doc.1)
}

/// O parser do documento. Vive aqui e não numa lib porque o contrato é de DOIS campos.
fn json_minimo(texto: &str) -> Option<(Option<Upstream>, Vec<Commit>)> {
    let v: serde_json::Value = serde_json::from_str(texto).ok()?;
    let up = v.get("upstream").and_then(|u| {
        // **`branch` ausente derruba o upstream inteiro**, e não vira string vazia: "→ origin"
        // sem ramo nenhum é uma linha que afirma um estado que ninguém mediu.
        let branch = u.get("branch")?.as_str()?.to_string();
        Some(Upstream {
            branch,
            remote: u.get("remote").and_then(|r| r.as_str()).map(str::to_string),
            ahead: u.get("ahead").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
            behind: u.get("behind").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
        })
    });
    let commits = v
        .get("commits")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .map(|c| Commit {
                    short: campo(c, "short"),
                    author: campo(c, "author"),
                    date: campo(c, "date"),
                    subject: campo(c, "subject"),
                    // Ausente ou de outro tipo conta como NÃO enviado. É o lado seguro: dizer
                    // "já foi" sobre um commit que só existe aqui é a frase que faz alguém
                    // formatar a máquina tranquilo.
                    pushed: c.get("pushed").and_then(|b| b.as_bool()) == Some(true),
                })
                .collect()
        })
        .unwrap_or_default();
    Some((up, commits))
}

fn campo(v: &serde_json::Value, chave: &str) -> String {
    v.get(chave).and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"upstream":{"branch":"main","remote":"origin/main","ahead":2,"behind":0},
      "commits":[
        {"short":"73847f7","author":"a","date":"2026-09-26","subject":"x","pushed":true},
        {"short":"abf3cd3","author":"b","date":"2026-09-25","subject":"y","pushed":false}]}"#;

    #[test]
    fn le_o_documento() {
        let (up, cs) = interpretar(DOC);
        let up = up.expect("há upstream");
        assert_eq!((up.branch.as_str(), up.ahead), ("main", 2));
        assert_eq!(up.remote.as_deref(), Some("origin/main"));
        assert_eq!(cs.len(), 2);
        assert!(cs[0].pushed);
        assert!(!cs[1].pushed);
    }

    /// **`pushed` ausente ou de outro tipo conta como NÃO enviado.**
    ///
    /// É o lado seguro: dizer "já foi" sobre um commit que só existe nesta máquina é a frase
    /// que faz alguém formatar o computador tranquilo.
    #[test]
    fn pushed_duvidoso_conta_como_nao_enviado() {
        let (_, cs) = interpretar(r#"{"commits":[{"short":"a"},{"short":"b","pushed":"sim"}]}"#);
        assert_eq!(cs.len(), 2);
        assert!(!cs[0].pushed, "campo ausente não afirma envio");
        assert!(!cs[1].pushed, "texto não vira `true`");
    }

    /// **`branch` ausente derruba o upstream inteiro.** Uma linha "→ origin" sem ramo nenhum
    /// afirma um estado que ninguém mediu.
    #[test]
    fn upstream_sem_ramo_nao_existe() {
        let (up, _) = interpretar(r#"{"upstream":{"remote":"origin/main","ahead":9}}"#);
        assert!(up.is_none());
    }

    /// **Documento ilegível vira VAZIO, e não pânico.** Esta é uma lista secundária da aba
    /// Overdev: um projeto que não é repositório git é caso normal, não defeito.
    #[test]
    fn entrada_hostil_vira_vazio_sem_panicar() {
        let fundo = format!("{}{}", "[".repeat(3000), "]".repeat(3000));
        for lixo in ["", "null", "[]", "0", "{ nao e json", "\u{0}", &fundo, r#"{"commits":7}"#] {
            let (up, cs) = interpretar(lixo);
            assert!(up.is_none() && cs.is_empty(), "{lixo:?}");
        }
    }
}
