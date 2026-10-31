//! A VISÃO da aba Custos: o [`Custos`] + período + data de hoje viram TEXTO pronto para a tela.
//!
//! **O quê:** a decisão do que cada card e cada tabela mostram, sem tocar em Slint. **Onde:**
//! [`crate::wire::custos`], que só copia estes campos para o modelo da janela. Pura — o teste
//! passa a data de hoje e o texto do ledger, sem relógio nem disco.

use crate::custos::*;
use chrono::NaiveDate;

/// Uma linha de tabela já formatada.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct LinhaTxt {
    pub(crate) chave: String,
    pub(crate) entrada: String,
    pub(crate) saida: String,
    pub(crate) cache: String,
    pub(crate) usd: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Tabela {
    pub(crate) linhas: Vec<LinhaTxt>,
    /// Quantas linhas existiam antes do teto de render.
    pub(crate) total: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Visao {
    /// `true` quando o período não é «Tudo» (o 1º card passa a ser «No período»).
    pub(crate) filtrado: bool,
    pub(crate) usd_total: String,
    pub(crate) tokens_total: String,
    pub(crate) tokens_detalhe: String,
    pub(crate) usd_hoje: String,
    pub(crate) tokens_hoje: String,
    pub(crate) usd_semana: String,
    pub(crate) tokens_semana: String,
    /// As tabelas por modelo/task não puderam ser refiltradas (sem ledger).
    pub(crate) sem_filtro_modelo_task: bool,
    pub(crate) modelos: Tabela,
    pub(crate) dias: Tabela,
    pub(crate) semanas: Tabela,
    pub(crate) tasks: Tabela,
    pub(crate) avisos: Vec<String>,
    pub(crate) gerado_em: String,
    pub(crate) pricing_versao: String,
}

fn txt(l: &Linha, e: Estilo) -> LinhaTxt {
    LinhaTxt {
        chave: l.chave.clone(),
        entrada: fmt_tokens(l.tokens.entrada, e),
        saida: fmt_tokens(l.tokens.saida, e),
        cache: fmt_tokens(l.tokens.cache(), e),
        usd: fmt_usd(l.usd, e),
    }
}

fn tabela(mut ls: Vec<Linha>, teto: usize, e: Estilo) -> Tabela {
    let total = ls.len();
    ls.truncate(teto);
    Tabela { linhas: ls.iter().map(|l| txt(l, e)).collect(), total }
}

/// **O quê:** monta a visão. `ledger` = texto do `ledger.jsonl` (se existir).
pub(crate) fn montar(
    c: &Custos,
    periodo: Periodo,
    hoje: NaiveDate,
    ledger: Option<&str>,
    e: Estilo,
) -> Visao {
    let desde = periodo.desde(hoje);
    let filtrado = desde.is_some();

    let mut dias = filtra_dias(&c.por_dia, desde);
    let mut semanas = filtra_semanas(&c.por_semana, desde);
    ordena_recente(&mut dias);
    ordena_recente(&mut semanas);

    // Modelo/task: do ledger quando há corte; senão, o agregado do projeto inteiro.
    let (mut modelos, mut tasks, sem_filtro) = match (filtrado, ledger) {
        (true, Some(l)) => {
            (agrega_ledger(l, "modelo", desde), agrega_ledger(l, "task", desde), false)
        }
        (true, None) => (c.por_modelo.clone(), c.por_task.clone(), true),
        (false, _) => (c.por_modelo.clone(), c.por_task.clone(), false),
    };
    ordena_por_custo(&mut modelos);
    ordena_por_custo(&mut tasks);

    // Cards: sem corte, o total oficial do JSON; com corte, a soma dos dias do período.
    let (tok, usd) = if filtrado {
        (soma_tokens(&dias), soma_usd(&dias))
    } else {
        (c.total_tokens, c.total_usd)
    };
    let (dia_h, sem_h) = hoje_e_semana(c, hoje);
    // Dia/semana sem registro valem zero: não houve consumo (diferente de «sem preço»).
    let zero = || Linha { chave: String::new(), tokens: Tokens::default(), usd: Some(0.0) };
    let dia_h = dia_h.unwrap_or_else(zero);
    let sem_h = sem_h.unwrap_or_else(zero);

    Visao {
        filtrado,
        usd_total: fmt_usd(usd, e),
        tokens_total: fmt_tokens(tok.total, e),
        tokens_detalhe: format!(
            "{} · {} · {}",
            fmt_tokens(tok.entrada, e),
            fmt_tokens(tok.saida, e),
            fmt_tokens(tok.cache(), e)
        ),
        usd_hoje: fmt_usd(dia_h.usd, e),
        tokens_hoje: fmt_tokens(dia_h.tokens.total, e),
        usd_semana: fmt_usd(sem_h.usd, e),
        tokens_semana: fmt_tokens(sem_h.tokens.total, e),
        sem_filtro_modelo_task: sem_filtro,
        modelos: tabela(modelos, MAX_LINHAS, e),
        dias: tabela(dias, MAX_LINHAS_DIA, e),
        semanas: tabela(semanas, MAX_LINHAS_SEMANA, e),
        tasks: tabela(tasks, MAX_LINHAS, e),
        avisos: c.avisos.iter().take(5).cloned().collect(),
        gerado_em: c.gerado_em.clone(),
        pricing_versao: c.pricing_versao.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("fixtures/custos_ok.json");
    const LEDGER: &str = include_str!("fixtures/ledger.jsonl");

    fn hoje() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    #[test]
    fn sem_corte_usa_o_total_oficial_e_ordena() {
        let c = parse(OK).unwrap();
        let v = montar(&c, Periodo::Tudo, hoje(), None, Estilo::Virgula);
        assert!(!v.filtrado && !v.sem_filtro_modelo_task);
        assert_eq!(v.usd_total, "US$ 1.234,50");
        assert_eq!(v.tokens_total, "2,5B");
        assert_eq!(v.usd_hoje, "< US$ 0,01");
        assert_eq!(v.usd_semana, "US$ 1,75");
        assert_eq!(v.dias.linhas[0].chave, "2026-10-31", "mais recente primeiro");
        assert_eq!(v.modelos.linhas[0].chave, "claude-sonnet-5-5");
        assert_eq!(v.modelos.linhas[1].usd, "—");
        assert_eq!(v.avisos.len(), 1);
    }

    #[test]
    fn com_corte_cards_vem_dos_dias_e_modelo_task_do_ledger() {
        let c = parse(OK).unwrap();
        let v = montar(&c, Periodo::Dias7, hoje(), Some(LEDGER), Estilo::Ponto);
        assert!(v.filtrado && !v.sem_filtro_modelo_task);
        assert_eq!(v.dias.total, 3);
        assert_eq!(v.usd_total, "US$ 1.75");
        assert_eq!(v.modelos.total, 1, "só m-a está dentro dos 7 dias");
        assert_eq!(v.tasks.linhas[0].chave, "T-1");
    }

    #[test]
    fn com_corte_sem_ledger_avisa_que_modelo_task_nao_filtram() {
        let c = parse(OK).unwrap();
        let v = montar(&c, Periodo::MesAtual, hoje(), None, Estilo::Ponto);
        assert!(v.sem_filtro_modelo_task);
        assert_eq!(v.modelos.total, 2);
    }

    #[test]
    fn hoje_sem_registro_e_zero_e_nao_traco() {
        let c = parse(OK).unwrap();
        let v = montar(
            &c,
            Periodo::Tudo,
            NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
            None,
            Estilo::Ponto,
        );
        assert_eq!(v.usd_hoje, "US$ 0.00");
        assert_eq!(v.tokens_hoje, "0");
    }

    #[test]
    fn teto_de_linhas_preserva_o_total() {
        let mut c = Custos::default();
        for i in 0..(MAX_LINHAS_DIA + 10) {
            c.por_dia.push(Linha {
                chave: format!("2026-01-{:02}-{i}", (i % 28) + 1),
                tokens: Tokens::default(),
                usd: Some(1.0),
            });
        }
        let v = montar(&c, Periodo::Tudo, hoje(), None, Estilo::Ponto);
        assert_eq!(v.dias.linhas.len(), MAX_LINHAS_DIA);
        assert_eq!(v.dias.total, MAX_LINHAS_DIA + 10);
    }
}
