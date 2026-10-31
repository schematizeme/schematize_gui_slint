//! Modelo de dados da aba Custos — DESACOPLADO da UI: structs, parse tolerante e formatação.
//!
//! **O quê:** lê o `custos.json` que o CLI (`schematize cost report`) grava no archive do projeto
//! (contrato `custos-v1.md`) e o traduz num tipo próprio desta GUI. Também aplica o filtro de
//! período e formata US$ e tokens. **Onde:** [`crate::wire::custos`] (fiação) — nada aqui toca
//! em Slint.
//!
//! **A GUI NÃO é dona do parser de transcripts.** O que ela lê é o agregado já pronto. O único
//! outro arquivo é o `ledger.jsonl` (1 linha por resposta), e só para refiltrar por período as
//! tabelas por modelo/task, que o agregado não consegue refiltrar sozinho.
//!
//! **Tolerância é o contrato:** JSON ausente, corrompido, de outra versão ou com campo novo NUNCA
//! derruba a janela — vira um [`Estado`] com texto acionável. Linha de tabela ilegível é pulada.
//! Cada função é PURA (recebe texto/data) para ser afirmada sem o CLI instalado.

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Tetos de linhas por tabela: o repeater do Slint não é virtualizado (custo de render constante).
pub(crate) const MAX_LINHAS_DIA: usize = 60;
pub(crate) const MAX_LINHAS_SEMANA: usize = 52;
pub(crate) const MAX_LINHAS: usize = 100;

/// Os quatro contadores de tokens de uma linha (ou do total).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Tokens {
    pub(crate) entrada: u64,
    pub(crate) saida: u64,
    pub(crate) cache_w: u64,
    pub(crate) cache_r: u64,
    pub(crate) total: u64,
}

impl Tokens {
    /// Cache = escrita + leitura (a tela mostra uma coluna só).
    pub(crate) fn cache(&self) -> u64 {
        self.cache_w.saturating_add(self.cache_r)
    }

    fn somar(&mut self, o: &Tokens) {
        self.entrada = self.entrada.saturating_add(o.entrada);
        self.saida = self.saida.saturating_add(o.saida);
        self.cache_w = self.cache_w.saturating_add(o.cache_w);
        self.cache_r = self.cache_r.saturating_add(o.cache_r);
        self.total = self.total.saturating_add(o.total);
    }
}

/// Uma linha de qualquer tabela (modelo, dia, semana ISO, task).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Linha {
    pub(crate) chave: String,
    pub(crate) tokens: Tokens,
    /// `None` = modelo sem preço conhecido. NUNCA vira zero: custo desconhecido não é custo nulo.
    pub(crate) usd: Option<f64>,
}

/// O `custos.json` já lido.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Custos {
    pub(crate) gerado_em: String,
    pub(crate) pricing_versao: String,
    pub(crate) avisos: Vec<String>,
    pub(crate) total_tokens: Tokens,
    pub(crate) total_usd: Option<f64>,
    pub(crate) por_modelo: Vec<Linha>,
    pub(crate) por_dia: Vec<Linha>,
    pub(crate) por_semana: Vec<Linha>,
    pub(crate) por_task: Vec<Linha>,
}

/// O que a tela pode estar mostrando. Cada variante tem um texto próprio — nunca tela calada.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Estado {
    /// Sem projeto, ou o projeto não tem `<projeto>_archive/`.
    SemArchive,
    /// Archive existe, mas o `custos.json` ainda não (nunca sincronizou).
    SemArquivo,
    /// O arquivo existe e não deu para ler/parsear; o motivo vai junto.
    Ilegivel(String),
    Pronto(Box<Custos>),
}

// ---------------------------------------------------------------------------
// Caminhos
// ---------------------------------------------------------------------------

/// **O quê:** o `<projeto>_archive/` de `root`, se existir.
///
/// Primeiro `<root>/<nome-da-pasta>_archive` (a convenção); senão o ÚNICO filho `*_archive`
/// (workspace cujo archive leva o prefixo comum dos microserviços). Mais de um = ambíguo = `None`.
pub(crate) fn archive_de(root: &Path) -> Option<PathBuf> {
    let nome = root.file_name()?.to_string_lossy().into_owned();
    let canon = root.join(format!("{nome}_archive"));
    if canon.is_dir() {
        return Some(canon);
    }
    let mut achados: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_archive"))
        })
        .collect();
    if achados.len() == 1 {
        achados.pop()
    } else {
        None
    }
}

pub(crate) fn dir_custos(root: &Path) -> Option<PathBuf> {
    archive_de(root).map(|a| a.join("custos"))
}

/// **O quê:** lê o estado da aba para `root`. **Efeitos:** só leitura de disco.
pub(crate) fn ler(root: Option<&Path>) -> Estado {
    let Some(dir) = root.and_then(dir_custos) else { return Estado::SemArchive };
    let f = dir.join("custos.json");
    match std::fs::read_to_string(&f) {
        Ok(txt) => match parse(&txt) {
            Ok(c) => Estado::Pronto(Box::new(c)),
            Err(e) => Estado::Ilegivel(e),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Estado::SemArquivo,
        Err(e) => Estado::Ilegivel(format!("{}: {e}", f.display())),
    }
}

// ---------------------------------------------------------------------------
// Parse tolerante
// ---------------------------------------------------------------------------

fn n_u64(v: &Value, chaves: &[&str]) -> u64 {
    chaves
        .iter()
        .find_map(|k| v.get(*k))
        .and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)))
        .unwrap_or(0)
}

fn tem(v: &Value, chaves: &[&str]) -> bool {
    chaves.iter().any(|k| v.get(*k).is_some_and(|x| x.is_number()))
}

/// Tokens a partir de um objeto `{in,out,cache_w,cache_r,total}`; `total` ausente é a soma.
fn tokens_de(v: &Value) -> Tokens {
    let mut t = Tokens {
        entrada: n_u64(v, &["in"]),
        saida: n_u64(v, &["out"]),
        cache_w: n_u64(v, &["cache_w"]),
        cache_r: n_u64(v, &["cache_r"]),
        total: 0,
    };
    t.total = if tem(v, &["total"]) {
        n_u64(v, &["total"])
    } else {
        t.entrada.saturating_add(t.saida).saturating_add(t.cache_w).saturating_add(t.cache_r)
    };
    t
}

fn usd_de(v: &Value) -> Option<f64> {
    v.get("usd").and_then(Value::as_f64).filter(|f| f.is_finite())
}

/// A chave de cada tabela no `custos.json` v1 (`custos-v1.md`): uma por `por_*`.
const CHAVES_LINHA: [&str; 4] = ["modelo", "dia", "semana", "task"];

/// Uma linha de tabela. `None` = sem chave legível (a linha é pulada, não derruba as outras).
fn linha_de(v: &Value) -> Option<Linha> {
    if !v.is_object() {
        return None;
    }
    let base = CHAVES_LINHA.iter().find_map(|k| v.get(*k).and_then(Value::as_str))?.to_string();
    // Task carrega um nome humano ao lado do id.
    let nome = v.get("nome").and_then(Value::as_str).filter(|n| !n.is_empty() && *n != base);
    let chave = match nome {
        Some(n) => format!("{base} · {n}"),
        None => base,
    };
    // Tokens aninhados (`tokens:{…}`) ou planos na própria linha.
    let tokens = tokens_de(v.get("tokens").filter(|t| t.is_object()).unwrap_or(v));
    Some(Linha { chave, tokens, usd: usd_de(v) })
}

fn linhas_de(v: &Value, campo: &str) -> Vec<Linha> {
    v.get(campo)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(linha_de).collect())
        .unwrap_or_default()
}

/// **O quê:** `custos.json` (texto) → [`Custos`]. Campo desconhecido é ignorado; seção ausente é
/// vazia; só um documento que não é objeto JSON é erro.
pub(crate) fn parse(texto: &str) -> Result<Custos, String> {
    let v: Value = serde_json::from_str(texto).map_err(|e| format!("JSON inválido: {e}"))?;
    if !v.is_object() {
        return Err("o documento não é um objeto JSON".into());
    }
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let total = v.get("total");
    let total_tokens = total
        .map(|t| tokens_de(t.get("tokens").filter(|x| x.is_object()).unwrap_or(t)))
        .unwrap_or_default();
    let mut c = Custos {
        gerado_em: s("gerado_em"),
        pricing_versao: s("pricing_versao"),
        avisos: v
            .get("avisos")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        total_tokens,
        total_usd: total.and_then(usd_de),
        por_modelo: linhas_de(&v, "por_modelo"),
        por_dia: linhas_de(&v, "por_dia"),
        por_semana: linhas_de(&v, "por_semana"),
        por_task: linhas_de(&v, "por_task"),
    };
    // Total ausente: reconstrói das linhas por modelo (mesmo conjunto de respostas).
    if total.is_none() {
        c.total_tokens = soma_tokens(&c.por_modelo);
        c.total_usd = soma_usd(&c.por_modelo);
    }
    Ok(c)
}

pub(crate) fn soma_tokens(ls: &[Linha]) -> Tokens {
    ls.iter().fold(Tokens::default(), |mut a, l| {
        a.somar(&l.tokens);
        a
    })
}

/// Soma dos custos conhecidos; `None` só se NENHUMA linha tem preço.
pub(crate) fn soma_usd(ls: &[Linha]) -> Option<f64> {
    ls.iter().filter_map(|l| l.usd).fold(None, |a, x| Some(a.unwrap_or(0.0) + x))
}

// ---------------------------------------------------------------------------
// Período
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Periodo {
    Tudo,
    Dias7,
    Dias30,
    MesAtual,
}

impl Periodo {
    pub(crate) fn de_indice(i: i32) -> Periodo {
        match i {
            1 => Periodo::Dias7,
            2 => Periodo::Dias30,
            3 => Periodo::MesAtual,
            _ => Periodo::Tudo,
        }
    }

    /// Primeiro dia (inclusive) do período; `None` = sem corte.
    pub(crate) fn desde(self, hoje: NaiveDate) -> Option<NaiveDate> {
        match self {
            Periodo::Tudo => None,
            Periodo::Dias7 => Some(hoje - Duration::days(6)),
            Periodo::Dias30 => Some(hoje - Duration::days(29)),
            Periodo::MesAtual => hoje.with_day(1),
        }
    }
}

fn data_de(chave: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(chave.get(..10)?, "%Y-%m-%d").ok()
}

/// `2026-W44` / `2026W44` / `2026-44` → (ano, semana).
pub(crate) fn semana_de(chave: &str) -> Option<(i32, u32)> {
    let c = chave.trim();
    let ano: i32 = c.get(..4)?.parse().ok()?;
    let resto = c.get(4..)?.trim_start_matches(['-', ' ']).trim_start_matches(['W', 'w']);
    let sem: u32 = resto.parse().ok()?;
    (1..=53).contains(&sem).then_some((ano, sem))
}

pub(crate) fn semana_iso_de(d: NaiveDate) -> (i32, u32) {
    let w = d.iso_week();
    (w.year(), w.week())
}

/// Dias dentro do período (linhas sem data legível saem: não dá para saber se cabem).
pub(crate) fn filtra_dias(ls: &[Linha], desde: Option<NaiveDate>) -> Vec<Linha> {
    match desde {
        None => ls.to_vec(),
        Some(d) => {
            ls.iter().filter(|l| data_de(&l.chave).is_some_and(|x| x >= d)).cloned().collect()
        }
    }
}

/// Semanas cujo fim não antecede o início do período.
pub(crate) fn filtra_semanas(ls: &[Linha], desde: Option<NaiveDate>) -> Vec<Linha> {
    match desde {
        None => ls.to_vec(),
        Some(d) => {
            let corte = semana_iso_de(d);
            ls.iter().filter(|l| semana_de(&l.chave).is_some_and(|w| w >= corte)).cloned().collect()
        }
    }
}

/// Soma do dia `hoje` e da semana ISO corrente.
pub(crate) fn hoje_e_semana(c: &Custos, hoje: NaiveDate) -> (Option<Linha>, Option<Linha>) {
    let dia = c.por_dia.iter().find(|l| data_de(&l.chave) == Some(hoje)).cloned();
    let alvo = semana_iso_de(hoje);
    let sem =
        c.por_semana.iter().find(|l| semana_de(&l.chave) == Some(alvo)).cloned().or_else(|| {
            // Sem a tabela semanal: soma os dias da semana.
            let dias: Vec<Linha> = c
                .por_dia
                .iter()
                .filter(|l| data_de(&l.chave).map(semana_iso_de) == Some(alvo))
                .cloned()
                .collect();
            (!dias.is_empty()).then(|| Linha {
                chave: String::new(),
                tokens: soma_tokens(&dias),
                usd: soma_usd(&dias),
            })
        });
    (dia, sem)
}

// ---------------------------------------------------------------------------
// Ledger (só para refiltrar modelo/task por período)
// ---------------------------------------------------------------------------

/// Agrega o `ledger.jsonl` no período por `campo` ("modelo" | "task"). Linha ilegível é pulada.
pub(crate) fn agrega_ledger(texto: &str, campo: &str, desde: Option<NaiveDate>) -> Vec<Linha> {
    let mut mapa: Vec<(String, Tokens, Option<f64>)> = Vec::new();
    for l in texto.lines() {
        let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
        let Some(dia) = v.get("dia").and_then(Value::as_str).and_then(data_de) else { continue };
        if desde.is_some_and(|d| dia < d) {
            continue;
        }
        let chave = v.get(campo).and_then(Value::as_str).unwrap_or("sem-task").to_string();
        let t = tokens_de(&v);
        let usd = usd_de(&v);
        match mapa.iter_mut().find(|(k, _, _)| *k == chave) {
            Some((_, a, u)) => {
                a.somar(&t);
                *u = match (*u, usd) {
                    (None, x) => x,
                    (x, None) => x,
                    (Some(a), Some(b)) => Some(a + b),
                };
            }
            None => mapa.push((chave, t, usd)),
        }
    }
    mapa.into_iter().map(|(chave, tokens, usd)| Linha { chave, tokens, usd }).collect()
}

/// Mais caro primeiro (sem preço no fim), empate pela chave.
pub(crate) fn ordena_por_custo(ls: &mut [Linha]) {
    ls.sort_by(|a, b| {
        b.usd
            .unwrap_or(-1.0)
            .partial_cmp(&a.usd.unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chave.cmp(&b.chave))
    });
}

/// Mais recente primeiro (as chaves de dia/semana ordenam como texto).
pub(crate) fn ordena_recente(ls: &mut [Linha]) {
    ls.sort_by(|a, b| b.chave.cmp(&a.chave));
}

// ---------------------------------------------------------------------------
// Formatação
// ---------------------------------------------------------------------------

/// Estilo numérico: vírgula decimal (pt/es) ou ponto (demais).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Estilo {
    Virgula,
    Ponto,
}

impl Estilo {
    pub(crate) fn do_idioma(codigo: &str) -> Estilo {
        let c = codigo.to_ascii_lowercase();
        if c.starts_with("pt") || c.starts_with("es") {
            Estilo::Virgula
        } else {
            Estilo::Ponto
        }
    }

    fn dec(self) -> char {
        if self == Estilo::Virgula {
            ','
        } else {
            '.'
        }
    }

    fn mil(self) -> char {
        if self == Estilo::Virgula {
            '.'
        } else {
            ','
        }
    }
}

fn agrupa_milhar(inteiro: u64, e: Estilo) -> String {
    let s = inteiro.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(e.mil());
        }
        out.push(ch);
    }
    out
}

/// **O quê:** `US$ 1.234,56`. `None` (sem preço) é `—`; positivo abaixo de 1 centavo é
/// `< US$ 0,01` (nunca `US$ 0,00` para algo que custou).
pub(crate) fn fmt_usd(v: Option<f64>, e: Estilo) -> String {
    let Some(v) = v.filter(|x| x.is_finite()) else { return "—".into() };
    let v = v.max(0.0);
    if v > 0.0 && v < 0.005 {
        return format!("< US$ 0{}01", e.dec());
    }
    let cent = (v * 100.0).round() as u64;
    format!("US$ {}{}{:02}", agrupa_milhar(cent / 100, e), e.dec(), cent % 100)
}

/// **O quê:** tokens abreviados: `999`, `1,2k`, `3,4M`, `5,6B`.
pub(crate) fn fmt_tokens(n: u64, e: Estilo) -> String {
    const ESC: [(u64, &str); 3] = [(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "k")];
    for (i, (base, suf)) in ESC.iter().enumerate() {
        if n >= *base {
            let mut x = n as f64 / *base as f64;
            let mut suf = *suf;
            // 999.950 arredondaria para "1000,0k": promove para a unidade de cima.
            if (x * 10.0).round() >= 10_000.0 && i > 0 {
                x = n as f64 / ESC[i - 1].0 as f64;
                suf = ESC[i - 1].1;
            }
            return format!("{x:.1}{suf}").replace('.', &e.dec().to_string());
        }
    }
    n.to_string()
}

#[cfg(test)]
#[path = "custos_tests.rs"]
mod tests;
