//! Fiação da VERSÃO do app: o que está instalado, se há atualização, e o self-update.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI.
//!
//! ## Este arquivo tinha TRÊS assuntos, e o inventário contou os três como um
//!
//! Ele juntava versão, sininho de notificações e comparação fork-vs-oficial. O inventário da
//! extradição classificou as 12 unidades inteiras como "market — o market é o dono de versão",
//! e só duas delas são. O sininho é de notificações e a comparação é de skills; os dois saem
//! nas fases E5/E6, e não aqui.
//!
//! A separação foi feita ANTES de mexer neles: com os três no mesmo arquivo, cada remoção
//! futura seria cirurgia num arquivo que três fases disputam. Agora cada uma tem o seu.
//!
//! ## A versão vem do GESTOR, não de uma conta local (E4 M5)
//!
//! Ver [`crate::marketstatus`]: a conta que havia aqui ignorava o PIN, e o rodapé oferecia
//! atualização a quem fixou uma versão de propósito.

use crate::prelude::*;

use crate::wire::Ctx;
use schematize::gestorboot;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // ---- relançar o app (janela nova), depois do self-update ----
    //
    // **Esta linha veio do `wire/skills.rs`, e a E5 quase a levou junto.** Ela morava lá por
    // acidente histórico — o self-update nasceu ao lado da lista de skills. Ao apagar aquele
    // arquivo, o `restart_app` ficou sem NENHUM chamador e o `-D warnings` o apontou: o botão
    // «Reiniciar» que aparece depois de uma atualização teria ficado inerte, e nada mais
    // reprovaria isso — um callback do `.slint` sem fiação não dá erro, ele só não faz nada.
    //
    // O lugar dele é aqui: reiniciar é o último passo do fluxo de VERSÃO.
    {
        app.global::<App>().on_restart(move || crate::sysenv::restart_app());
    }

    // ==================== Versão do app + self-update ====================
    // "Verificar atualização" → `marketstatus::ler()` em thread; se há versão nova,
    // acende o botão "Atualizar app" que roda selfupdate::run() em thread e, ao
    // concluir, sugere reiniciar (o restart já existe: relança a janela nova).
    {
        let weak = app.as_weak();
        app.global::<App>().on_check_update(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<App>().get_checking() || app.global::<App>().get_updating() {
                return;
            }
            app.global::<App>().set_checking(true);
            app.global::<App>().set_update_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                // **PERGUNTA AO GESTOR, não à rede.** A conta antiga comparava a versão
                // instalada com a última tag do GitHub e ignorava o PIN — o rodapé anunciava
                // atualização a quem fixou versão de propósito, e o botão ao lado não
                // atualizava, porque o market respeita o pin. Ver `crate::marketstatus`.
                let res = crate::marketstatus::ler();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<App>().set_checking(false);
                        // `Some(true)` → há alvo novo; `Some(false)` → está em dia; `None` →
                        // não deu para perguntar. Os três estados têm mensagens próprias, e é
                        // o ponto: anunciar "atualizado" sobre um "não sei" é a mesma confusão
                        // entre ausência e vazio que já custou uma versão a este projeto.
                        let novo = res
                            .as_ref()
                            .filter(|s| s.ha_atualizacao() == Some(true))
                            .map(|s| s.alvo().to_string());
                        match novo {
                            Some(new) => {
                                app.global::<App>().set_has_update(true);
                                app.global::<App>().set_update_status(
                                    format!(
                                        "{} v{new}",
                                        tor("gui.app_new_version", "Nova versão disponível:")
                                    )
                                    .into(),
                                );
                            }
                            None => {
                                app.global::<App>().set_has_update(false);
                                // **Três estados, três frases.** "Está atualizado" só quando
                                // o gestor respondeu E a conta deu que não há alvo novo. Sem
                                // resposta, a frase é outra: dizer "atualizado" sobre um "não
                                // sei" é a mesma confusão entre ausência e vazio que já custou
                                // uma versão a este projeto, e aqui ela mandaria alguém dormir
                                // tranquilo numa máquina desatualizada.
                                let msg = match res.as_ref().map(|s| s.ha_atualizacao()) {
                                    Some(Some(false)) => {
                                        tor("gui.app_up_to_date", "Você está atualizado")
                                    }
                                    Some(None) => tor(
                                        "gui.app_version_unknown",
                                        "O gestor respondeu, mas não soube dizer a versão — verifique a rede.",
                                    ),
                                    _ => tor(
                                        "gui.app_manager_silent",
                                        "Não consegui perguntar ao schematize-market. Ele está instalado?",
                                    ),
                                };
                                app.global::<App>().set_update_status(msg.into());
                            }
                        }
                    }
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        app.global::<App>().on_do_update(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<App>().get_updating() {
                return;
            }
            app.global::<App>().set_updating(true);
            app.global::<App>().set_update_status(tor("gui.app_updating", "Atualizando…").into());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = selfupdate::run();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<App>().set_updating(false);
                        match res {
                            Ok(msg) => {
                                app.global::<App>().set_update_done(true);
                                app.global::<App>().set_has_update(false);
                                app.global::<App>().set_update_status(msg.into());
                            }
                            Err(e) => {
                                app.global::<App>()
                                    .set_update_status(tf("err.prefix", &[("e", &e)]).into());
                            }
                        }
                    }
                });
            });
        });
    }

    // Gestor de atualizações (`schematize-market`, ADR-0013): checa na ABERTURA se está
    // instalado; se faltar, a UI mostra o prompt "instalar". Cobre instalação limpa E update —
    // sem o gestor, o update central não roda. O botão baixa o binário (ensure_gestor) numa
    // thread. Até o ADR-0013 o gestor era o `schematize-updater`, absorvido pelo market.
    {
        let weak = app.as_weak();
        app.global::<App>().on_install_gestor(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<App>().get_gestor_installing() {
                return;
            }
            app.global::<App>().set_gestor_installing(true);
            app.global::<App>().set_gestor_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = selfupdate::ensure_gestor();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<App>().set_gestor_installing(false);
                        match res {
                            Ok(_p) => {
                                app.global::<App>().set_gestor_missing(false);
                                app.global::<App>().set_gestor_status(
                                    tor(
                                        "gui.gestor_installed",
                                        "Gestor de atualizações instalado.",
                                    )
                                    .into(),
                                );
                            }
                            Err(e) => app
                                .global::<App>()
                                .set_gestor_status(tf("err.prefix", &[("e", &e)]).into()),
                        }
                    }
                });
            });
        });
    }
    // Estado inicial do prompt: o gestor está presente?
    app.global::<App>().set_gestor_missing(!gestorboot::present());
    // ...e se FALTAR, instala SOZINHO em segundo plano. O botão continua ali (pra
    // retentar na mão), mas ninguém deveria precisar dele: quem instalou o app não
    // tem de saber que existe um gestor de atualizações separado — se ele não está
    // na máquina, o update degrada pro fluxo interno e vira "cliquei e não
    // aconteceu nada". Presente = só um stat (sem rede); ausente = uma tentativa,
    // limitada por carimbo em disco (ver `gestorboot`), pra máquina offline não
    // bater no GitHub a cada abertura.
    if !gestorboot::present() {
        let weak = app.as_weak();
        std::thread::spawn(move || {
            let outcome = gestorboot::ensure_now();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else { return };
                match outcome {
                    gestorboot::Outcome::Instalado(_) | gestorboot::Outcome::JaTinha => {
                        app.global::<App>().set_gestor_missing(false);
                        app.global::<App>().set_gestor_status(
                            tor("gui.gestor_installed", "Gestor de atualizações instalado.").into(),
                        );
                    }
                    // Adiado/Falhou: mantém o prompt visível pra tentativa manual.
                    _ => app.global::<App>().set_gestor_missing(true),
                }
            });
        });
    }
    // Startup: checa update do app em background pra a bolinha de update do header (versão) acender
    // sozinha, sem o usuário precisar clicar "Verificar atualização".
    {
        let weak = app.as_weak();
        std::thread::spawn(move || {
            // A MESMA fonte do botão: o gestor. Duas leituras do mesmo fato divergem, e a
            // bolinha acesa por uma conta que ignora o pin sobre um botão que o respeita é o
            // pior dos dois mundos — o aviso aparece e a ação não acontece.
            let has = crate::marketstatus::ler().and_then(|s| s.ha_atualizacao()) == Some(true);
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    app.global::<App>().set_has_update(has);
                }
            });
        });
    }
}
