//! A PORTA ÚNICA do hub para o `schematize-overdev` — a casca que roda o binário e lê o `--json`.
//!
//! **O quê:** resolve o caminho ABSOLUTO do app, roda um subcomando COM o projeto como diretório
//! corrente e devolve o texto ou o MOTIVO da falha; e traduz cada documento `--json` num tipo
//! próprio desta GUI.
//!
//! **Onde:** a aba Overdev (carga, monitor, caixa, itens humanos, split, terminal), a aba Grafo e
//! a chave dos hooks nas Configurações. Nada fora daqui chama o app.
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

/// **O quê:** um campo numérico não negativo (ausente ou de outro tipo vale 0).
/// **Onde:** contagens dos documentos — zero é verdade para «ainda não há».
fn num(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
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
// Progresso e monitor
// ---------------------------------------------------------------------------------------------

/// Em que pé está o run (`progresso --json`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Progresso {
    /// `active` | `stopped` | `done` | `blocked`; vazio se não há run.
    pub(crate) modo: String,
    /// Itens de máquina abertos.
    pub(crate) abertos: u64,
    /// Feitos (máquina + humano).
    pub(crate) feitos: u64,
    /// On-hold (pergunta parkeada).
    pub(crate) on_hold: u64,
    /// Humanos abertos.
    pub(crate) humanos: u64,
    /// Ciclos já gastos.
    pub(crate) iteracoes: u64,
    /// Teto de ciclos (0 se não há run).
    pub(crate) teto: u64,
}

impl Progresso {
    /// **O quê:** o run terminou? Parado, ou ativo sem item de máquina aberto.
    /// **Onde:** o laço do monitor — a mesma regra do gate do app.
    pub(crate) fn terminou(&self) -> bool {
        self.modo == "stopped" || (self.modo == "active" && self.abertos == 0)
    }
}

/// **O quê:** `progresso --json` → [`Progresso`]. PURA. **Onde:** [`progresso`].
pub(crate) fn interpretar_progresso(texto: &str) -> Result<Progresso, String> {
    let v = doc(texto)?;
    Ok(Progresso {
        modo: txt(&v, "modo"),
        abertos: num(&v, "abertos"),
        feitos: num(&v, "feitos"),
        on_hold: num(&v, "on_hold"),
        humanos: num(&v, "humanos"),
        iteracoes: num(&v, "iteracoes"),
        teto: num(&v, "teto"),
    })
}

/// **O quê:** o progresso do run em `dir`. **Onde:** o monitor (a cada ~3 s: é barato).
pub(crate) fn progresso(dir: &Path) -> Result<Progresso, String> {
    interpretar_progresso(&rodar(dir, &["progresso", "--json"])?)
}

/// Uma conclusão do log do run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Conclusao {
    /// Epoch em segundos.
    pub(crate) ts: i64,
    /// O item concluído.
    pub(crate) text: String,
}

/// O que o monitor mostra: itens de máquina abertos e as conclusões recentes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Monitor {
    /// Texto dos itens abertos (sem marcador).
    pub(crate) abertos: Vec<String>,
    /// Conclusões, da mais antiga para a mais nova.
    pub(crate) conclusoes: Vec<Conclusao>,
}

/// **O quê:** `monitor --json` → [`Monitor`]. PURA. **Onde:** [`monitor`].
pub(crate) fn interpretar_monitor(texto: &str) -> Result<Monitor, String> {
    let v = doc(texto)?;
    let abertos = v
        .get("abertos")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let conclusoes = v
        .get("conclusoes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| Conclusao {
                    ts: c.get("em").and_then(Value::as_i64).unwrap_or(0),
                    text: txt(c, "texto"),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Monitor { abertos, conclusoes })
}

/// **O quê:** o monitor do run em `dir`. **Onde:** o laço do monitor (a cada ~3 s: é barato).
pub(crate) fn monitor(dir: &Path) -> Result<Monitor, String> {
    interpretar_monitor(&rodar(dir, &["monitor", "--json"])?)
}

/// Tokens e modelo do projeto, lidos dos transcripts do Claude.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Tokens {
    /// Tokens de entrada.
    pub(crate) entrada: u64,
    /// Tokens de saída.
    pub(crate) saida: u64,
    /// Leitura de cache.
    pub(crate) cache_leitura: u64,
    /// Total.
    pub(crate) total: u64,
    /// O modelo principal, se houve transcript.
    pub(crate) modelo: Option<String>,
}

/// **O quê:** `tokens --json` → [`Tokens`]. PURA. **Onde:** [`tokens`].
pub(crate) fn interpretar_tokens(texto: &str) -> Result<Tokens, String> {
    let v = doc(texto)?;
    Ok(Tokens {
        entrada: num(&v, "entrada"),
        saida: num(&v, "saida"),
        cache_leitura: num(&v, "cache_leitura"),
        total: num(&v, "total"),
        modelo: txt_opt(&v, "modelo"),
    })
}

/// **O quê:** os tokens do projeto em `dir`. **Onde:** a thread de uso do monitor.
///
/// **PESADO** (o app varre transcripts de 100 MB+): nunca no laço de 3 s, nunca no event loop.
pub(crate) fn tokens(dir: &Path) -> Result<Tokens, String> {
    interpretar_tokens(&rodar(dir, &["tokens", "--json"])?)
}

// ---------------------------------------------------------------------------------------------
// Checklist, documentos, notas, caixa
// ---------------------------------------------------------------------------------------------

/// Um item do checklist já classificado pelo app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemChecklist {
    /// `open|done|hold|cancel|hopen|hdone|hresp|hrec|nota`.
    pub(crate) tipo: String,
    /// Texto sem marcador.
    pub(crate) texto: String,
    /// Item de máquina?
    pub(crate) maquina: bool,
    /// Posição 1-based entre os humanos abertos (0 nos demais).
    pub(crate) humano_n: i32,
    /// Linha indentada (subtarefa/nota de resposta).
    pub(crate) sub: bool,
}

/// O checklist do run: cabeçalho e itens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Checklist {
    /// Modo do run (`None` sem run).
    pub(crate) modo: Option<String>,
    /// Objetivo (vazio sem run).
    pub(crate) objetivo: String,
    /// Itens na ordem do arquivo.
    pub(crate) itens: Vec<ItemChecklist>,
}

/// **O quê:** `checklist --json` → [`Checklist`]. PURA. **Onde:** [`checklist`].
pub(crate) fn interpretar_checklist(texto: &str) -> Result<Checklist, String> {
    let v = doc(texto)?;
    let itens = v
        .get("itens")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|i| ItemChecklist {
                    tipo: txt(i, "tipo"),
                    texto: txt(i, "texto"),
                    maquina: i.get("maquina").and_then(Value::as_bool).unwrap_or(false),
                    humano_n: i.get("humano_n").and_then(Value::as_i64).unwrap_or(0) as i32,
                    sub: i.get("sub").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Checklist { modo: txt_opt(&v, "modo"), objetivo: txt(&v, "objetivo"), itens })
}

/// **O quê:** o checklist do run em `dir`. **Onde:** a carga da aba Overdev.
pub(crate) fn checklist(dir: &Path) -> Result<Checklist, String> {
    interpretar_checklist(&rodar(dir, &["checklist", "--json"])?)
}

/// Decisões, plano e perguntas parkeadas do run (texto cru, mostrado dobrado).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Documentos {
    /// `DECISOES.md`.
    pub(crate) decisoes: String,
    /// `PLAN.md`.
    pub(crate) plano: String,
    /// `PERGUNTAS-OVERDEV.txt`.
    pub(crate) perguntas: String,
}

/// **O quê:** `documentos --json` → [`Documentos`]. PURA. **Onde:** [`documentos`].
pub(crate) fn interpretar_documentos(texto: &str) -> Result<Documentos, String> {
    let v = doc(texto)?;
    Ok(Documentos {
        decisoes: txt(&v, "decisoes"),
        plano: txt(&v, "plano"),
        perguntas: txt(&v, "perguntas"),
    })
}

/// **O quê:** os documentos do run em `dir`. **Onde:** a carga da aba Overdev.
pub(crate) fn documentos(dir: &Path) -> Result<Documentos, String> {
    interpretar_documentos(&rodar(dir, &["documentos", "--json"])?)
}

/// **O quê:** as notas do humano (`notas --json`) → texto. PURA. **Onde:** [`notas`].
pub(crate) fn interpretar_notas(texto: &str) -> Result<String, String> {
    Ok(txt(&doc(texto)?, "texto"))
}

/// **O quê:** as notas do humano em `dir`. **Onde:** a carga e depois de adicionar nota.
pub(crate) fn notas(dir: &Path) -> Result<String, String> {
    interpretar_notas(&rodar(dir, &["notas", "--json"])?)
}

/// **O quê:** `caixa list --json` → `(a organizar, a fundir)`. PURA. **Onde:** [`caixa`].
pub(crate) fn interpretar_caixa(texto: &str) -> Result<(usize, usize), String> {
    let v = doc(texto)?;
    let n = |k: &str| v.get(k).and_then(Value::as_array).map_or(0, Vec::len);
    Ok((n("pendentes"), n("processadas")))
}

/// **O quê:** quantas demandas há na caixa de `dir`. **Onde:** o contador da aba Overdev.
pub(crate) fn caixa(dir: &Path) -> Result<(usize, usize), String> {
    interpretar_caixa(&rodar(dir, &["caixa", "list", "--json"])?)
}

// ---------------------------------------------------------------------------------------------
// Governador, hooks
// ---------------------------------------------------------------------------------------------

/// O teto de concorrência da máquina (`agents --json`).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Orcamento {
    /// Teto seguro de claudes.
    pub(crate) teto: usize,
    /// Slots livres agora.
    pub(crate) livres: usize,
    /// Claudes rodando agora.
    pub(crate) rodando: usize,
    /// Load médio de 1 min.
    pub(crate) load1: f64,
}

/// **O quê:** `agents --json` → [`Orcamento`]. PURA. **Onde:** [`orcamento`].
pub(crate) fn interpretar_orcamento(texto: &str) -> Result<Orcamento, String> {
    let v = doc(texto)?;
    Ok(Orcamento {
        teto: num(&v, "total_cap") as usize,
        livres: num(&v, "available") as usize,
        rodando: num(&v, "running_claudes") as usize,
        load1: v.get("load1").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

/// **O quê:** o orçamento de concorrência. **Onde:** a linha «máquina» da aba Overdev.
/// O orçamento é da MÁQUINA; o cwd é só o projeto atual, por não haver um neutro.
pub(crate) fn orcamento(dir: &Path) -> Result<Orcamento, String> {
    interpretar_orcamento(&rodar(dir, &["agents", "--json"])?)
}

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

/// **O quê:** argumentos de `note` — o texto vai DEPOIS de `--`, senão uma nota que começa com
/// `-` viraria flag. PURA. **Onde:** [`adicionar_nota`].
pub(crate) fn args_nota<'a>(tipo: &'a str, texto: &'a str) -> Vec<&'a str> {
    vec!["note", "--kind", tipo, "--", texto]
}

/// **O quê:** anexa uma nota do humano (`task` ou `correcao`). **Onde:** os campos de nota.
pub(crate) fn adicionar_nota(dir: &Path, tipo: &str, texto: &str) -> Result<(), String> {
    rodar(dir, &args_nota(tipo, texto.trim())).map(|_| ())
}

/// **O quê:** captura uma demanda na caixa SEM tocar o checklist. **Onde:** «Capturar».
pub(crate) fn capturar_demanda(dir: &Path, texto: &str) -> Result<(), String> {
    rodar(dir, &["add", "--", texto.trim()]).map(|_| ())
}

/// **O quê:** fecha o `n`-ésimo (1-based) item humano aberto. **Onde:** «Marcar feito».
pub(crate) fn marcar_humano(dir: &Path, n: i32) -> Result<(), String> {
    if n < 1 {
        return Err("índice humano inválido".into());
    }
    let n = n.to_string();
    rodar(dir, &["human", "--done", &n]).map(|_| ())
}

/// **O quê:** responde (`true`) ou recusa (`false`) o `n`-ésimo item humano aberto e devolve a
/// frase do app («liberado pra máquina: …» / «cancelado: …»). **Onde:** o modal de resolver.
pub(crate) fn resolver_humano(
    dir: &Path,
    n: i32,
    responder: bool,
    texto: &str,
) -> Result<String, String> {
    let n = n.max(1).to_string();
    rodar(dir, &[if responder { "answer" } else { "refuse" }, &n, "--", texto.trim()])
}

/// **O quê:** funde as demandas organizadas no checklist; devolve a frase do app. **Onde:** «Fundir».
pub(crate) fn fundir_caixa(dir: &Path) -> Result<String, String> {
    rodar(dir, &["caixa", "merge"])
}

/// **O quê:** abre o agente que organiza a caixa num terminal. **Onde:** «Organizar».
pub(crate) fn organizar_caixa(dir: &Path) -> Result<String, String> {
    rodar(dir, &["caixa", "agente"])
}

/// **O quê:** abre o painel HTML do projeto no navegador. **Onde:** «Painel» (Overdev e Grafo).
pub(crate) fn abrir_painel(dir: &Path) -> Result<(), String> {
    rodar(dir, &["panel"]).map(|_| ())
}

/// **O quê:** divide o checklist em `k` partes (e lança `k` claudes se `lancar`); devolve o
/// resumo. **Onde:** o painel de split. O teto do governador é conferido PELO APP — a recusa
/// volta como erro com o número.
pub(crate) fn dividir(dir: &Path, k: usize, lancar: bool) -> Result<String, String> {
    let k = k.to_string();
    let mut args = vec!["split", k.as_str()];
    if lancar {
        args.push("--dispatch");
    }
    rodar(dir, &args).map(|saida| resumo_split(&saida))
}

/// **O quê:** das dezenas de linhas do `split`, só as que dizem o que aconteceu (`✓`/`✗`).
/// PURA. **Onde:** [`dividir`]. Sem nenhuma, devolve a última linha — nunca vazio.
pub(crate) fn resumo_split(saida: &str) -> String {
    let marcadas: Vec<&str> =
        saida.lines().map(str::trim).filter(|l| l.starts_with('✓') || l.starts_with('✗')).collect();
    if marcadas.is_empty() {
        saida.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("split concluído").into()
    } else {
        marcadas.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O progresso lê os nomes do contrato; sem run, `modo` e `teto` nulos viram vazio/0.
    #[test]
    fn progresso_le_o_contrato_e_o_sem_run() {
        let p = interpretar_progresso(
            r#"{"raiz":"/p","modo":"active","feitos":3,"abertos":2,"on_hold":1,"humanos":4,
                "recusados":0,"iteracoes":5,"teto":40}"#,
        )
        .expect("documento válido");
        assert_eq!(
            p,
            Progresso {
                modo: "active".into(),
                abertos: 2,
                feitos: 3,
                on_hold: 1,
                humanos: 4,
                iteracoes: 5,
                teto: 40
            }
        );
        assert!(!p.terminou());
        let vazio = interpretar_progresso(r#"{"modo":null,"feitos":0,"teto":null}"#).unwrap();
        assert_eq!((vazio.modo.as_str(), vazio.teto), ("", 0));
        assert!(Progresso { modo: "active".into(), ..Default::default() }.terminou());
        assert!(Progresso { modo: "stopped".into(), abertos: 3, ..Default::default() }.terminou());
    }

    /// Documento truncado ou que não é JSON é ERRO com motivo — nunca um progresso zerado.
    #[test]
    fn documento_ilegivel_e_erro_e_nao_zero() {
        for ruim in ["", "{", "não é json", "[1,2"] {
            let e = interpretar_progresso(ruim).expect_err("deve recusar");
            assert!(e.contains("ilegível"), "{ruim:?} -> {e}");
        }
        assert!(interpretar_monitor("{").is_err());
        assert!(interpretar_tokens("").is_err());
        assert!(interpretar_checklist("x").is_err());
        assert!(interpretar_grafo("x").is_err());
        assert!(interpretar_hooks(r#"{"settings":"valido"}"#).is_err(), "sem `ligados` não chuta");
    }

    /// Monitor: texto dos abertos e `em`/`texto` das conclusões; tipo errado não derruba.
    #[test]
    fn monitor_le_abertos_e_conclusoes() {
        let m = interpretar_monitor(
            r#"{"abertos":["a","b",3],"conclusoes":[{"em":10,"texto":"x"},{"em":"quebrado"}]}"#,
        )
        .unwrap();
        assert_eq!(m.abertos, ["a", "b"], "o que não é texto é ignorado");
        assert_eq!(m.conclusoes[0], Conclusao { ts: 10, text: "x".into() });
        assert_eq!(m.conclusoes[1], Conclusao { ts: 0, text: String::new() });
    }

    /// Tokens: zero é verdade, o modelo nulo é `None`.
    #[test]
    fn tokens_modelo_nulo_e_none() {
        let t = interpretar_tokens(
            r#"{"entrada":1,"saida":2,"cache_leitura":3,"cache_escrita":4,"total":10,
                "mensagens":5,"modelo":null}"#,
        )
        .unwrap();
        assert_eq!((t.entrada, t.saida, t.cache_leitura, t.total, t.modelo), (1, 2, 3, 10, None));
        let m = interpretar_tokens(r#"{"modelo":"claude-x"}"#).unwrap();
        assert_eq!(m.modelo.as_deref(), Some("claude-x"));
    }

    /// Checklist: o item humano carrega o número que `human --done` espera.
    #[test]
    fn checklist_traz_o_numero_humano() {
        let c = interpretar_checklist(
            r#"{"raiz":"/p","modo":"active","objetivo":"meta","itens":[
                {"tipo":"open","texto":"a","maquina":true,"humano_n":0,"sub":false},
                {"tipo":"hopen","texto":"b","maquina":false,"humano_n":1,"sub":false},
                {"tipo":"nota","texto":"resposta: x","maquina":false,"humano_n":0,"sub":true}]}"#,
        )
        .unwrap();
        assert_eq!((c.modo.as_deref(), c.objetivo.as_str()), (Some("active"), "meta"));
        assert_eq!(c.itens.len(), 3);
        assert_eq!((c.itens[1].tipo.as_str(), c.itens[1].humano_n), ("hopen", 1));
        assert!(c.itens[2].sub && !c.itens[2].maquina);
        assert!(interpretar_checklist(r#"{"modo":null,"objetivo":null,"itens":[]}"#)
            .unwrap()
            .modo
            .is_none());
    }

    /// Documentos, notas e caixa.
    #[test]
    fn documentos_notas_e_caixa() {
        let d = interpretar_documentos(r#"{"decisoes":"d","plano":"p","perguntas":"q"}"#).unwrap();
        assert_eq!((d.decisoes.as_str(), d.plano.as_str(), d.perguntas.as_str()), ("d", "p", "q"));
        assert_eq!(interpretar_notas(r#"{"texto":"oi\nlinha"}"#).unwrap(), "oi\nlinha");
        assert_eq!(
            interpretar_caixa(
                r#"{"pendentes":[{"id":"1"},{"id":"2"}],"processadas":[{"id":"3"}]}"#
            )
            .unwrap(),
            (2, 1)
        );
        assert_eq!(interpretar_caixa(r#"{"pendentes":[],"processadas":[]}"#).unwrap(), (0, 0));
    }

    /// Orçamento e hooks.
    #[test]
    fn orcamento_e_hooks() {
        let o = interpretar_orcamento(
            r#"{"total_cap":8,"available":5,"running_claudes":3,"load1":1.5,"ram_tight":false}"#,
        )
        .unwrap();
        assert_eq!((o.teto, o.livres, o.rodando, o.load1), (8, 5, 3, 1.5));
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

    /// Texto que começa com `-` fica depois do `--` e não vira flag; o split resume o que importa.
    #[test]
    fn argumentos_e_resumo_do_split() {
        assert_eq!(args_nota("task", "-rf"), ["note", "--kind", "task", "--", "-rf"]);
        let saida = "Governador…\n  teto: 8\n\n✓ dividido: 6 itens em 2 parte(s):\n  part 1\n  ✓ claude 1 lançado (a)\n  ✗ claude 2 falhou: x";
        assert_eq!(
            resumo_split(saida),
            "✓ dividido: 6 itens em 2 parte(s): · ✓ claude 1 lançado (a) · ✗ claude 2 falhou: x"
        );
        assert_eq!(resumo_split("a\nfim\n\n"), "fim");
        assert_eq!(resumo_split(""), "split concluído");
    }

    /// Sem o app instalado, rodar dá o motivo acionável (e não pânico nem vazio).
    #[test]
    fn rodar_sem_o_app_diz_o_motivo() {
        if presente() {
            return; // a máquina de quem roda tem o app; o caminho é coberto pelo teste de contrato.
        }
        let e = rodar(Path::new("/"), &["progresso", "--json"]).expect_err("sem app");
        assert_eq!(e, AUSENTE);
    }

    /// **Contra o BINÁRIO de verdade:** o que o app instalado devolve é o que os `interpretar_*`
    /// leem (grafo global e de serviço, checklist, monitor, documentos, caixa e hooks). Sem o app
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
        let od = d.join(".schematize/overdev");
        std::fs::create_dir_all(&g).expect("dir de grafos");
        std::fs::create_dir_all(&od).expect("dir do overdev");
        std::fs::write(g.join("GRAFO_GLOBAL.md"), "front -> api (login)\napi -> auth\n").unwrap();
        std::fs::write(
            g.join("api.md"),
            "- `handler` — recebe · api/src/h.rs:7\nhandler -> repo\n",
        )
        .unwrap();
        std::fs::write(
            od.join("state.json"),
            r#"{"mode":"active","max_iters":40,"objetivo":"meta","started":1}"#,
        )
        .unwrap();
        std::fs::write(od.join("CHECKLIST.md"), "- [x] a\n- [ ] b\n- [H ] c\n").unwrap();
        std::fs::write(od.join("PLAN.md"), "# plano\n").unwrap();

        let global = grafo(&d, None).expect("grafo global");
        assert!(global.nos.iter().any(|n| n.id == "front") && !global.agregado, "{global:?}");
        assert!(global.arestas.iter().any(|e| e.de == "front" && e.para == "api"));
        let api = grafo(&d, Some("api")).expect("grafo do serviço");
        let h = api.nos.iter().find(|n| n.id == "handler").expect("handler");
        assert_eq!(h.loc.as_deref(), Some("api/src/h.rs:7"));
        assert!(grafo(&d, Some("nada")).expect("serviço sem grafo").nos.is_empty());

        let c = checklist(&d).expect("checklist");
        assert_eq!(c.objetivo, "meta");
        assert_eq!(c.itens.iter().filter(|i| i.tipo == "hopen").count(), 1);
        assert_eq!(documentos(&d).expect("documentos").plano, "# plano\n");
        assert_eq!(caixa(&d).expect("caixa"), (0, 0));
        assert!(monitor(&d).expect("monitor").abertos.iter().any(|t| t == "b"));
        assert_eq!(progresso(&d).expect("progresso").abertos, 1);
        assert!(hooks_ligados(&d).is_ok(), "hooks --json responde");
        let _ = std::fs::remove_dir_all(&d);
    }
}
