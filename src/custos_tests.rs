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
