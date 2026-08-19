//! Formatação de valores pra UI (data, tamanho, milhar, upstream, tokens).
//! Funções puras — sem I/O e sem tocar em propriedade. Testáveis em unidade.

use crate::prelude::*;

// ---------------------------------------------------------------------------
// Formatação p/ o histórico do DB do overdev. Sem crate de data: converte o
// epoch (UTC) via o algoritmo civil de Howard Hinnant.
// ---------------------------------------------------------------------------
pub(crate) fn fmt_ts(ts: i64) -> String {
    if ts <= 0 {
        return "—".into();
    }
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (h, mi) = (secs / 3600, (secs % 3600) / 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Tamanho legível (B / KB / MB).
pub(crate) fn fmt_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Linha de upstream (branch → remote · ↑ahead ↓behind); vazia se sem tracking.
pub(crate) fn fmt_upstream(up: Option<githist::Upstream>) -> String {
    match up {
        Some(u) => {
            let remote = u.remote.unwrap_or_else(|| "—".into());
            format!("{} → {} · ↑{} ↓{}", u.branch, remote, u.ahead, u.behind)
        }
        None => String::new(),
    }
}

/// Agrupa milhares com `.` (separador pt-BR): 1234567 → "1.234.567". PURO.
pub(crate) fn sep_thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push('.');
        }
        out.push(*b as char);
    }
    out
}

/// Converte um epoch (segundos) pra `HH:MM:SS` na hora LOCAL via `chrono::Local`.
/// Fallback improvável (timestamp fora de faixa) → string vazia.
pub(crate) fn fmt_ts_local(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Teto de linhas do log de conclusões que a UI segura. O painel é um repeater
/// NÃO virtualizado dentro do ScrollView do overdev: um run longo acumula
/// centenas de conclusões e o monitor as reconstruía TODAS a cada 3 s. Mantemos
/// as mais RECENTES (é o que interessa acompanhar); o histórico completo continua
/// no `.schematize/overdev/completions.json`.
pub(crate) const OD_COMPLETIONS_MAX: usize = 120;

/// Materializa as conclusões (`overdev::completions`) em linhas prontas pra UI:
/// `HH:MM:SS  <texto>` (hora local). Mantém a ordem do lib (ts asc → recentes
/// embaixo) e corta nas últimas [`OD_COMPLETIONS_MAX`].
pub(crate) fn fmt_completions(cs: Vec<overdev::Completion>) -> Vec<String> {
    let cs = if cs.len() > OD_COMPLETIONS_MAX {
        cs[cs.len() - OD_COMPLETIONS_MAX..].to_vec()
    } else {
        cs
    };
    cs.into_iter()
        .map(|c| {
            let hhmmss = fmt_ts_local(c.ts);
            if hhmmss.is_empty() {
                c.text
            } else {
                format!("{hhmmss}  {}", c.text)
            }
        })
        .collect()
}

/// Monta a linha de tokens do painel do monitor a partir de `usage::Usage`.
/// "Tokens: <total> (in <in> / out <out> · cache-read <cr>) · Modelo: <main>".
pub(crate) fn fmt_usage(u: &usage::Usage) -> String {
    let model = u.main_model().unwrap_or("—");
    format!(
        "{}: {} (in {} / out {} · cache-read {}) · {}: {}",
        tor("gui.od_tokens", "Tokens"),
        sep_thousands(u.total),
        sep_thousands(u.input),
        sep_thousands(u.output),
        sep_thousands(u.cache_read),
        tor("gui.od_model", "Modelo"),
        model,
    )
}
