//! FIAÇÃO do QUIZ — os callbacks da fila de perguntas da aba Overdev.
//!
//! **O quê:** lê a fila pelo CLI, popula os cards, e manda as respostas de volta.
//! **Onde:** `wire::quiz::wire`, uma vez, no arranque.
//!
//! ## Por que tudo passa pelo `schematize` instalado, e não por uma cópia
//!
//! A validação da fila (opção declarada, texto só onde cabe, uma-de-N) vive no CLI. Reimplementá-la
//! aqui criaria duas leis para o mesmo formato, e a que divergisse gravaria um arquivo que o outro
//! lado recusa. O mesmo raciocínio do `marketlink.rs`, uma porta ao lado.
//!
//! **Consequência boa:** a janela não pode escrever uma resposta inválida nem se quiser — quem
//! grava é o CLI, e ele valida.

use super::Ctx;
use crate::prelude::*;
use crate::quizmodel::{self, Card};

/// **O quê:** roda `schematize overdev questions --json` na raiz do projeto.
///
/// **Onde:** [`recarregar`]. Devolve o texto cru, para o parser PURO decidir.
///
/// **`schematize` resolvido por caminho, não pelo nome:** o lançador do desktop dá PATH mínimo,
/// e é a armadilha que já quebrou o ícone deste ecossistema mais de uma vez.
fn ler_do_cli(raiz: &std::path::Path) -> Result<String, String> {
    let bin = sysenv::schematize_bin();
    let saida = std::process::Command::new(&bin)
        .args(["overdev", "questions", "--json"])
        .current_dir(raiz)
        .output()
        .map_err(|e| format!("{bin}: {e}"))?;
    if !saida.status.success() {
        // O stderr do CLI é o que explica (sem run de overdev, projeto sem `.schematize`, …).
        let msg = String::from_utf8_lossy(&saida.stderr).trim().to_string();
        return Err(if msg.is_empty() { format!("{bin} falhou") } else { msg });
    }
    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

/// **O quê:** relê a fila e escreve os globais da tela.
///
/// **Onde:** o arranque, o `quiz-refresh`, e depois de CADA resposta — senão o card responderia
/// e continuaria desenhado como aberto, e a pessoa clicaria de novo.
pub(crate) fn recarregar(app: &AppWindow, raiz: Option<&std::path::Path>) {
    let od = app.global::<Od>();
    let Some(raiz) = raiz else {
        od.set_quiz(ModelRc::new(VecModel::from(Vec::<QuizItem>::new())));
        od.set_quiz_abertas(0);
        od.set_quiz_revisar(0);
        return;
    };
    let cards = match ler_do_cli(raiz).and_then(|t| quizmodel::ler(&t)) {
        Ok(c) => c,
        Err(e) => {
            // ERRO APARECE. Um `default()` silencioso diria "nenhuma pergunta" a quem tem cinco
            // esperando — o mesmo bug do ADR-0015, um nível abaixo.
            od.set_quiz(ModelRc::new(VecModel::from(Vec::<QuizItem>::new())));
            od.set_quiz_abertas(0);
            od.set_quiz_revisar(0);
            od.set_quiz_status(e.into());
            od.set_quiz_erro(true);
            return;
        }
    };
    od.set_quiz_abertas(cards.iter().filter(|c| c.aberta).count() as i32);
    od.set_quiz_revisar(cards.iter().filter(|c| c.aguarda_revisao).count() as i32);
    od.set_quiz(ModelRc::new(VecModel::from(
        cards.iter().map(|c| item_de(c, &[])).collect::<Vec<_>>(),
    )));
}

/// **O quê:** um [`Card`] → a struct que o `.slint` consome.
///
/// **Onde:** [`recarregar`]. `marcadas` são os ids que o usuário já marcou num `multipla` — eles
/// vivem no Rust entre um clique e o Confirmar, porque a tela é redesenhada a cada mudança.
fn item_de(c: &Card, marcadas: &[String]) -> QuizItem {
    QuizItem {
        id: c.id.clone().into(),
        titulo: c.titulo.clone().into(),
        contexto: c.contexto.clone().into(),
        kind: c.kind.clone().into(),
        destrava: c.destrava.clone().into(),
        aberta: c.aberta,
        aguarda_revisao: c.aguarda_revisao,
        resposta_texto: c.resposta_texto.clone().into(),
        respondida_em: c.respondida_em.clone().into(),
        revisao: c.revisao.clone().into(),
        opcoes: ModelRc::new(VecModel::from(
            c.opcoes
                .iter()
                .map(|o| QuizOpcao {
                    id: o.id.clone().into(),
                    rotulo: o.rotulo.clone().into(),
                    detalhe: o.detalhe.clone().into(),
                    sugerida: o.sugerida,
                    marcada: marcadas.contains(&o.id),
                })
                .collect::<Vec<_>>(),
        )),
    }
}

/// **O quê:** roda um subcomando da fila e devolve a mensagem para a tela.
///
/// **Onde:** todos os callbacks de resposta. Um lugar só, porque o tratamento é idêntico: o
/// stderr do CLI **é** a mensagem de erro que a pessoa precisa ler (ele explica por que a
/// resposta foi recusada, e essa explicação é o produto do formato).
fn rodar(raiz: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let bin = sysenv::schematize_bin();
    let saida = std::process::Command::new(&bin)
        .args(args)
        .current_dir(raiz)
        .output()
        .map_err(|e| format!("{bin}: {e}"))?;
    let out = String::from_utf8_lossy(&saida.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&saida.stderr).trim().to_string();
    if saida.status.success() {
        Ok(out)
    } else {
        Err(if err.is_empty() { out } else { err })
    }
}

/// **O quê:** registra os callbacks do quiz.
///
/// **Onde:** `main`, junto dos outros `wire::*::wire`.
pub(crate) fn wire(app: &AppWindow, ctx: &Ctx) {
    let od = app.global::<Od>();

    // O que o usuário marcou num `multipla`, entre o clique e o Confirmar.
    let marcadas: std::rc::Rc<RefCell<HashMap<String, Vec<String>>>> =
        std::rc::Rc::new(RefCell::new(HashMap::new()));

    // Responder por CLIQUE numa opção.
    {
        let w = app.as_weak();
        let cur = ctx.od_current.clone();
        od.on_quiz_responder(move |qid, opcao| {
            aplicar(&w, &cur, &["overdev", "reply", &qid, "--escolha", &opcao]);
        });
    }
    // Responder com TEXTO (o "outro", ou `livre`).
    {
        let w = app.as_weak();
        let cur = ctx.od_current.clone();
        od.on_quiz_responder_texto(move |qid, texto| {
            if texto.trim().is_empty() {
                // Resposta vazia não é resposta — e dizer isso aqui evita o round-trip.
                if let Some(a) = w.upgrade() {
                    let od = a.global::<Od>();
                    od.set_quiz_status(
                        tor("gui.quiz_vazia", "escreva a resposta antes de enviar").into(),
                    );
                    od.set_quiz_erro(true);
                }
                return;
            }
            aplicar(&w, &cur, &["overdev", "reply", &qid, "--texto", texto.trim()]);
        });
    }
    // `multipla`: marca/desmarca sem enviar.
    {
        let w = app.as_weak();
        let m = marcadas.clone();
        od.on_quiz_marcar(move |qid, opcao| {
            {
                let mut m = m.borrow_mut();
                let v = m.entry(qid.to_string()).or_default();
                if let Some(i) = v.iter().position(|x| x == opcao.as_str()) {
                    v.remove(i);
                } else {
                    v.push(opcao.to_string());
                }
            }
            // Redesenha só as marcas, sem reler o CLI: reler aqui perderia as marcas, porque a
            // fila no disco ainda não sabe delas.
            if let Some(a) = w.upgrade() {
                remarcar(&a, &m.borrow());
            }
        });
    }
    // `multipla`: confirma o conjunto marcado.
    {
        let w = app.as_weak();
        let cur = ctx.od_current.clone();
        let m = marcadas.clone();
        od.on_quiz_confirmar_multipla(move |qid| {
            let escolhas = m.borrow().get(qid.as_str()).cloned().unwrap_or_default();
            if escolhas.is_empty() {
                if let Some(a) = w.upgrade() {
                    let od = a.global::<Od>();
                    od.set_quiz_status(
                        tor("gui.quiz_sem_marca", "marque ao menos uma opção").into(),
                    );
                    od.set_quiz_erro(true);
                }
                return;
            }
            let mut args: Vec<String> = vec!["overdev".into(), "reply".into(), qid.to_string()];
            for e in &escolhas {
                args.push("--escolha".into());
                args.push(e.clone());
            }
            let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            aplicar(&w, &cur, &refs);
            m.borrow_mut().remove(qid.as_str());
        });
    }
    // As duas ações que o usuário pediu ao lado das respostas.
    {
        let w = app.as_weak();
        let cur = ctx.od_current.clone();
        od.on_quiz_marcar_feito(move |qid| {
            aplicar(&w, &cur, &["overdev", "reply", &qid, "--texto", "feito"]);
        });
    }
    {
        let w = app.as_weak();
        let cur = ctx.od_current.clone();
        od.on_quiz_recusar(move |qid| {
            // `refuse` CANCELA o item de máquina vinculado, em vez de fingir que foi feito —
            // mentir no progresso seria trocar um problema por outro.
            aplicar(&w, &cur, &["overdev", "refuse", &qid, "recusado pela janela"]);
        });
    }
    {
        let w = app.as_weak();
        let cur = ctx.od_current.clone();
        od.on_quiz_refresh(move || {
            if let Some(a) = w.upgrade() {
                let r = cur.borrow().clone();
                recarregar(&a, r.as_deref());
            }
        });
    }
}

/// **O quê:** roda o comando, mostra o resultado e RELÊ a fila.
///
/// **Onde:** todos os callbacks de resposta.
///
/// **Reler é obrigatório:** sem isso o card responderia e continuaria desenhado como aberto, e a
/// pessoa clicaria de novo — gravando a resposta duas vezes.
fn aplicar(w: &Weak<AppWindow>, raiz_atual: &Rc<RefCell<Option<PathBuf>>>, args: &[&str]) {
    let Some(app) = w.upgrade() else { return };
    let od = app.global::<Od>();
    let Some(raiz) = raiz_atual.borrow().clone() else {
        od.set_quiz_status(tor("gui.quiz_sem_projeto", "nenhum projeto aberto").into());
        od.set_quiz_erro(true);
        return;
    };
    match rodar(&raiz, args) {
        Ok(msg) => {
            od.set_quiz_status(msg.lines().next().unwrap_or("").to_string().into());
            od.set_quiz_erro(false);
        }
        Err(e) => {
            // A mensagem do CLI é o produto: ela diz POR QUE a resposta foi recusada.
            od.set_quiz_status(e.lines().next().unwrap_or("").to_string().into());
            od.set_quiz_erro(true);
        }
    }
    od.set_quiz_outro_input(String::new().into());
    recarregar(&app, Some(&raiz));
}

/// **O quê:** reescreve só as marcas de `multipla`, preservando o resto.
///
/// **Onde:** `on_quiz_marcar`.
fn remarcar(app: &AppWindow, marcadas: &HashMap<String, Vec<String>>) {
    let od = app.global::<Od>();
    let atual = od.get_quiz();
    let novos: Vec<QuizItem> = atual
        .iter()
        .map(|mut q| {
            let vazio = Vec::new();
            let marcas = marcadas.get(q.id.as_str()).unwrap_or(&vazio);
            q.opcoes = ModelRc::new(VecModel::from(
                q.opcoes
                    .iter()
                    .map(|mut o| {
                        o.marcada = marcas.contains(&o.id.to_string());
                        o
                    })
                    .collect::<Vec<_>>(),
            ));
            q
        })
        .collect();
    od.set_quiz(ModelRc::new(VecModel::from(novos)));
}
