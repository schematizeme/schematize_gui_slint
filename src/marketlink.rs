//! A ponte para o MARKET — o que o hub ainda precisa saber de environments, pelo dono deles.
//!
//! **O quê:** lê `schematize-market list --json` e devolve, por linguagem, os métodos de
//! instalação disponíveis nesta máquina.
//!
//! **Onde:** o MODAL do marketplace, que oferece instalar o environment de uma linguagem junto
//! com a skill e precisa desenhar os chips de método.
//!
//! ## Por que este arquivo substituiu uma cópia inteira do módulo
//!
//! O hub carregava `src/environments/` — nove arquivos, uma cópia do que vive no market. Os
//! ADR-0010/0012 disseram que ela sairia, e o comando `env` já saiu: o `--help` do hub manda
//! `env -> schematize-market` há dois commits. O que segurava a cópia era **esta** leitura, e
//! só ela.
//!
//! As duas cópias já tinham divergido, e num sentido só: o market ganhou o caminho de repo de
//! fornecedor (o que consertou o `exit 104` do `csharp`/zypper), e o hub ficou com a versão
//! anterior — a que **recusava** e mandava a pessoa adicionar o repo da Microsoft à mão. Duas
//! cópias do mesmo domínio não divergem "se"; divergem quando.
//!
//! ## Por que subprocesso, e não uma dependência de crate
//!
//! Depender do crate `market` traria o domínio de volta para dentro do hub por outra porta, e
//! amarraria as versões dos dois: o hub passaria a compilar com o market de um commit
//! específico, que é exatamente o mecanismo (`gui-pin.txt`, `cargo update --precise`) que este
//! ecossistema já paga caro para manter entre CLI e GUI.
//!
//! Com subprocesso, o hub fala com o market **que está instalado** — e, quando não há market,
//! degrada em vez de quebrar (piso 10).

use std::collections::{HashMap, HashSet};

/// O que o modal precisa saber. Vazio quando o market não está instalado.
///
/// **Vazio NÃO é erro**, e é o piso 10 em ação: o hub abre e funciona sem o market. O modal
/// simplesmente não oferece o environment — a skill instala do mesmo jeito.
#[derive(Debug, Default, Clone)]
pub(crate) struct Environments {
    /// linguagem → métodos disponíveis (`docker`, `mise`, `distro`, `official`).
    pub metodos: HashMap<String, Vec<String>>,
    /// As linguagens que TÊM environment. Ferramentas (claude/code/codex) ficam de fora: elas
    /// não entram na oferta do modal.
    pub linguagens: HashSet<String>,
}

/// **O quê:** roda `schematize-market list --json` e lê os métodos por linguagem.
///
/// **Onde:** a montagem do estado inicial do hub, uma vez.
///
/// **Best-effort, e mudo de propósito.** Um market ausente não é um erro do hub: ele abre e
/// funciona sem, e é o piso 10 (cada app é entidade à parte). Falar aqui poria um aviso na
/// abertura de quem nunca vai usar o modal.
pub(crate) fn ler() -> Environments {
    let Some(bin) = crate::sysenv::market_bin_opcional() else { return Environments::default() };
    let Ok(saida) = std::process::Command::new(bin)
        .args(["list", "--json"])
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return Environments::default();
    };
    if !saida.status.success() {
        return Environments::default();
    }
    parse(&String::from_utf8_lossy(&saida.stdout))
}

/// **O quê:** extrai `slug` e `methods` de cada item de `languages`. PURA.
///
/// **Onde:** [`ler`], e os testes — é a metade que não toca em processo nenhum.
///
/// ## Um leitor pequeno, e o que ele NÃO tenta ser
///
/// São dois campos de uma lista, num documento cujo shape é travado por teste do lado do
/// market (`as_chaves_do_contrato_estao_todas_la`). Trazer um parser de JSON para o hub por
/// causa disto seria uma dependência a mais numa árvore que já tem 226 crates.
///
/// **Ele lê só `languages`**, e é isso que deixa `tools` de fora sem precisar filtrar por
/// categoria: o contrato já separa os dois em listas diferentes. A versão anterior desta
/// leitura filtrava `category == "language"` no meio de uma lista única, e essa filtragem é
/// justamente o tipo de regra que se esquece de atualizar.
///
/// **Shape inesperado devolve vazio, nunca pânico.** O hub tem de abrir mesmo com um market de
/// outra versão do outro lado.
fn parse(texto: &str) -> Environments {
    let mut out = Environments::default();
    // Recorta a lista `languages` — o documento tem `languages`, `tools` e `apps`, e ler o
    // texto inteiro casaria itens das outras duas.
    let Some(i) = texto.find("\"languages\"") else { return out };
    let resto = &texto[i..];
    let fim = resto.find("\"tools\"").unwrap_or(resto.len());
    for linha in resto[..fim].lines() {
        let Some(slug) = valor(linha, "slug") else { continue };
        let metodos = lista(linha, "methods");
        out.linguagens.insert(slug.clone());
        out.metodos.insert(slug, metodos);
    }
    out
}

/// **O quê:** o valor de string de uma chave, DENTRO de uma linha. **Onde:** [`parse`].
///
/// Por linha, e não no documento inteiro: cada item do contrato ocupa uma linha, e é isso que
/// impede a busca de casar o `slug` do primeiro item ao ler o segundo.
fn valor(linha: &str, chave: &str) -> Option<String> {
    let marca = format!("\"{chave}\": \"");
    let i = linha.find(&marca)? + marca.len();
    let j = linha[i..].find('"')? + i;
    Some(linha[i..j].to_string())
}

/// **O quê:** os itens de uma lista de strings, DENTRO de uma linha. **Onde:** [`parse`].
fn lista(linha: &str, chave: &str) -> Vec<String> {
    let marca = format!("\"{chave}\": [");
    let Some(i) = linha.find(&marca) else { return Vec::new() };
    let i = i + marca.len();
    let Some(j) = linha[i..].find(']') else { return Vec::new() };
    linha[i..i + j]
        .split(',')
        .map(|p| p.trim().trim_matches('"').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saída real do `schematize-market list --json`, recortada.
    const SAIDA: &str = r#"{
  "market": "0.2.1",
  "languages": [
    {"slug": "go", "display": "Go", "category": "language", "methods": ["docker", "mise", "distro", "official"], "installed_via": "mise", "present": true, "provenance": "mise", "hint": "docker, mise", "status_text": "via mise"},
    {"slug": "rust", "display": "Rust", "category": "language", "methods": ["mise", "official"], "installed_via": null, "present": true, "provenance": "distro", "hint": "mise", "status_text": "via distro (cargo1.97)"}
  ],
  "tools": [
    {"slug": "gh", "display": "GitHub CLI", "category": "tool", "methods": [], "installed_via": null, "present": true, "provenance": "distro", "hint": "distro", "status_text": "via distro (gh)"}
  ],
  "apps": [
    {"bin": "schematize-deployer", "about": "deploy", "state": "installed", "version": "0.7.2"}
  ]
}"#;

    /// Cada linguagem traz os SEUS métodos — o bug que a leitura por linha evita: uma busca no
    /// documento inteiro casaria o `slug` do primeiro item ao ler o segundo.
    #[test]
    fn cada_linguagem_traz_os_proprios_metodos() {
        let e = parse(SAIDA);
        assert_eq!(e.metodos["go"], vec!["docker", "mise", "distro", "official"]);
        assert_eq!(e.metodos["rust"], vec!["mise", "official"]);
    }

    /// **Só `languages`.** Ferramentas e apps não entram na oferta do modal, e o contrato já
    /// os separa em listas próprias — não é preciso filtrar por categoria, o que é justamente
    /// o tipo de regra que se esquece de atualizar.
    #[test]
    fn ferramentas_e_apps_ficam_de_fora() {
        let e = parse(SAIDA);
        assert_eq!(e.linguagens.len(), 2);
        assert!(e.linguagens.contains("go") && e.linguagens.contains("rust"));
        assert!(!e.linguagens.contains("gh"), "ferramenta não é linguagem");
        assert!(!e.linguagens.contains("schematize-deployer"), "app não é linguagem");
    }

    /// **Shape inesperado devolve vazio, nunca pânico.** O hub tem de abrir mesmo com um market
    /// de outra versão do outro lado — ou com nenhum.
    #[test]
    fn entrada_hostil_devolve_vazio_sem_panicar() {
        for lixo in ["", "isto nao e json", "{}", r#"{"languages":"#, r#"{"languages": []}"#] {
            let e = parse(lixo);
            assert!(e.linguagens.is_empty(), "{lixo:?}");
            assert!(e.metodos.is_empty(), "{lixo:?}");
        }
        // Item sem `methods`: a linguagem entra, com lista vazia. É melhor oferecer a
        // linguagem sem chip do que sumir com ela da oferta por causa de um campo.
        let e = parse(r#"{"languages":[{"slug": "go"}],"tools":[]}"#);
        assert!(e.linguagens.contains("go"));
        assert!(e.metodos["go"].is_empty());
    }
}
