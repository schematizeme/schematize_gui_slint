//! Fiação da CAIXA DE ENTRADA e das ações de item humano (responder/recusar),
//! mais o aviso de skills desatualizadas neste projeto.
//!
//! Tudo aqui opera num `root` explícito — a GUI observa um projeto que não é o cwd; o app roda
//! com ele como diretório corrente. A lógica (trava, idempotência, máquina de estados) mora no
//! app `schematize-overdev`; aqui é só ligar o botão nela, por [`crate::overdevcli`].

use crate::prelude::*;
use crate::wire::Ctx;

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
            match crate::overdevcli::capturar_demanda(&root, &texto) {
                Ok(()) => {
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
            // O app monta o prompt do organizador. Sem demanda pendente não há o que organizar.
            if matches!(crate::overdevcli::caixa(&root), Ok((0, _))) {
                return;
            }
            match crate::overdevcli::organizar_caixa(&root) {
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
            match crate::overdevcli::fundir_caixa(&root) {
                Ok(frase) => {
                    aviso(&app, frase, false);
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
            let responder = o.get_resolve_answer();
            let texto = o.get_resolve_text().to_string();
            match crate::overdevcli::resolver_humano(
                &root,
                o.get_resolve_index(),
                responder,
                &texto,
            ) {
                Ok(frase) => {
                    o.set_resolve_open(false);
                    // A frase vem do app: «liberado pra máquina: …» / «cancelado: …».
                    aviso(&app, frase, false);
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
            // O prompt vem pronto do `schematize-skills` — ver `skillsapplied`.
            let aplicadas = crate::skillsapplied::ler(&root);
            if aplicadas.atrasadas.is_empty() {
                return;
            }
            match crate::overdevcli::terminal(
                &root,
                crate::overdevcli::Terminal::Prompt(&aplicadas.prompt),
            ) {
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
    // Falha do app NÃO zera os contadores (zero afirma «caixa vazia»): o motivo vai ao banner.
    match crate::overdevcli::caixa(root) {
        Ok((pendentes, prontas)) => {
            o.set_caixa_pending(pendentes as i32);
            o.set_caixa_ready(prontas as i32);
        }
        Err(e) => aviso(app, e, true),
    }
    let atrasadas = crate::skillsapplied::ler(root).atrasadas;
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
