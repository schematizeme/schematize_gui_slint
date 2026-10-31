//! Textos da aba Custos em pt/en/es.
//!
//! **Por que local:** o catálogo i18n mora no CLI (`schematize::i18n`, 11 locales) e esta aba
//! nasceu antes das chaves `gui.custos_*` existirem lá. Cada texto tenta PRIMEIRO o catálogo do
//! CLI (quando as chaves entrarem, passam a valer sozinhas) e cai nesta tabela. Idioma sem
//! tradução aqui usa o inglês.

use crate::prelude::*;

/// (chave, pt, en, es)
const TXT: &[(&str, &str, &str, &str)] = &[
    ("title", "Custos", "Costs", "Costos"),
    ("sync", "Sincronizar", "Sync", "Sincronizar"),
    ("syncing", "Sincronizando…", "Syncing…", "Sincronizando…"),
    (
        "disclaimer",
        "Estimativa API-equivalente; assinatura ou plano pode cobrar diferente. Confira o /usage.",
        "API-equivalent estimate; your subscription or plan may bill differently. Check /usage.",
        "Estimación equivalente a la API; la suscripción o el plan puede cobrar distinto. Revisa /usage.",
    ),
    ("card_total", "Total", "Total", "Total"),
    ("card_tokens", "Tokens", "Tokens", "Tokens"),
    ("card_today", "Hoje", "Today", "Hoy"),
    ("card_week", "Semana corrente", "This week", "Semana actual"),
    ("card_period", "No período", "In period", "En el período"),
    ("per_all", "Tudo", "All", "Todo"),
    ("per_7", "7 dias", "7 days", "7 días"),
    ("per_30", "30 dias", "30 days", "30 días"),
    ("per_month", "Mês atual", "This month", "Mes actual"),
    ("t_model", "Por modelo", "By model", "Por modelo"),
    ("t_day", "Por dia", "By day", "Por día"),
    ("t_week", "Por semana ISO", "By ISO week", "Por semana ISO"),
    ("t_task", "Por task", "By task", "Por tarea"),
    ("c_in", "Entrada", "Input", "Entrada"),
    ("c_out", "Saída", "Output", "Salida"),
    ("c_cache", "Cache", "Cache", "Caché"),
    ("c_usd", "US$", "US$", "US$"),
    ("c_item", "Item", "Item", "Elemento"),
    ("no_rows", "Sem registros.", "No records.", "Sin registros."),
    (
        "all_time_note",
        "Totais do projeto inteiro (sem o ledger não dá para refiltrar por período).",
        "Whole-project totals (without the ledger this table cannot be filtered by period).",
        "Totales de todo el proyecto (sin el ledger no se puede filtrar por período).",
    ),
    ("truncated", "Mostrando as primeiras {n} linhas.", "Showing the first {n} rows.", "Mostrando las primeras {n} filas."),
    (
        "empty_title",
        "Ainda não há custos calculados para este projeto",
        "No costs have been calculated for this project yet",
        "Aún no hay costos calculados para este proyecto",
    ),
    (
        "empty_body",
        "Sincronize para ler os transcripts do Claude Code e montar o relatório.",
        "Sync to read the Claude Code transcripts and build the report.",
        "Sincroniza para leer los transcripts de Claude Code y armar el informe.",
    ),
    (
        "no_archive",
        "Este projeto ainda não tem um archive. Crie o archive para guardar o histórico de custos.",
        "This project has no archive yet. Create the archive to keep the cost history.",
        "Este proyecto aún no tiene archive. Crea el archive para guardar el historial de costos.",
    ),
    ("no_project", "Escolha um projeto no seletor acima.", "Pick a project in the selector above.", "Elige un proyecto en el selector de arriba."),
    (
        "unreadable",
        "Não consegui ler o relatório de custos agora. Sincronizar costuma regerá-lo.",
        "I could not read the cost report right now. Syncing usually regenerates it.",
        "No pude leer el informe de costos ahora. Sincronizar suele regenerarlo.",
    ),
    ("detail", "Detalhe", "Detail", "Detalle"),
    ("sync_ok", "Sincronizado.", "Synced.", "Sincronizado."),
    (
        "sync_fail",
        "Não consegui sincronizar agora",
        "I could not sync right now",
        "No pude sincronizar ahora",
    ),
    ("updated", "Atualizado em", "Updated", "Actualizado"),
    ("pricing", "tabela de preços", "pricing table", "tabla de precios"),
    ("warnings", "avisos", "warnings", "avisos"),
    ("home_desc", "Quanto cada projeto gasta em tokens e US$.", "What each project spends in tokens and US$.", "Cuánto gasta cada proyecto en tokens y US$."),
];

/// Resolve uma chave local pelo código de idioma. Pura: o teste passa o código.
pub(crate) fn local(chave: &str, codigo: &str) -> String {
    let c = codigo.to_ascii_lowercase();
    TXT.iter()
        .find(|(k, ..)| *k == chave)
        .map(|(_, pt, en, es)| {
            if c.starts_with("pt") {
                *pt
            } else if c.starts_with("es") {
                *es
            } else {
                *en
            }
        })
        .unwrap_or(chave)
        .to_string()
}

/// O texto da aba na língua atual do app (catálogo do CLI primeiro, tabela local depois).
pub(crate) fn tc(chave: &str) -> String {
    let k = format!("gui.custos_{chave}");
    let v = t(&k);
    if v != k {
        v
    } else {
        local(chave, &i18n::current_code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toda_chave_tem_pt_en_es_nao_vazios() {
        for (k, pt, en, es) in TXT {
            assert!(!pt.is_empty() && !en.is_empty() && !es.is_empty(), "{k}");
        }
    }

    #[test]
    fn idioma_resolve_e_cai_no_ingles() {
        assert_eq!(local("sync", "pt-BR"), "Sincronizar");
        assert_eq!(local("card_today", "es"), "Hoy");
        assert_eq!(local("card_today", "de"), "Today");
        assert_eq!(local("nao-existe", "pt"), "nao-existe");
    }

    #[test]
    fn chaves_unicas() {
        let mut ks: Vec<_> = TXT.iter().map(|t| t.0).collect();
        ks.sort_unstable();
        let n = ks.len();
        ks.dedup();
        assert_eq!(n, ks.len());
    }
}
