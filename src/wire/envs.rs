//! Fiação da aba Environments e do modal de instalação do Mercado (a skill mais
//! a base recomendada e o environment da linguagem, num passo só).
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// O comando que instala a janela do Mercado. Uma constante, e não um `format!` espalhado: ele
/// aparece em TRÊS lugares — o texto visível na tela, o eco no topo do terminal e o comando
/// executado. Se os três divergirem, o erro que a pessoa copiar para pedir ajuda será sobre um
/// comando que ninguém rodou.
const COMANDO_INSTALAR_JANELA: &str =
    "cargo install --git https://github.com/schematizeme/schematize_updater_gui_rs";

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let row_items = cx.row_items.clone();
    let modal = cx.modal.clone();
    // ==================== aba MERCADO ====================
    //
    // Esta aba DELEGA: ela abre a janela do market em vez de desenhar a lista (ADR-0012).
    // Os quatro callbacks que havia aqui — escolher método, instalar, remover, recarregar —
    // saíram junto com a tela. Quem os tem agora é a janela do market, que é de quem eles são.

    // Estado inicial: a janela está instalada? A resposta é resolvida UMA vez, ao subir, e não
    // a cada clique — e é ela que decide entre "abrir" e "instalar", em vez de um botão que
    // tenta e falha em silêncio.
    {
        let gui = crate::sysenv::market_gui_bin();
        let cfg = app.global::<Cfg>();
        cfg.set_mercado_presente(gui.is_some());
        // O comando fica VISÍVEL mesmo com o botão ali do lado: quem prefere o terminal não
        // deveria ter de adivinhá-lo, e quem for pedir ajuda tem o que colar.
        cfg.set_mercado_cmd(SharedString::from(COMANDO_INSTALAR_JANELA));
    }

    // Abrir a janela do market.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_mercado_abrir(move || {
            let Some(a) = weak.upgrade() else { return };
            if crate::sysenv::abrir_market_gui() {
                a.global::<Cfg>().set_mercado_msg(SharedString::new());
                return;
            }
            // Falhou o spawn de uma janela que ESTAVA lá: pode ter sido removida entre o
            // arranque e o clique. A aba volta ao estado honesto em vez de insistir.
            a.global::<Cfg>().set_mercado_presente(false);
            a.global::<Cfg>().set_mercado_msg(SharedString::from(
                "não consegui abrir a janela do Mercado — ela ainda está instalada?",
            ));
        });
    }

    // Instalar a janela, num TERMINAL — compila do fonte por minutos, e é o mesmo caminho que
    // o resto da casa usa para trabalho longo: progresso, `Ctrl-C` e erro copiável de verdade.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_mercado_instalar(move || {
            let Some(a) = weak.upgrade() else { return };
            let inner = format!(
                "echo '── {COMANDO_INSTALAR_JANELA} ──'; echo; \
                 {COMANDO_INSTALAR_JANELA}; \
                 echo; read -n1 -s -r -p '…'"
            );
            let msg = if launch_terminal(&inner) {
                t("gui.env_terminal_opened")
            } else {
                tf("gui.env_no_terminal", &[("cmd", COMANDO_INSTALAR_JANELA)])
            };
            a.global::<Cfg>().set_mercado_msg(msg.into());
        });
    }

    // ==================== modal de instalação (Marketplace) ====================

    {
        let weak = app.as_weak();
        app.global::<Mp>().on_toggle_rec(move || {
            if let Some(a) = weak.upgrade() {
                a.global::<Mp>().set_rec_check(!a.global::<Mp>().get_rec_check());
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Mp>().on_toggle_env(move || {
            if let Some(a) = weak.upgrade() {
                a.global::<Mp>().set_env_check(!a.global::<Mp>().get_env_check());
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Mp>().on_pick_method(move |m| {
            if let Some(a) = weak.upgrade() {
                a.global::<Mp>().set_method_sel(m);
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Mp>().on_cancel(move || {
            if let Some(a) = weak.upgrade() {
                a.global::<Mp>().set_open(false);
            }
        });
    }
    // confirmar: instala a skill in-process (+ a base marcada, no MESMO lote paralelo)
    // e, se marcado, dispara o environment num TERMINAL (fora do processo).
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        let modal = modal.clone();
        app.global::<Mp>().on_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let st = modal.borrow().clone();
            // lote in-process: a skill + (recomendada SÓ se o usuário marcou).
            let mut ops: Vec<(usize, bool, Item)> = Vec::new();
            if let Some(Some(it)) = row_items.get(st.skill_idx) {
                ops.push((st.skill_idx, true, it.clone()));
            }
            if app.global::<Mp>().get_rec_check() && !st.rec_slug.is_empty() {
                if let Some(ridx) = row_idx_of_slug(&row_items, &st.rec_slug) {
                    if let Some(Some(rit)) = row_items.get(ridx) {
                        ops.push((ridx, true, rit.clone()));
                    }
                }
            }
            // environment opcional → terminal (só se marcado + método escolhido).
            let do_env = app.global::<Mp>().get_env_check() && !st.env_lang.is_empty();
            let env_method = app.global::<Mp>().get_method_sel().to_string();
            app.global::<Mp>().set_open(false);
            run_batch(weak.clone(), ops);
            if do_env && !env_method.is_empty() {
                // O environment ainda é instalado a partir daqui: o modal do marketplace
                // oferece instalar a linguagem junto com a skill, e isso não é a lista do
                // mercado — é um passo do fluxo de instalar skill.
                let label = run_env_action("install", &st.env_lang, &env_method);
                // A mensagem vai para a barra de status das skills, e só. Antes ela era
                // refletida também no card da aba Environments; aquela aba deixou de desenhar
                // a lista (ADR-0012), então não há mais card onde refletir.
                app.global::<Sk>().set_status(SharedString::from(label));
            }
        });
    }
}
