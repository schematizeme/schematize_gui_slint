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

/// O comando que instala a janela do Deployer. Mesma razão da constante acima: ele aparece no
/// texto visível, no eco do terminal e no comando executado, e três literais divergem.
const COMANDO_INSTALAR_DEPLOYER: &str =
    "cargo install --git https://github.com/schematizeme/schematize_deployer_gui_rs";

/// O comando que instala a janela do Database. Mesma razão das constantes acima.
///
/// **Note o `--bins`, que as irmãs não têm.** A janela do database é o SEGUNDO binário do repo
/// do CLI dele (ADR-0020), e um `cargo install` sem `--bins` recusa um pacote com mais de um
/// binário pedindo que se escolha um. Sem esta flag, o botão abriria um terminal que falha com
/// um erro do cargo sobre algo que a pessoa não pediu.
const COMANDO_INSTALAR_DATABASE: &str =
    "cargo install --git https://github.com/schematizeme/schematize_database_rs --bins";

/// O comando que instala a janela do Git. O `--bins` pela mesma razão do database: a janela é
/// o SEGUNDO binário do repo do CLI dele (ADR-0020), e o cargo recusa um pacote com mais de um
/// binário sem a flag.
const COMANDO_INSTALAR_GIT: &str =
    "cargo install --git https://github.com/schematizeme/schematize_git_rs --bins";

/// O comando que instala a janela do Optimizer.
///
/// **Sem `--bins` aqui, e a assimetria é real:** a janela do optimizer tem REPO PRÓPRIO
/// (`schematize_optimizer_gui_rs`), enquanto as do database e do git são o segundo binário do
/// repo do CLI delas (ADR-0020). Copiar a flag sem olhar faria o cargo reclamar de uma
/// ambiguidade que não existe neste repo.
const COMANDO_INSTALAR_OPTIMIZER: &str =
    "cargo install --git https://github.com/schematizeme/schematize_optimizer_gui_rs";

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
            if crate::sysenv::abrir_gui(crate::sysenv::market_gui_bin(), None) {
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

    // ==================== telas DELEGADAS ao deployer ====================
    //
    // Chaves SSH e Hosts eram TELAS deste hub, alimentadas pelos módulos `sshkeys`/`vps` do
    // crate. Elas foram embora junto com os módulos: o deployer é um app à parte desde o
    // ADR-0010, e tem a própria janela com as três telas (chaves, hosts, cofre).
    //
    // E havia um motivo a mais para a de CHAVES não morar aqui: ela tinha FORMULÁRIO DE
    // PASSPHRASE. Um campo de senha numa janela põe o segredo na memória do processo sem
    // necessidade nenhuma — o `ssh-keygen` já sabe lê-la sem eco. A janela do deployer não tem
    // campo de entrada nenhum, e há teste do lado de lá que reprova um.
    {
        let cfg = app.global::<Cfg>();
        cfg.set_deployer_presente(crate::sysenv::deployer_gui_bin().is_some());
        cfg.set_deployer_cmd(SharedString::from(COMANDO_INSTALAR_DEPLOYER));
    }

    // Duas aberturas para a MESMA janela, em abas diferentes: quem clicou em "Chaves SSH" não
    // deveria cair em Hosts e ter de clicar de novo.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_deployer_abrir_chaves(move || {
            if let Some(a) = weak.upgrade() {
                abrir_deployer(&a, "--chaves");
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_deployer_abrir_hosts(move || {
            if let Some(a) = weak.upgrade() {
                abrir_deployer(&a, "--hosts");
            }
        });
    }

    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_deployer_instalar(move || {
            let Some(a) = weak.upgrade() else { return };
            let inner = format!(
                "echo '── {COMANDO_INSTALAR_DEPLOYER} ──'; echo; \
                 {COMANDO_INSTALAR_DEPLOYER}; \
                 echo; read -n1 -s -r -p '…'"
            );
            let msg = if launch_terminal(&inner) {
                t("gui.env_terminal_opened")
            } else {
                tf("gui.env_no_terminal", &[("cmd", COMANDO_INSTALAR_DEPLOYER)])
            };
            a.global::<Cfg>().set_deployer_msg(msg.into());
        });
    }

    // ==================== tela DELEGADA ao database ====================
    //
    // A tela de Banco de dados era deste hub: introspecção, editor de schema, gerador de SQL e
    // um grafo inteiro. Ela foi embora junto com o módulo `database` do crate (E1 da extradição,
    // ADR-0018) — o `schematize-database` tem app e janela próprios.
    {
        let cfg = app.global::<Cfg>();
        cfg.set_database_presente(crate::sysenv::database_gui_bin().is_some());
        cfg.set_database_cmd(SharedString::from(COMANDO_INSTALAR_DATABASE));
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_database_abrir(move || {
            let Some(a) = weak.upgrade() else { return };
            // Sem flag de aba: a janela do database abre no Schema, que é de onde tudo parte.
            // Quem quiser o SQL clica uma vez lá — e é honesto, porque não há como este hub
            // saber qual das duas telas a pessoa queria.
            if crate::sysenv::abrir_gui(crate::sysenv::database_gui_bin(), None) {
                a.global::<Cfg>().set_database_msg(SharedString::new());
                return;
            }
            // Estava lá ao subir e não abriu: pode ter sido removida no meio. A tela volta ao
            // estado honesto em vez de insistir num botão que não funciona.
            a.global::<Cfg>().set_database_presente(false);
            a.global::<Cfg>().set_database_msg(SharedString::from(
                "não consegui abrir a janela do Database — ela ainda está instalada?",
            ));
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_database_instalar(move || {
            let Some(a) = weak.upgrade() else { return };
            let inner = format!(
                "echo '── {COMANDO_INSTALAR_DATABASE} ──'; echo; \
                 {COMANDO_INSTALAR_DATABASE}; \
                 echo; read -n1 -s -r -p '…'"
            );
            let msg = if launch_terminal(&inner) {
                t("gui.env_terminal_opened")
            } else {
                tf("gui.env_no_terminal", &[("cmd", COMANDO_INSTALAR_DATABASE)])
            };
            a.global::<Cfg>().set_database_msg(msg.into());
        });
    }

    // ==================== tela DELEGADA ao git ====================
    //
    // Contas, o que ainda não saiu desta máquina e os repositórios saíram daqui com o módulo
    // `githist` (E2 da extradição, ADR-0019). E havia um motivo a mais: aplicar identidade e
    // escrever alias podem PEDIR CREDENCIAL, e uma janela Slint não tem como responder a um
    // prompt — lá as duas ações abrem TERMINAL, que é onde o git e o ssh sabem perguntar.
    {
        let cfg = app.global::<Cfg>();
        cfg.set_git_presente(crate::sysenv::git_gui_bin().is_some());
        cfg.set_git_cmd(SharedString::from(COMANDO_INSTALAR_GIT));
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_git_abrir(move || {
            let Some(a) = weak.upgrade() else { return };
            // Sem flag de aba: a janela do git abre em Projetos, que é a pergunta mais urgente
            // que ela responde — o que pode sumir com a máquina.
            if crate::sysenv::abrir_gui(crate::sysenv::git_gui_bin(), None) {
                a.global::<Cfg>().set_git_msg(SharedString::new());
                return;
            }
            a.global::<Cfg>().set_git_presente(false);
            a.global::<Cfg>().set_git_msg(SharedString::from(
                "não consegui abrir a janela do Git — ela ainda está instalada?",
            ));
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_git_instalar(move || {
            let Some(a) = weak.upgrade() else { return };
            let inner = format!(
                "echo '── {COMANDO_INSTALAR_GIT} ──'; echo; \
                 {COMANDO_INSTALAR_GIT}; \
                 echo; read -n1 -s -r -p '…'"
            );
            let msg = if launch_terminal(&inner) {
                t("gui.env_terminal_opened")
            } else {
                tf("gui.env_no_terminal", &[("cmd", COMANDO_INSTALAR_GIT)])
            };
            a.global::<Cfg>().set_git_msg(msg.into());
        });
    }

    // ==================== tela DELEGADA ao optimizer ====================
    //
    // O inventário do lixo recriável saiu daqui com o módulo que o alimentava (E3). A janela
    // do optimizer já tinha Diagnóstico e Limites; agora tem cinco abas, e Disco é uma delas —
    // que é onde ela sempre devia ter morado, porque quem MEDE a máquina é aquele app.
    {
        let cfg = app.global::<Cfg>();
        cfg.set_optimizer_presente(crate::sysenv::optimizer_gui_bin().is_some());
        cfg.set_optimizer_cmd(SharedString::from(COMANDO_INSTALAR_OPTIMIZER));
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_optimizer_abrir_disco(move || {
            let Some(a) = weak.upgrade() else { return };
            // COM flag de aba: quem clicou em "Disco" no hub não deveria cair no Diagnóstico
            // e ter de achar a aba de novo. A janela aceita `--disco` desde a E3.
            if crate::sysenv::abrir_gui(crate::sysenv::optimizer_gui_bin(), Some("--disco")) {
                a.global::<Cfg>().set_optimizer_msg(SharedString::new());
                return;
            }
            a.global::<Cfg>().set_optimizer_presente(false);
            a.global::<Cfg>().set_optimizer_msg(SharedString::from(
                "não consegui abrir a janela do Optimizer — ela ainda está instalada?",
            ));
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_optimizer_instalar(move || {
            let Some(a) = weak.upgrade() else { return };
            let inner = format!(
                "echo '── {COMANDO_INSTALAR_OPTIMIZER} ──'; echo; \
                 {COMANDO_INSTALAR_OPTIMIZER}; \
                 echo; read -n1 -s -r -p '…'"
            );
            let msg = if launch_terminal(&inner) {
                t("gui.env_terminal_opened")
            } else {
                tf("gui.env_no_terminal", &[("cmd", COMANDO_INSTALAR_OPTIMIZER)])
            };
            a.global::<Cfg>().set_optimizer_msg(msg.into());
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

/// **O quê:** abre a janela do Deployer na aba pedida, e conta à tela o que aconteceu.
///
/// **Onde:** os botões das telas de Chaves SSH e de Hosts.
///
/// **Por que é uma FUNÇÃO e não o corpo do callback.** Duas razões, e a segunda é a que
/// decidiu:
///
/// 1. Os dois callbacks fazem a mesma coisa com uma flag diferente, e duas cópias de um
///    tratamento de erro são duas cópias que divergem.
/// 2. **O índice de funcionalidades não enxerga corpo de closure.** Enquanto isto era um
///    `move || { … }`, a aresta `schematize_gui_slint -> schematize_deployer_gui_rs` — a
///    dependência mais nova do hub — simplesmente não aparecia no grafo global. Um grafo que
///    omite a dependência mais nova é um grafo que se consulta e engana, e o propósito dele é
///    justamente responder "quem chama quem" antes de alguém mexer.
///
/// **O `false` do spawn não é ignorado.** Se a janela ESTAVA lá e não abriu, ela pode ter sido
/// removida entre o arranque e o clique — a tela volta ao estado honesto (oferecendo instalar)
/// em vez de insistir num botão que não funciona.
fn abrir_deployer(app: &AppWindow, aba: &str) {
    if crate::sysenv::abrir_gui(crate::sysenv::deployer_gui_bin(), Some(aba)) {
        app.global::<Cfg>().set_deployer_msg(SharedString::new());
        return;
    }
    app.global::<Cfg>().set_deployer_presente(false);
    app.global::<Cfg>().set_deployer_msg(SharedString::from(
        "não consegui abrir a janela do Deployer — ela ainda está instalada?",
    ));
}
