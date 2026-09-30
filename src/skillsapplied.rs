//! Que versão de skill moldou o projeto — lida do BINÁRIO `schematize-skills`.
//!
//! **O quê:** roda `schematize-skills applied --json` com o cwd no projeto observado e devolve
//! as skills que ficaram para trás, junto com o prompt que as reaplica.
//!
//! **Onde:** a caixa de entrada do overdev (`wire::caixa`) — o contador «N skills evoluíram» e
//! o botão que abre o agente para reaplicá-las.
//!
//! ## Por que subprocesso, e não mais `schematize::skillsproj`
//!
//! O E5 tirou o domínio de skills do hub: o `skillsproj` saiu do crate `schematize` e mora no
//! `schematize-skills`. A GUI continuou importando o módulo e só compilava porque o `Cargo.lock`
//! prendia um commit antigo do CLI; o primeiro build contra o `main` quebrou a instalação
//! inteira (`unresolved import schematize::skillsproj`). O contrato agora é o documento JSON,
//! o mesmo padrão de [`crate::skillactions`].
//!
//! **O prompt vem PRONTO do app**, e é deliberado: montá-lo aqui faria duas versões do texto que
//! o agente recebe, e a que divergisse o mandaria reaplicar com instruções velhas.
//!
//! **Sem o app, o resultado é VAZIO** — nenhuma skill atrasada, nenhum botão. A aba de Skills
//! já diz que o app não está instalado, e o resto da janela segue inteiro (piso 10).

use std::path::Path;

/// O que ficou para trás neste projeto.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Aplicadas {
    /// `(slug, versão aplicada, versão instalada)`.
    pub(crate) atrasadas: Vec<(String, String, String)>,
    /// O texto que o agente recebe. Vazio quando não há atrasadas.
    pub(crate) prompt: String,
}

/// **O quê:** as skills atrasadas em `root`. VAZIO sem o app, com erro ou com documento ruim.
///
/// **Onde:** `wire::caixa::atualiza_caixa` (contagem) e o callback `skills_rerun` (prompt).
///
/// **O `current_dir` é o `root`**, não o cwd da janela: a GUI observa um projeto que não é o
/// diretório de onde ela foi aberta, e o `applied --json` responde sobre o cwd dele.
pub(crate) fn ler(root: &Path) -> Aplicadas {
    let Some(bin) = crate::sysenv::skills_bin() else { return Aplicadas::default() };
    let saida = std::process::Command::new(bin)
        .args(["applied", "--json"])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output();
    let Ok(o) = saida else { return Aplicadas::default() };
    if !o.status.success() {
        return Aplicadas::default();
    }
    interpretar(&String::from_utf8_lossy(&o.stdout))
}

/// **O quê:** o mesmo que [`ler`], a partir do TEXTO. PURA, e é onde os testes entram.
///
/// **Onde:** [`ler`]. Separada para que documento truncado, de outra versão do app ou hostil
/// seja afirmável sem o app instalado na máquina de quem roda a suíte.
pub(crate) fn interpretar(texto: &str) -> Aplicadas {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(texto) else {
        return Aplicadas::default();
    };
    let Some(itens) = v.get("atrasadas").and_then(|a| a.as_array()) else {
        return Aplicadas::default();
    };
    let txt = |x: &serde_json::Value, c: &str| {
        x.get(c).and_then(|s| s.as_str()).unwrap_or_default().trim().to_string()
    };
    let atrasadas: Vec<(String, String, String)> = itens
        .iter()
        .filter_map(|a| {
            let slug = txt(a, "slug");
            // Sem slug não há o que nomear na tela nem o que reaplicar.
            (!slug.is_empty()).then(|| (slug, txt(a, "aplicada"), txt(a, "instalada")))
        })
        .collect();
    // **Prompt sem atrasadas é descartado**, e o inverso também zera tudo: um botão que abre um
    // agente sem tarefa, ou uma contagem sem prompt para agir sobre ela, promete o que não faz.
    let prompt = txt(&v, "prompt");
    if atrasadas.is_empty() || prompt.is_empty() {
        return Aplicadas::default();
    }
    Aplicadas { atrasadas, prompt }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"raiz":"/p","atrasadas":[
      {"slug":"rust","aplicada":"1.9.0","instalada":"1.10.0"},
      {"slug":"web","aplicada":"1.15.0","instalada":"1.16.0"}],
      "prompt":"Este projeto foi moldado por versões ANTIGAS\n  x applied --mark rust"}"#;

    #[test]
    fn le_as_atrasadas_e_o_prompt() {
        let a = interpretar(DOC);
        assert_eq!(a.atrasadas.len(), 2);
        assert_eq!(a.atrasadas[0], ("rust".into(), "1.9.0".into(), "1.10.0".into()));
        assert!(a.prompt.contains("applied --mark rust"), "o prompt vem do app, intacto");
    }

    /// **Nada atrasado é VAZIO de verdade** — o formato que o app emite quando está tudo em dia.
    #[test]
    fn em_dia_e_vazio() {
        assert_eq!(
            interpretar(r#"{"raiz":"/p","atrasadas":[],"prompt":""}"#),
            Aplicadas::default()
        );
    }

    /// **Contagem sem prompt, ou prompt sem contagem, zera tudo.** Um botão que abre um agente
    /// sem tarefa promete o que não faz.
    #[test]
    fn metade_do_documento_nao_vira_botao() {
        let sem_prompt = r#"{"atrasadas":[{"slug":"rust","aplicada":"1","instalada":"2"}]}"#;
        assert_eq!(interpretar(sem_prompt), Aplicadas::default());
        let sem_slug = r#"{"atrasadas":[{"slug":"  ","aplicada":"1"}],"prompt":"faça"}"#;
        assert_eq!(interpretar(sem_slug), Aplicadas::default());
    }

    /// **Documento ruim vira vazio, nunca pânico** (piso 10).
    #[test]
    fn documento_ruim_vira_vazio() {
        let fundo = format!("{}{}", "[".repeat(3000), "]".repeat(3000));
        for lixo in ["", "null", "[]", "0", "{ nao e json", "\u{0}", &fundo, r#"{"atrasadas":7}"#] {
            assert_eq!(interpretar(lixo), Aplicadas::default(), "{lixo:?}");
        }
    }

    /// Sem o app no PATH (ou com `root` inexistente) o resultado é vazio, não erro.
    #[test]
    fn root_inexistente_e_vazio() {
        let a = ler(Path::new("/nao/existe/schematize-teste"));
        assert_eq!(a, Aplicadas::default());
    }
}
