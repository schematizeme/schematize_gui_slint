//! Testes do modelo de custos: parse tolerante, período e formatação, sobre fixtures do §4.

use super::*;

const OK: &str = include_str!("fixtures/custos_ok.json");
const LEDGER: &str = include_str!("fixtures/ledger.jsonl");

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn parse_le_totais_e_tabelas_e_ignora_campo_novo() {
    let c = parse(OK).unwrap();
    assert_eq!(c.pricing_versao, "2026-10-01");
    assert_eq!(c.total_usd, Some(1234.5));
    assert_eq!(c.total_tokens.total, 2_503_451_200);
    assert_eq!(c.avisos.len(), 1);
    assert_eq!(c.por_modelo.len(), 2);
    assert_eq!(c.por_modelo[1].usd, None, "sem preço continua null, nunca zero");
    // A linha sem chave é pulada; as outras 4 ficam.
    assert_eq!(c.por_dia.len(), 4);
    assert_eq!(c.por_task[0].chave, "T-1 · Aba Custos", "o nome da task vai ao lado do id");
}

#[test]
fn total_ausente_na_linha_e_a_soma_dos_quatro() {
    let c = parse(OK).unwrap();
    assert_eq!(c.por_dia[0].tokens.total, 300);
    assert_eq!(c.por_dia[1].tokens.cache(), 10);
}

#[test]
fn json_quebrado_ou_nao_objeto_vira_erro_e_nao_panico() {
    assert!(parse("{ nao eh json").is_err());
    assert!(parse("[1,2]").is_err());
    assert!(parse("").is_err());
    // Objeto vazio é válido: tudo zerado.
    let c = parse("{}").unwrap();
    assert_eq!(c, Custos::default());
}

#[test]
fn tipos_inesperados_nao_derrubam() {
    let c =
        parse(r#"{"total":"x","por_modelo":{"a":1},"por_dia":[1,"b",null,{"dia":5}],"avisos":7}"#)
            .unwrap();
    assert!(c.por_dia.is_empty() && c.por_modelo.is_empty() && c.avisos.is_empty());
}

#[test]
fn total_ausente_reconstroi_das_linhas_por_modelo() {
    let c = parse(r#"{"por_modelo":[{"modelo":"a","in":1,"out":2,"usd":1.5},{"modelo":"b","in":3,"usd":null}]}"#)
        .unwrap();
    assert_eq!(c.total_tokens.total, 6);
    assert_eq!(c.total_usd, Some(1.5));
}

#[test]
fn ler_distingue_sem_archive_sem_arquivo_ilegivel_e_pronto() {
    assert_eq!(ler(None), Estado::SemArchive);
    let tmp = std::env::temp_dir().join(format!("cust-{}", std::process::id()));
    let proj = tmp.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    assert_eq!(ler(Some(&proj)), Estado::SemArchive);
    let cdir = proj.join("proj_archive").join("custos");
    std::fs::create_dir_all(&cdir).unwrap();
    assert_eq!(ler(Some(&proj)), Estado::SemArquivo);
    std::fs::write(cdir.join("custos.json"), "{ corrompido").unwrap();
    assert!(matches!(ler(Some(&proj)), Estado::Ilegivel(_)));
    std::fs::write(cdir.join("custos.json"), OK).unwrap();
    assert!(matches!(ler(Some(&proj)), Estado::Pronto(_)));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn archive_com_prefixo_comum_so_vale_se_for_unico() {
    let tmp = std::env::temp_dir().join(format!("cust-ws-{}", std::process::id()));
    std::fs::create_dir_all(tmp.join("ws/app_archive")).unwrap();
    assert_eq!(archive_de(&tmp.join("ws")), Some(tmp.join("ws/app_archive")));
    std::fs::create_dir_all(tmp.join("ws/outro_archive")).unwrap();
    assert_eq!(archive_de(&tmp.join("ws")), None, "dois candidatos = ambíguo");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn periodo_calcula_o_corte() {
    let hoje = d("2026-10-31");
    assert_eq!(Periodo::Tudo.desde(hoje), None);
    assert_eq!(Periodo::Dias7.desde(hoje), Some(d("2026-10-25")));
    assert_eq!(Periodo::Dias30.desde(hoje), Some(d("2026-10-02")));
    assert_eq!(Periodo::MesAtual.desde(hoje), Some(d("2026-10-01")));
    assert_eq!(Periodo::de_indice(99), Periodo::Tudo);
}

#[test]
fn filtra_dias_e_semanas_pelo_corte() {
    let c = parse(OK).unwrap();
    let desde = Some(d("2026-10-25"));
    assert_eq!(filtra_dias(&c.por_dia, desde).len(), 3);
    let sem = filtra_semanas(&c.por_semana, desde);
    assert_eq!(sem.len(), 1);
    assert_eq!(sem[0].chave, "2026-W44");
    assert_eq!(filtra_dias(&c.por_dia, None).len(), 4);
}

#[test]
fn hoje_e_semana_corrente() {
    let c = parse(OK).unwrap();
    let (dia, sem) = hoje_e_semana(&c, d("2026-10-31"));
    assert_eq!(dia.unwrap().usd, Some(0.004));
    assert_eq!(sem.unwrap().usd, Some(1.754));
    // Dia sem registro: None (a tela mostra zero, não inventa).
    let (dia, _) = hoje_e_semana(&c, d("2026-10-28"));
    assert!(dia.is_none());
    // Sem tabela semanal: soma os dias da semana ISO.
    let mut c2 = c.clone();
    c2.por_semana.clear();
    let (_, sem) = hoje_e_semana(&c2, d("2026-10-31"));
    assert_eq!(sem.unwrap().tokens.total, 300 + 40 + 10);
}

#[test]
fn semana_de_aceita_formas_comuns() {
    assert_eq!(semana_de("2026-W44"), Some((2026, 44)));
    assert_eq!(semana_de("2026W04"), Some((2026, 4)));
    assert_eq!(semana_de("lixo"), None);
    assert_eq!(semana_de("2026-W99"), None);
}

#[test]
fn ledger_refiltra_por_periodo_e_pula_linha_corrompida() {
    let tudo = agrega_ledger(LEDGER, "modelo", None);
    assert_eq!(tudo.len(), 2);
    let mes = agrega_ledger(LEDGER, "task", Some(d("2026-10-01")));
    assert_eq!(mes.len(), 1);
    assert_eq!(mes[0].chave, "T-1");
    assert_eq!(mes[0].usd, Some(1.5));
    assert_eq!(mes[0].tokens.total, 40);
    let sem_preco =
        agrega_ledger(LEDGER, "modelo", None).into_iter().find(|l| l.chave == "m-b").unwrap();
    assert_eq!(sem_preco.usd, None);
}

#[test]
fn ordenacao() {
    let mut v = parse(OK).unwrap().por_modelo;
    v.reverse();
    ordena_por_custo(&mut v);
    assert_eq!(v[0].chave, "claude-sonnet-5-5");
    assert_eq!(v[1].usd, None, "sem preço no fim");
    let mut dias = parse(OK).unwrap().por_dia;
    ordena_recente(&mut dias);
    assert_eq!(dias[0].chave, "2026-10-31");
}

#[test]
fn usd_formata_por_idioma() {
    assert_eq!(fmt_usd(Some(1234.5), Estilo::Virgula), "US$ 1.234,50");
    assert_eq!(fmt_usd(Some(1234.5), Estilo::Ponto), "US$ 1,234.50");
    assert_eq!(fmt_usd(Some(0.0), Estilo::Virgula), "US$ 0,00");
    assert_eq!(fmt_usd(Some(0.004), Estilo::Virgula), "< US$ 0,01");
    assert_eq!(fmt_usd(Some(1_000_000.0), Estilo::Ponto), "US$ 1,000,000.00");
    assert_eq!(fmt_usd(None, Estilo::Ponto), "—");
    assert_eq!(fmt_usd(Some(f64::NAN), Estilo::Ponto), "—");
    assert_eq!(fmt_usd(Some(-3.0), Estilo::Ponto), "US$ 0.00");
}

#[test]
fn tokens_abreviados() {
    let e = Estilo::Virgula;
    assert_eq!(fmt_tokens(0, e), "0");
    assert_eq!(fmt_tokens(999, e), "999");
    assert_eq!(fmt_tokens(1_000, e), "1,0k");
    assert_eq!(fmt_tokens(1_234, e), "1,2k");
    assert_eq!(fmt_tokens(3_400_000, e), "3,4M");
    assert_eq!(fmt_tokens(2_500_000_000, Estilo::Ponto), "2.5B");
    assert_eq!(fmt_tokens(999_950, e), "1,0M", "não mostra 1000,0k");
    assert!(fmt_tokens(u64::MAX, Estilo::Ponto).ends_with('B'));
}

#[test]
fn estilo_do_idioma() {
    assert_eq!(Estilo::do_idioma("pt-BR"), Estilo::Virgula);
    assert_eq!(Estilo::do_idioma("es"), Estilo::Virgula);
    assert_eq!(Estilo::do_idioma("en"), Estilo::Ponto);
    assert_eq!(Estilo::do_idioma(""), Estilo::Ponto);
}

// ---------------------------------------------------------------------------
// Contrato com o CLI real: `custos.json` v1 gerado por `schematize cost report --json`
// (fixture anonimizada: ids de sessão/agente trocados, valores reais).
// ---------------------------------------------------------------------------

const REAL: &str = include_str!("fixtures/custos_real.json");
const LEDGER_REAL: &str = include_str!("fixtures/ledger_real.jsonl");

fn soma_tok(ls: &[Linha]) -> Tokens {
    soma_tokens(ls)
}

#[test]
fn contrato_real_parseia_todas_as_tabelas_sem_pular_linha() {
    let bruto: Value = serde_json::from_str(REAL).unwrap();
    let n = |k: &str| bruto[k].as_array().unwrap().len();
    let c = parse(REAL).unwrap();
    assert_eq!(c.por_modelo.len(), n("por_modelo"));
    assert_eq!(c.por_dia.len(), n("por_dia"));
    assert_eq!(c.por_semana.len(), n("por_semana"));
    assert_eq!(c.por_task.len(), n("por_task"));
    assert_eq!(c.pricing_versao, bruto["pricing_versao"].as_str().unwrap());
    assert_eq!(c.avisos.len(), n("avisos"));
    assert!(c.por_semana[0].chave.starts_with("2026-W"), "{:?}", c.por_semana[0]);
}

#[test]
fn contrato_real_totais_batem_com_o_json_e_entre_as_tabelas() {
    let bruto: Value = serde_json::from_str(REAL).unwrap();
    let c = parse(REAL).unwrap();
    let t = &bruto["total"]["tokens"];
    assert_eq!(c.total_tokens.entrada, t["in"].as_u64().unwrap());
    assert_eq!(c.total_tokens.saida, t["out"].as_u64().unwrap());
    assert_eq!(c.total_tokens.cache_w, t["cache_w"].as_u64().unwrap());
    assert_eq!(c.total_tokens.cache_r, t["cache_r"].as_u64().unwrap());
    assert_eq!(c.total_tokens.total, t["total"].as_u64().unwrap());
    assert_eq!(c.total_usd, bruto["total"]["usd"].as_f64());
    // Cada tabela particiona as mesmas respostas: tokens somam o total; USD bate (sem arredondar).
    for tab in [&c.por_modelo, &c.por_dia, &c.por_semana, &c.por_task] {
        assert_eq!(soma_tok(tab), c.total_tokens);
        let d = soma_usd(tab).unwrap() - c.total_usd.unwrap();
        assert!(d.abs() < 1e-3, "diferença de US$ {d}");
    }
}

#[test]
fn contrato_real_cards_e_tabelas_formatados() {
    use crate::custosview::montar;
    let c = parse(REAL).unwrap();
    let hoje = d("2026-10-20");
    let v = montar(&c, Periodo::Tudo, hoje, None, Estilo::Virgula);
    assert_eq!(v.usd_total, fmt_usd(c.total_usd, Estilo::Virgula));
    assert_eq!(v.tokens_total, fmt_tokens(c.total_tokens.total, Estilo::Virgula));
    assert_eq!(v.modelos.total, 3);
    assert!(v.modelos.linhas[0].usd != "—");
    // Dia 2026-10-20 existe no real: o card «hoje» é ele, não zero.
    let dia = c.por_dia.iter().find(|l| l.chave == "2026-10-20").unwrap();
    assert_eq!(v.usd_hoje, fmt_usd(dia.usd, Estilo::Virgula));
    assert_eq!(v.tokens_hoje, fmt_tokens(dia.tokens.total, Estilo::Virgula));
    // Task única «sem-task»: o nome null do CLI não vira « · null».
    assert_eq!(c.por_task[0].chave, "sem-task");
    // Os 3 modelos do real têm preço (haiku sem confirmação só gera aviso).
    assert!(c.por_modelo.iter().all(|l| l.usd.is_some()));
    assert_eq!(v.avisos.len(), 1);
    // O mesmo conteúdo em inglês só muda o estilo numérico.
    let en = montar(&c, Periodo::Tudo, hoje, None, Estilo::Ponto);
    assert_eq!(en.usd_total, fmt_usd(c.total_usd, Estilo::Ponto));
}

#[test]
fn contrato_real_ledger_refiltra_modelo_e_task() {
    let tudo = agrega_ledger(LEDGER_REAL, "modelo", None);
    let linhas = LEDGER_REAL.lines().count() as u64;
    assert!(tudo.len() >= 2);
    // Toda resposta do ledger entra em exatamente um modelo.
    let esperado: u64 = LEDGER_REAL
        .lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            v["in"].as_u64().unwrap() + v["out"].as_u64().unwrap()
        })
        .sum();
    assert!(linhas > 0);
    let obtido: u64 = tudo.iter().map(|l| l.tokens.entrada + l.tokens.saida).sum();
    assert_eq!(obtido, esperado);
    let recorte = agrega_ledger(LEDGER_REAL, "task", Some(d("2026-10-15")));
    assert_eq!(recorte[0].chave, "sem-task");
}

#[test]
fn contrato_real_tolera_campo_novo_e_secao_ausente() {
    let mut v: Value = serde_json::from_str(REAL).unwrap();
    v["campo_do_futuro"] = serde_json::json!({"x": 1});
    v["por_modelo"][0]["campo_novo"] = serde_json::json!("y");
    v.as_object_mut().unwrap().remove("por_semana");
    v.as_object_mut().unwrap().remove("por_sessao");
    let c = parse(&v.to_string()).unwrap();
    assert!(c.por_semana.is_empty());
    assert_eq!(c.por_modelo.len(), 3);
    assert_eq!(c.total_usd, parse(REAL).unwrap().total_usd);
}
