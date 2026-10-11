//! A PORTA ÚNICA do hub para o `schematize-overdev` — a casca que roda o binário e lê o `--json`.
//!
//! **O quê:** resolve o caminho ABSOLUTO do app, roda um subcomando COM o projeto como diretório
//! corrente e devolve o texto ou o MOTIVO da falha; e traduz cada documento `--json` num tipo
//! próprio desta GUI.
//!
//! **Onde:** a aba Overdev (terminal e painel HTML), a aba Grafo e a chave dos hooks nas
//! Configurações. Nada fora daqui chama o app. O checklist, a execução, a caixa, as notas e o
//! split moraram aqui até a janela `schematize-overdev-gui` portá-los (E6 M5): o hub não os lê
//! mais.
//!
//! ## Por que existe
//!
//! A E6 do ADR-0018 tirou o motor do overdev do crate `schematize` (overdev, panel, agentrun,
//! settings, usage, agents, overdevdb). O hub importava esses módulos; o contrato agora é o
//! documento JSON (mesmo padrão de [`crate::skillactions`] e [`crate::skillsapplied`]). Um só
//! lugar fala com o app, para que «o app não está instalado» tenha UMA frase e UMA dica.
//!
//! ## Independência (piso 10)
//!
//! O hub SOBE sem o app. Ausência é um `Err` com texto acionável — nunca pânico, nunca tela
//! calada. Erro de execução volta como motivo (stderr do app), nunca como lista vazia.
//!
//! ## Leitura pura
//!
//! Cada `interpretar_*` é PURA (recebe o texto) para que documento truncado, de outra versão ou
//! hostil seja afirmável sem o app instalado na máquina de quem roda a suíte.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

/// A frase de «app ausente». Uma só: a tela e o log dizem a mesma coisa.
pub(crate) const AUSENTE: &str = "o app `schematize-overdev` não está instalado nesta máquina";

/// **O quê:** o app está instalado? **Onde:** os estados «instale» da aba Overdev e dos hooks.
pub(crate) fn presente() -> bool {
    crate::sysenv::overdev_bin().is_some()
}

/// **O quê:** roda `schematize-overdev <args…>` com `dir` como cwd e devolve o stdout (aparado).
///
/// **Onde:** toda leitura e toda ação deste módulo e dos `wire::*`.
///
/// **Devolve o MOTIVO, nunca vazio:** sem o app é [`AUSENTE`]; com falha de execução é o stderr
/// do app (ou o código de saída se ele calou). O chamador mostra o texto — não adivinha.
/// **Efeitos:** cria um processo; o app pode escrever no projeto (as ações) ou só ler.
pub(crate) fn rodar(dir: &Path, args: &[&str]) -> Result<String, String> {
    let bin = crate::sysenv::overdev_bin().ok_or_else(|| AUSENTE.to_string())?;
    let out = Command::new(&bin)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("não consegui executar {bin}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        return Err(if err.is_empty() {
            format!("`schematize-overdev {}` falhou ({})", args.join(" "), out.status)
        } else {
            err.to_string()
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// **O quê:** parseia um documento JSON ou diz que ele veio ilegível. **Onde:** os `interpretar_*`.
fn doc(texto: &str) -> Result<Value, String> {
    serde_json::from_str(texto).map_err(|e| format!("resposta ilegível do schematize-overdev: {e}"))
}

/// **O quê:** um campo de texto (ausente, `null` ou de outro tipo vale `""`). **Onde:** idem.
fn txt(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// **O quê:** um campo de texto que pode ser `null` (ausente = `None`, vazio = `None`).
fn txt_opt(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

// ---------------------------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------------------------

/// **O quê:** `hooks --json` → os hooks do Claude Code estão ligados? PURA. **Onde:** [`hooks_ligados`].
pub(crate) fn interpretar_hooks(texto: &str) -> Result<bool, String> {
    doc(texto)?
        .get("ligados")
        .and_then(Value::as_bool)
        .ok_or_else(|| "resposta ilegível do schematize-overdev: faltou `ligados`".to_string())
}

/// **O quê:** os hooks do overdev estão registrados no `settings.json`?
///
/// **Onde:** o estado inicial e a releitura da chave de Configurações. `Err` (app ausente ou
/// ilegível) NÃO é «desligado»: a tela precisa dizer o motivo em vez de mostrar uma chave que
/// afirma o que não sabe.
pub(crate) fn hooks_ligados(dir: &Path) -> Result<bool, String> {
    interpretar_hooks(&rodar(dir, &["hooks", "--json"])?)
}

/// **O quê:** liga (`true`) ou desliga (`false`) os hooks. **Onde:** a chave de Configurações.
pub(crate) fn definir_hooks(dir: &Path, ligar: bool) -> Result<(), String> {
    rodar(dir, &[if ligar { "enable" } else { "disable" }]).map(|_| ())
}

// ---------------------------------------------------------------------------------------------
// Grafo
// ---------------------------------------------------------------------------------------------

/// Um nó do grafo do índice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoGrafo {
    /// Nome (função/serviço).
    pub(crate) id: String,
    /// `arquivo:linha`, se conhecido.
    pub(crate) loc: Option<String>,
}

/// Uma aresta dirigida (`de -> para`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArestaGrafo {
    /// Origem.
    pub(crate) de: String,
    /// Destino.
    pub(crate) para: String,
}

/// O grafo que a aba desenha.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Grafo {
    /// A visão global foi agregada por serviço (passou do teto de nós)?
    pub(crate) agregado: bool,
    /// Nós.
    pub(crate) nos: Vec<NoGrafo>,
    /// Arestas.
    pub(crate) arestas: Vec<ArestaGrafo>,
    /// Nó → «O quê» do MAPA, para o bloco lateral.
    pub(crate) descricoes: HashMap<String, String>,
}

/// **O quê:** `graph json` → [`Grafo`]. PURA. **Onde:** [`grafo`].
///
/// Aresta sem origem ou destino é descartada (não há como desenhá-la); o rótulo do contrato não
/// é usado pela tela e por isso não é lido.
pub(crate) fn interpretar_grafo(texto: &str) -> Result<Grafo, String> {
    let v = doc(texto)?;
    let nos = v
        .get("nos")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|n| {
                    let id = txt(n, "id");
                    (!id.is_empty()).then(|| NoGrafo { id, loc: txt_opt(n, "loc") })
                })
                .collect()
        })
        .unwrap_or_default();
    let arestas = v
        .get("arestas")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let (de, para) = (txt(e, "de"), txt(e, "para"));
                    (!de.is_empty() && !para.is_empty()).then_some(ArestaGrafo { de, para })
                })
                .collect()
        })
        .unwrap_or_default();
    let descricoes = v
        .get("descricoes")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter().filter_map(|(k, d)| d.as_str().map(|s| (k.clone(), s.to_string()))).collect()
        })
        .unwrap_or_default();
    Ok(Grafo {
        agregado: v.get("agregado").and_then(Value::as_bool).unwrap_or(false),
        nos,
        arestas,
        descricoes,
    })
}

/// **O quê:** o grafo do índice de `dir` — global, ou o detalhe de `servico`.
/// **Onde:** a aba Grafo (carga e drill-down). Índice ausente é grafo vazio, não erro.
pub(crate) fn grafo(dir: &Path, servico: Option<&str>) -> Result<Grafo, String> {
    let mut args = vec!["graph", "json"];
    if let Some(s) = servico {
        args.extend(["--servico", s]);
    }
    interpretar_grafo(&rodar(dir, &args)?)
}

/// **O quê:** o diretório do vault depois de `graph obsidian`. PURA. **Onde:** [`exportar_obsidian`].
pub(crate) fn interpretar_vault(saida: &str) -> Result<String, String> {
    saida
        .lines()
        .find_map(|l| l.strip_prefix("vault Obsidian:"))
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .ok_or_else(|| format!("o export não disse onde gravou: {saida}"))
}

/// **O quê:** exporta o índice de `dir` como vault Obsidian e devolve a pasta.
/// **Onde:** o botão «Exportar» da aba Grafo.
pub(crate) fn exportar_obsidian(dir: &Path) -> Result<String, String> {
    interpretar_vault(&rodar(dir, &["graph", "obsidian"])?)
}

// ---------------------------------------------------------------------------------------------
// Ações
// ---------------------------------------------------------------------------------------------

/// **O quê:** `{"terminal":"<nome>"}` → o nome do terminal aberto. PURA. **Onde:** [`terminal`].
pub(crate) fn interpretar_terminal(texto: &str) -> Result<String, String> {
    let nome = txt(&doc(texto)?, "terminal");
    (!nome.is_empty())
        .then_some(nome)
        .ok_or_else(|| "resposta ilegível do schematize-overdev: faltou `terminal`".to_string())
}

/// Como abrir o terminal.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Terminal<'a> {
    /// Interativo, com o `claude` pronto.
    Vazio,
    /// Com um prompt livre.
    Prompt(&'a str),
    /// Com uma receita do app: `archive-todos`, `reindex` ou `overdev`.
    Receita(&'a str),
}

/// **O quê:** abre um terminal externo em `dir` e devolve o NOME dele. **Onde:** «Rodar skill»,
/// «Abrir terminal», «Executar overdev», «Gerar afazeres», «Reindexar», organizar a caixa,
/// reaplicar skills. O app é quem monta o prompt e sobe o supervisor.
pub(crate) fn terminal(dir: &Path, como: Terminal<'_>) -> Result<String, String> {
    let mut args = vec!["terminal", "--json"];
    match como {
        Terminal::Vazio => {}
        Terminal::Prompt(p) => args.extend(["--prompt", p]),
        Terminal::Receita(r) => args.extend(["--receita", r]),
    }
    interpretar_terminal(&rodar(dir, &args)?)
}

/// **O quê:** abre o painel HTML do projeto no navegador. **Onde:** «Painel» (Overdev e Grafo).
pub(crate) fn abrir_painel(dir: &Path) -> Result<(), String> {
    rodar(dir, &["panel"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Documento truncado ou que não é JSON é ERRO com motivo — nunca um valor zerado.
    #[test]
    fn documento_ilegivel_e_erro_e_nao_zero() {
        for ruim in ["", "{", "não é json", "[1,2"] {
            let e = interpretar_hooks(ruim).expect_err("deve recusar");
            assert!(e.contains("ilegível"), "{ruim:?} -> {e}");
        }
        assert!(interpretar_grafo("x").is_err());
        assert!(interpretar_hooks(r#"{"settings":"valido"}"#).is_err(), "sem `ligados` não chuta");
    }

    /// Hooks: `ligados` é o que decide; `false` é verdade, não ausência.
    #[test]
    fn hooks_le_ligados() {
        assert!(interpretar_hooks(r#"{"settings":"valido","ligados":true,"em_dia":true}"#).unwrap());
        assert!(
            !interpretar_hooks(r#"{"settings":"ausente","ligados":false,"em_dia":false}"#).unwrap()
        );
    }

    /// Grafo: nó sem id e aresta incompleta são descartados; `loc` nulo é `None`; descrições vêm.
    #[test]
    fn grafo_descarta_o_que_nao_da_para_desenhar() {
        let g = interpretar_grafo(
            r#"{"servico":null,"agregado":true,
                "nos":[{"id":"api","loc":"api/x.rs:3"},{"id":"db","loc":null},{"id":""},{"loc":"z"}],
                "arestas":[{"de":"api","para":"db","rotulo":null},{"de":"api","para":""},{"para":"x"}],
                "descricoes":{"api":"faz a API","db":7}}"#,
        )
        .unwrap();
        assert!(g.agregado);
        assert_eq!(g.nos.len(), 2);
        assert_eq!(g.nos[0].loc.as_deref(), Some("api/x.rs:3"));
        assert_eq!(g.nos[1].loc, None);
        assert_eq!(g.arestas, [ArestaGrafo { de: "api".into(), para: "db".into() }]);
        assert_eq!(g.descricoes.get("api").map(String::as_str), Some("faz a API"));
        assert!(!g.descricoes.contains_key("db"), "descrição que não é texto não entra");
    }

    /// O vault sai da linha `vault Obsidian: <dir>`; sem ela é erro com a saída crua.
    #[test]
    fn vault_vem_da_linha_certa() {
        assert_eq!(
            interpretar_vault("vault Obsidian: /p/x_archive/obsidian\n  Abra a pasta").unwrap(),
            "/p/x_archive/obsidian"
        );
        let e = interpretar_vault("nada aqui").expect_err("sem a linha");
        assert!(e.contains("nada aqui"), "{e}");
        assert!(interpretar_vault("vault Obsidian:   ").is_err());
    }

    /// O terminal devolve o nome; sem ele é erro.
    #[test]
    fn terminal_exige_o_nome() {
        assert_eq!(interpretar_terminal(r#"{"terminal":"konsole"}"#).unwrap(), "konsole");
        assert!(interpretar_terminal(r#"{"terminal":""}"#).is_err());
        assert!(interpretar_terminal("{}").is_err());
    }

    /// Sem o app instalado, rodar dá o motivo acionável (e não pânico nem vazio).
    #[test]
    fn rodar_sem_o_app_diz_o_motivo() {
        if presente() {
            return; // a máquina de quem roda tem o app; o caminho é coberto pelo teste de contrato.
        }
        let e = rodar(Path::new("/"), &["hooks", "--json"]).expect_err("sem app");
        assert_eq!(e, AUSENTE);
    }

    /// **Contra o BINÁRIO de verdade:** o que o app instalado devolve é o que os `interpretar_*`
    /// leem (grafo global e de serviço e hooks). Sem o app
    /// na máquina de quem roda a suíte não há contra o que provar, e o teste diz isso em voz
    /// alta em vez de passar calado.
    #[test]
    fn contrato_com_o_binario_instalado() {
        if !presente() {
            eprintln!("schematize-overdev ausente: contrato com o binário NÃO verificado aqui");
            return;
        }
        let d = std::env::temp_dir().join(format!("gui-overdevcli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let g = d.join(".schematize/grafos");
        std::fs::create_dir_all(&g).expect("dir de grafos");
        std::fs::write(g.join("GRAFO_GLOBAL.md"), "front -> api (login)\napi -> auth\n").unwrap();
        std::fs::write(
            g.join("api.md"),
            "- `handler` — recebe · api/src/h.rs:7\nhandler -> repo\n",
        )
        .unwrap();

        let global = grafo(&d, None).expect("grafo global");
        assert!(global.nos.iter().any(|n| n.id == "front") && !global.agregado, "{global:?}");
        assert!(global.arestas.iter().any(|e| e.de == "front" && e.para == "api"));
        let api = grafo(&d, Some("api")).expect("grafo do serviço");
        let h = api.nos.iter().find(|n| n.id == "handler").expect("handler");
        assert_eq!(h.loc.as_deref(), Some("api/src/h.rs:7"));
        assert!(grafo(&d, Some("nada")).expect("serviço sem grafo").nos.is_empty());

        assert!(hooks_ligados(&d).is_ok(), "hooks --json responde");
        let _ = std::fs::remove_dir_all(&d);
    }
}
