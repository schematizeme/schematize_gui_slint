//! A FILA DE QUIZ do overdev, lida do CLI e traduzida para as linhas da tela.
//!
//! **O quê:** roda `schematize overdev questions --json` no projeto atual e devolve os cards.
//! **Onde:** [`crate::wire::quiz`], que registra os callbacks da aba Overdev.
//!
//! ## Parser PURO, e a razão tem nome
//!
//! [`ler`] recebe **texto** e devolve as linhas — nada de subprocesso dentro dela. Assim o
//! comportamento é afirmável com JSON inválido, truncado, de outro idioma ou hostil, sem
//! depender do que está instalado na máquina de quem roda a suíte.
//!
//! Este repo já pagou duas vezes por não ter feito isso: a janela que casava **rótulo em
//! português** e devolvia vazio nos outros dezenove idiomas sem erro nenhum, e a pílula que
//! saía vazia contra um binário de outra versão. As duas eram leitura de saída humana.
//!
//! ## Vazio e ERRO são coisas diferentes
//!
//! `Ok(vec![])` é "não há pergunta". Um JSON ilegível é `Err`, e a tela mostra o erro — porque
//! um `default()` silencioso diria "nenhuma pergunta" a quem tem cinco esperando, que é
//! exatamente o bug do ADR-0015 um nível abaixo.

use crate::i18nbind::tor;

/// Uma pergunta, já no formato que a tela consome.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Card {
    pub id: String,
    pub titulo: String,
    pub contexto: String,
    pub kind: String,
    pub opcoes: Vec<Opc>,
    pub destrava: String,
    pub aberta: bool,
    pub aguarda_revisao: bool,
    pub resposta_texto: String,
    pub respondida_em: String,
    pub revisao: String,
}

/// Uma opção clicável.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Opc {
    pub id: String,
    pub rotulo: String,
    pub detalhe: String,
    pub sugerida: bool,
}

/// **O quê:** as opções IMPLÍCITAS de um `kind`, que o JSON não declara.
///
/// **Onde:** [`ler`]. `aprovar` e `sim_nao_outro` têm sempre as mesmas, e o CLI as valida sem
/// que ninguém as escreva. A tela precisa DESENHÁ-LAS, então elas nascem aqui — com os mesmos
/// ids do CLI, senão o clique manda um id que ele recusa.
fn opcoes_implicitas(kind: &str) -> Vec<Opc> {
    let par = |id: &str, rotulo: String| Opc {
        id: id.to_string(),
        rotulo,
        detalhe: String::new(),
        sugerida: false,
    };
    match kind {
        "aprovar" => vec![
            par("aprovar", tor("gui.quiz_aprovar", "Aprovar")),
            par("negar", tor("gui.quiz_negar", "Negar")),
        ],
        "sim_nao_outro" => {
            vec![par("sim", tor("gui.quiz_sim", "Sim")), par("nao", tor("gui.quiz_nao", "Não"))]
        }
        _ => vec![],
    }
}

/// **O quê:** epoch → `"24/09 20:59"` em hora LOCAL.
///
/// **Onde:** [`ler`], para o card dizer QUANDO a pessoa respondeu — pedido explícito.
///
/// **Hora local e não UTC:** quem lê é a pessoa que respondeu, e ela pensa no fuso dela. Um
/// epoch 0 (relógio torto) vira vazio em vez de "01/01 00:00", que pareceria uma data real.
fn hora_local(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(epoch, 0) {
        chrono::offset::LocalResult::Single(dt) => dt.format("%d/%m %H:%M").to_string(),
        _ => String::new(),
    }
}

/// **O quê:** o texto do `--json` → os cards, na ordem em que a tela os mostra.
///
/// **Onde:** [`crate::wire::quiz`]. Função PURA.
///
/// **A ordem é deliberada:** primeiro as ABERTAS (é o que espera a pessoa), depois as que
/// aguardam a máquina, depois as revisadas. Ordenar por data poria uma pergunta respondida
/// ontem acima de uma aberta hoje.
pub(crate) fn ler(texto: &str) -> Result<Vec<Card>, String> {
    let v: serde_json::Value = serde_json::from_str(texto)
        .map_err(|e| format!("a fila de perguntas veio ilegível: {e}"))?;

    let mut cards = Vec::new();
    // A ordem dos grupos é a ordem da tela.
    for grupo in ["abertas", "aguardando_revisao", "revisadas"] {
        let Some(arr) = v.get(grupo).and_then(|x| x.as_array()) else { continue };
        for q in arr {
            let s = |k: &str| q.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let kind = s("kind");
            let auto = s("auto");

            let mut opcoes: Vec<Opc> = q
                .get("opcoes")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .map(|o| {
                            let g = |k: &str| {
                                o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
                            };
                            let id = g("id");
                            Opc {
                                sugerida: !auto.is_empty() && id == auto,
                                id,
                                rotulo: g("rotulo"),
                                detalhe: g("detalhe"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if opcoes.is_empty() {
                opcoes = opcoes_implicitas(&kind);
            }

            let resposta = q.get("resposta").filter(|r| !r.is_null());
            let revisada =
                resposta.and_then(|r| r.get("revisada")).and_then(|x| x.as_bool()).unwrap_or(false);
            let em = resposta.and_then(|r| r.get("em")).and_then(|x| x.as_i64()).unwrap_or(0);

            // `linka` vira texto corrido: a tela mostra o que a resposta destrava, e uma lista
            // de ids soltos não diz nada a quem não está com o checklist aberto do lado.
            let destrava = q
                .get("linka")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" · "))
                .unwrap_or_default();

            cards.push(Card {
                id: s("id"),
                titulo: s("titulo"),
                contexto: s("contexto"),
                kind,
                opcoes,
                destrava,
                aberta: resposta.is_none(),
                aguarda_revisao: resposta.is_some() && !revisada,
                resposta_texto: resposta_em_texto(q, resposta),
                respondida_em: hora_local(em),
                revisao: resposta
                    .and_then(|r| r.get("revisao"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    Ok(cards)
}

/// **O quê:** a resposta em linguagem de gente — RÓTULO da opção, nunca o id.
///
/// **Onde:** [`ler`]. O id é contrato de máquina; quem lê o card é pessoa.
fn resposta_em_texto(q: &serde_json::Value, resposta: Option<&serde_json::Value>) -> String {
    let Some(r) = resposta else { return String::new() };
    let rotulo_de = |id: &str| -> String {
        q.get("opcoes")
            .and_then(|x| x.as_array())
            .and_then(|a| {
                a.iter()
                    .find(|o| o.get("id").and_then(|x| x.as_str()) == Some(id))
                    .and_then(|o| o.get("rotulo"))
                    .and_then(|x| x.as_str())
                    .map(String::from)
            })
            .or_else(|| {
                let kind = q.get("kind").and_then(|x| x.as_str()).unwrap_or("");
                opcoes_implicitas(kind).into_iter().find(|o| o.id == id).map(|o| o.rotulo)
            })
            .unwrap_or_else(|| id.to_string())
    };
    let mut partes: Vec<String> = r
        .get("escolhas")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(rotulo_de).collect())
        .unwrap_or_default();
    let txt = r.get("texto").and_then(|x| x.as_str()).unwrap_or("").trim();
    if !txt.is_empty() {
        partes.push(txt.to_string());
    }
    partes.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = r#"{
      "abertas": [{
        "id": "q-a-1", "criada_em": 1790297245, "fase": "FH",
        "titulo": "`services` merece tela própria, ou fica como lista secundária?",
        "contexto": "Hoje é seção colapsável.", "kind": "escolha",
        "opcoes": [
          {"id": "propria", "rotulo": "Tela própria", "detalhe": "quinta aba"},
          {"id": "secundaria", "rotulo": "Lista secundária", "detalhe": ""}
        ],
        "auto": "secundaria", "linka": ["E3-tela-services"], "resposta": null
      }],
      "aguardando_revisao": [{
        "id": "q-b-2", "criada_em": 1790297000, "fase": "", "titulo": "Corto as tags?",
        "contexto": "", "kind": "aprovar", "opcoes": [], "auto": "", "linka": [],
        "resposta": {"em": 1790297100, "por": "humano", "escolhas": ["aprovar"],
                     "texto": "", "revisada": false, "revisao": ""}
      }],
      "revisadas": [{
        "id": "q-c-3", "criada_em": 1790296000, "fase": "", "titulo": "Windows fica?",
        "contexto": "", "kind": "sim_nao_outro", "opcoes": [], "auto": "", "linka": [],
        "resposta": {"em": 1790296500, "por": "humano", "escolhas": [],
                     "texto": "pode fazer", "revisada": true, "revisao": "fica no -msvc"}
      }],
      "ilegiveis": []
    }"#;

    #[test]
    fn le_os_tres_grupos_na_ordem_da_tela() {
        let c = ler(REAL).expect("lê");
        assert_eq!(c.len(), 3);
        assert_eq!(
            (c[0].aberta, c[1].aguarda_revisao, c[2].aberta),
            (true, true, false),
            "aberta primeiro, depois a que espera a máquina, depois a fechada"
        );
        assert_eq!(c[0].id, "q-a-1");
    }

    /// A sugestão do agente marca a opção certa — e NÃO marca as outras.
    #[test]
    fn auto_marca_uma_opcao_so() {
        let c = ler(REAL).expect("lê");
        let sug: Vec<&str> =
            c[0].opcoes.iter().filter(|o| o.sugerida).map(|o| o.id.as_str()).collect();
        assert_eq!(sug, vec!["secundaria"]);
    }

    /// `aprovar`/`sim_nao_outro` não declaram opção, e a tela precisa desenhar as duas.
    #[test]
    fn kind_implicito_ganha_as_opcoes_que_o_json_nao_traz() {
        let c = ler(REAL).expect("lê");
        let ids: Vec<&str> = c[1].opcoes.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["aprovar", "negar"], "sem isto o card viria sem botão");
        let ids: Vec<&str> = c[2].opcoes.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["sim", "nao"]);
    }

    /// A resposta aparece por RÓTULO, não por id — inclusive a de opção implícita.
    #[test]
    fn resposta_vem_por_rotulo() {
        let c = ler(REAL).expect("lê");
        assert_eq!(c[1].resposta_texto, "Aprovar", "id `aprovar` → rótulo");
        assert_eq!(c[2].resposta_texto, "pode fazer", "o texto do `outro` entra como está");
    }

    /// **A lição que este repo pagou duas vezes:** a decisão não pode depender da prosa.
    #[test]
    fn a_decisao_nao_muda_com_o_idioma_da_prosa() {
        let jp = REAL
            .replace("Tela própria", "専用画面")
            .replace("Lista secundária", "副リスト")
            .replace("Hoje é seção colapsável.", "現在は折りたたみセクションです。");
        let a = ler(REAL).expect("pt");
        let b = ler(&jp).expect("ja");
        // O que É decisão: ids, kind, estado, sugestão.
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.kind, y.kind);
            assert_eq!(x.aberta, y.aberta);
            assert_eq!(x.aguarda_revisao, y.aguarda_revisao);
            let ids = |c: &Card| c.opcoes.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
            assert_eq!(ids(x), ids(y));
            let sug = |c: &Card| c.opcoes.iter().map(|o| o.sugerida).collect::<Vec<_>>();
            assert_eq!(sug(x), sug(y));
        }
        // E o que é PROSA mudou de verdade — senão o teste passaria por não ter trocado nada.
        assert_ne!(a[0].opcoes[0].rotulo, b[0].opcoes[0].rotulo);
    }

    /// Fila vazia é `Ok(vec![])`, e ilegível é `Err`. Não são o mesmo estado.
    #[test]
    fn vazio_e_erro_sao_estados_diferentes() {
        let vazio = ler(r#"{"abertas":[],"aguardando_revisao":[],"revisadas":[],"ilegiveis":[]}"#);
        assert_eq!(vazio.expect("vazio é Ok"), vec![]);
        assert!(ler("{ isto nao e json").is_err(), "ilegível tem de ser Err, não lista vazia");
        // Um objeto sem os grupos é Ok e vazio: é JSON válido que simplesmente não tem fila.
        assert_eq!(ler("{}").expect("objeto vazio"), vec![]);
    }

    /// Entrada hostil não panica — janela que morre ao abrir é pior que lista vazia.
    #[test]
    fn entrada_hostil_nao_panica() {
        let profundo = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
        for lixo in [
            "",
            "null",
            "[]",
            "0",
            "\"texto\"",
            "\u{0}",
            &profundo,
            r#"{"abertas":"nao e lista"}"#,
            r#"{"abertas":[null]}"#,
            r#"{"abertas":[{"id":123,"kind":true,"opcoes":"x","linka":5,"resposta":7}]}"#,
            r#"{"abertas":[{"resposta":{"em":"nao e numero","escolhas":{}}}]}"#,
        ] {
            let _ = ler(lixo); // só não pode panicar
        }
    }

    /// Campo ausente não derruba a linha: ela vem com o campo vazio.
    #[test]
    fn campo_ausente_vira_vazio_em_vez_de_erro() {
        let c = ler(r#"{"abertas":[{"id":"q-x"}]}"#).expect("lê");
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].titulo.as_str(), c[0].kind.as_str()), ("", ""));
        assert!(c[0].aberta, "sem `resposta` é aberta");
        assert!(c[0].opcoes.is_empty(), "kind vazio não tem opção implícita");
    }

    /// Epoch 0 não vira uma data que parece real.
    #[test]
    fn epoch_zero_nao_inventa_data() {
        assert_eq!(hora_local(0), "");
        assert_eq!(hora_local(-5), "");
        assert!(!hora_local(1_790_297_245).is_empty());
    }
}
