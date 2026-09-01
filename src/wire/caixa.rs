//! Fiação da CAIXA DE ENTRADA e das ações de item humano (responder/recusar),
//! mais o aviso de skills desatualizadas neste projeto.
//!
//! Tudo aqui opera num `root` explícito — a GUI observa um projeto que não é o cwd,
//! então nada pode usar as funções cwd-relative do lib. A lógica (trava, idempotência,
//! máquina de estados) mora no lib; aqui é só ligar o botão nela.

use crate::prelude::*;
use crate::wire::Ctx;
use schematize::overdev::{caixa, resposta, trava};

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    // ---- capturar demanda (instantâneo, não toca o checklist) --------------
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        app.global::<Od>().on_caixa_add(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(root) = cur.borrow().clone() else {
                aviso(&app, tor("gui.od_no_project", "Selecione um projeto primeiro."), true);
                return;
            };
            let texto = app.global::<Od>().get_caixa_text().to_string();
            match caixa::adicionar(&root, &texto) {
                Ok(_) => {
                    app.global::<Od>().set_caixa_text(SharedString::new());
                    aviso(
                        &app,
                        tor("gui.od_caixa_ok", "capturado — o checklist não foi tocado."),
                        false,
                    );
                    atualiza_caixa(&app, Some(&root));
                }
                Err(e) => aviso(&app, e, true),
            }
        });
    }

    // ---- abrir o agente organizador num terminal ---------------------------
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        app.global::<Od>().on_caixa_agent(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(root) = cur.borrow().clone() else { return };
            let n = caixa::pendentes(&root).len();
            if n == 0 {
                return;
            }
            let prompt = caixa::prompt_agente(&schematize_bin(), n);
            match agentrun::launch_prompt_in_terminal(&root, &prompt) {
                Ok(_) => {
                    aviso(&app, tor("gui.od_caixa_agent_ok", "agente aberto no terminal."), false)
                }
                Err(e) => aviso(&app, e, true),
            }
        });
    }

    // ---- fundir no checklist (sob trava, no lib) ---------------------------
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        let cl = cx.od_cl.clone();
        app.global::<Od>().on_caixa_merge(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(root) = cur.borrow().clone() else { return };
            match caixa::mesclar(&root) {
                Ok(n) => {
                    aviso(
                        &app,
                        format!("{n} {}", tor("gui.od_caixa_merged", "item(ns) no checklist.")),
                        false,
                    );
                    // Recarrega o checklist: os itens novos têm de aparecer na hora,
                    // senão parece que a fusão não fez nada.
                    load_overdev_into(&app, &cl, Some(&root));
                    atualiza_caixa(&app, Some(&root));
                }
                Err(e) => aviso(&app, e, true),
            }
        });
    }

    // ---- pedir resolução de um item humano (abre o modal) ------------------
    {
        let weak = app.as_weak();
        app.global::<Od>().on_resolve_request(move |idx, texto, responder| {
            let Some(app) = weak.upgrade() else { return };
            let o = app.global::<Od>();
            o.set_resolve_index(idx);
            o.set_resolve_item(texto);
            o.set_resolve_answer(responder);
            o.set_resolve_text(SharedString::new());
            o.set_resolve_status(SharedString::new());
            o.set_resolve_error(false);
            o.set_resolve_open(true);
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Od>().on_resolve_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_resolve_open(false);
            }
        });
    }

    // ---- confirmar: aplica a máquina de estados do lib, sob trava ----------
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        let cl = cx.od_cl.clone();
        app.global::<Od>().on_resolve_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(root) = cur.borrow().clone() else { return };
            let o = app.global::<Od>();
            let acao = if o.get_resolve_answer() {
                resposta::Acao::Responder
            } else {
                resposta::Acao::Recusar
            };
            let alvo = resposta::Alvo::Indice(o.get_resolve_index().max(1) as usize);
            let texto = o.get_resolve_text().to_string();
            match resolver_em(&root, &alvo, acao, &texto) {
                Ok(r) => {
                    o.set_resolve_open(false);
                    let msg = match (&r.vinculado, acao) {
                        (Some(m), resposta::Acao::Responder) => {
                            format!("{} {m}", tor("gui.od_released", "liberado pra máquina:"))
                        }
                        (Some(m), resposta::Acao::Recusar) => {
                            format!("{} {m}", tor("gui.od_cancelled", "cancelado:"))
                        }
                        (None, _) => tor("gui.od_resolved", "item resolvido."),
                    };
                    aviso(&app, msg, false);
                    load_overdev_into(&app, &cl, Some(&root));
                }
                Err(e) => {
                    o.set_resolve_error(true);
                    o.set_resolve_status(e.into());
                }
            }
        });
    }

    // ---- rerodar as skills desatualizadas deste projeto --------------------
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        app.global::<Od>().on_skills_rerun(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(root) = cur.borrow().clone() else { return };
            let alvos = skillsproj::desatualizadas(&root);
            if alvos.is_empty() {
                return;
            }
            let prompt = skillsproj::prompt_rerun(&schematize_bin(), &alvos);
            match agentrun::launch_prompt_in_terminal(&root, &prompt) {
                Ok(_) => aviso(
                    &app,
                    tor("gui.od_rerun_ok", "agente aberto pra reaplicar as skills."),
                    false,
                ),
                Err(e) => aviso(&app, e, true),
            }
        });
    }

    // ---- abrir um destino do portal no NAVEGADOR do usuário ----------------
    {
        app.global::<App>().on_open_portal(move |alvo| {
            // O lib é dono das URLs canônicas; a GUI não as reescreve. Alvo
            // desconhecido não abre nada — melhor que abrir a página errada.
            if let Some(u) = links::url_for(&alvo) {
                util::open_url(u);
            }
        });
    }
}

/// Resolve um item humano num `root` explícito, com o MESMO ciclo do CLI:
/// ler e escrever sob a mesma trava, usando a máquina de estados pura do lib.
///
/// Não duplica a regra — só a I/O path-aware, que é o que o lib não expõe (as funções
/// dele são cwd-relative e a GUI observa outro diretório).
fn resolver_em(
    root: &Path,
    alvo: &resposta::Alvo,
    acao: resposta::Acao,
    texto: &str,
) -> Result<resposta::Resolucao, String> {
    let cl = schematize::paths::overdev_dir_at(root).join("CHECKLIST.md");
    let r = trava::com_trava(&cl, || {
        let s = std::fs::read_to_string(&cl).map_err(|e| e.to_string())?;
        let r = resposta::resolver_str(&s, alvo, acao, texto)?;
        trava::escreve_atomico(&cl, &r.texto)?;
        Ok(r)
    })?;
    // A decisão vai pro registro durável do projeto — mesmo destino do CLI.
    let dec = schematize::paths::overdev_dir_at(root).join("DECISOES.md");
    let rotulo = if acao == resposta::Acao::Responder { "RESPOSTA" } else { "RECUSA" };
    // Mesmo cuidado do CLI: `unwrap_or_default` mapeia falha de leitura para vazio, e o
    // `escreve_atomico` seguinte reescreveria o DECISOES.md inteiro a partir do vazio —
    // o historico de decisoes do projeto apagado por um byte nao-UTF-8.
    let mut atual = schematize::util::ler_para_modificar(&dec)?;
    atual.push_str(&format!("\n## {rotulo}: {}\n\n{texto}\n", r.item));
    trava::escreve_atomico(&dec, &atual)?;
    Ok(r)
}

/// Recontagem da caixa + do estado das skills. Chamada ao trocar de projeto e após
/// cada ação — os números da tela são a única forma de saber que há algo pendente.
pub(crate) fn atualiza_caixa(app: &AppWindow, root: Option<&Path>) {
    let o = app.global::<Od>();
    let Some(root) = root else {
        o.set_caixa_pending(0);
        o.set_caixa_ready(0);
        o.set_skills_outdated(0);
        o.set_skills_summary(SharedString::new());
        return;
    };
    o.set_caixa_pending(caixa::pendentes(root).len() as i32);
    o.set_caixa_ready(caixa::processadas(root).len() as i32);
    let atrasadas = skillsproj::desatualizadas(root);
    o.set_skills_outdated(atrasadas.len() as i32);
    o.set_skills_summary(
        atrasadas
            .iter()
            .map(|(s, de, para)| format!("{s} v{de} → v{para}"))
            .collect::<Vec<_>>()
            .join(" · ")
            .into(),
    );
}

/// Banner da caixa de entrada (sucesso ou erro).
fn aviso(app: &AppWindow, msg: impl Into<String>, erro: bool) {
    let o = app.global::<Od>();
    o.set_caixa_status(msg.into().into());
    o.set_caixa_error(erro);
}
