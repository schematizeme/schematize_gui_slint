//! Fiação do que o projeto atual mostra além do seletor: o aviso de SKILLS desatualizadas
//! (dado do `schematize-skills`) e o atalho do portal.
//!
//! A caixa de entrada e as ações de item humano (responder/recusar) saíram na E6 M5: são as
//! abas «Caixa» e «Checklist» da janela do overdev, e a lógica (trava, idempotência, máquina de
//! estados) mora no app `schematize-overdev`, não aqui.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
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
                Ok(_) => {
                    aviso(&app, tor("gui.od_rerun_ok", "agente aberto pra reaplicar as skills."))
                }
                Err(e) => aviso(&app, e),
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

/// **O quê:** recontagem das skills atrasadas no projeto, para o aviso da aba Overdev.
///
/// **Onde:** [`crate::odproj::load_projeto`] (a função única que significa «a tela passou a
/// mostrar este projeto») e o rerun. Sem projeto zera o aviso, senão o do anterior ficaria
/// falando de outro lugar.
pub(crate) fn atualiza_skills(app: &AppWindow, root: Option<&Path>) {
    let o = app.global::<Od>();
    let atrasadas = root.map(|r| crate::skillsapplied::ler(r).atrasadas).unwrap_or_default();
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

/// Resultado do rerun das skills (sucesso ou motivo da falha), no status do projeto.
fn aviso(app: &AppWindow, msg: impl Into<String>) {
    app.global::<Od>().set_run_status(msg.into().into());
}
