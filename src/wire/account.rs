//! Fiação da conta: login por device flow (rede em thread, cancelável), estado da
//! sessão e logout.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // ==================== Conta (login via device flow) ====================
    // Estado da sessão + fluxo de login OAuth device flow. `device_start` e o loop
    // de `device_poll_once` são REDE — rodam numa thread (nunca bloqueiam o event
    // loop); a UI é tocada só via `invoke_from_event_loop`. O loop é CANCELÁVEL: o
    // flag corrente vive num `Rc<RefCell<Arc<AtomicBool>>>` (padrão do worker do
    // overdev). Cada login troca por um flag NOVO e levanta o antigo, encerrando
    // qualquer thread remanescente; Cancelar/Sair levantam o flag corrente.
    // Só dados `Send` (String/PathBuf/Arc) cruzam a fronteira.
    let acc_stop: Rc<RefCell<Arc<AtomicBool>>> = Rc::new(RefCell::new(Arc::new(AtomicBool::new(false))));
    // Estado inicial: reflete a sessão persistida em disco.
    app.global::<Acc>().set_logged_in(account::is_logged_in());
    app.global::<Acc>().set_sub(account::account_sub().unwrap_or_default().into());

    // iniciar o device flow.
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.global::<Acc>().on_login(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<Acc>().get_polling() {
                return; // já há um login em andamento
            }
            // levanta o flag antigo (encerra thread remanescente) e cria um novo.
            acc_stop.borrow().store(true, Ordering::SeqCst);
            let stop = Arc::new(AtomicBool::new(false));
            *acc_stop.borrow_mut() = stop.clone();
            app.global::<Acc>().set_polling(true);
            app.global::<Acc>().set_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                match account::device_start() {
                    Err(e) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak.upgrade() {
                                app.global::<Acc>().set_polling(false);
                                app.global::<Acc>().set_status(
                                    format!("{} {e}", tor("gui.acc_start_error", "Falha ao iniciar o login:")).into(),
                                );
                            }
                        });
                    }
                    Ok(dl) => {
                        // Mostra o código + a URL e abre o modal.
                        let user_code = dl.user_code.clone();
                        let verification_uri = dl.verification_uri.clone();
                        let verification_complete = dl.verification_uri_complete.clone();
                        {
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(app) = weak.upgrade() {
                                    app.global::<Acc>().set_user_code(user_code.into());
                                    app.global::<Acc>().set_verification_uri(verification_uri.into());
                                    app.global::<Acc>().set_verification_uri_complete(verification_complete.into());
                                    app.global::<Acc>().set_status(SharedString::new());
                                    app.global::<Acc>().set_modal_open(true);
                                }
                            });
                        }
                        // Loop de poll (respeita interval/expires_in; cancelável via `stop`).
                        let start = Instant::now();
                        let mut interval = dl.interval.max(1);
                        loop {
                            if stop.load(Ordering::SeqCst) {
                                return; // cancelado/substituído — a UI já foi tratada
                            }
                            if start.elapsed().as_secs() >= dl.expires_in {
                                let weak = weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        app.global::<Acc>().set_modal_open(false);
                                        app.global::<Acc>().set_polling(false);
                                        app.global::<Acc>().set_status(
                                            tor("gui.acc_expired", "O código expirou. Tente novamente.").into(),
                                        );
                                    }
                                });
                                return;
                            }
                            // dorme `interval` em passos de 1s pra reagir rápido ao cancelamento.
                            let mut slept = 0u64;
                            while slept < interval {
                                if stop.load(Ordering::SeqCst) {
                                    return;
                                }
                                std::thread::sleep(Duration::from_secs(1));
                                slept += 1;
                            }
                            match account::device_poll_once(&dl.device_code) {
                                Ok(account::PollResult::Pending) => {}
                                Ok(account::PollResult::SlowDown) => interval += 5,
                                Ok(account::PollResult::Denied) => {
                                    let weak = weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = weak.upgrade() {
                                            app.global::<Acc>().set_modal_open(false);
                                            app.global::<Acc>().set_polling(false);
                                            app.global::<Acc>().set_status(
                                                tor("gui.acc_denied", "Acesso negado. Tente novamente.").into(),
                                            );
                                        }
                                    });
                                    return;
                                }
                                Ok(account::PollResult::Expired) => {
                                    let weak = weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = weak.upgrade() {
                                            app.global::<Acc>().set_modal_open(false);
                                            app.global::<Acc>().set_polling(false);
                                            app.global::<Acc>().set_status(
                                                tor("gui.acc_expired", "O código expirou. Tente novamente.").into(),
                                            );
                                        }
                                    });
                                    return;
                                }
                                Ok(account::PollResult::Ok(tokens)) => {
                                    let sub = tokens.sub.clone();
                                    let save_err = account::save_tokens(&tokens).err();
                                    let weak = weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = weak.upgrade() {
                                            app.global::<Acc>().set_modal_open(false);
                                            app.global::<Acc>().set_polling(false);
                                            match save_err {
                                                None => {
                                                    app.global::<Acc>().set_logged_in(true);
                                                    app.global::<Acc>().set_sub(sub.into());
                                                    app.global::<Acc>().set_status(SharedString::new());
                                                    // recomputa o badge do sino (notificações do
                                                    // servidor aparecem quando logado).
                                                    app.global::<Notif>().invoke_refresh();
                                                }
                                                Some(e) => app.global::<Acc>().set_status(
                                                    format!("{} {e}", tor("gui.acc_save_error", "Falha ao salvar a sessão:")).into(),
                                                ),
                                            }
                                        }
                                    });
                                    return;
                                }
                                // erro de rede transitório: mantém o poll (não derruba o fluxo).
                                Err(_) => {}
                            }
                        }
                    }
                }
            });
        });
    }

    // abrir a verification_uri_complete no navegador.
    {
        let weak = app.as_weak();
        app.global::<Acc>().on_open_verify(move || {
            if let Some(app) = weak.upgrade() {
                let url = app.global::<Acc>().get_verification_uri_complete().to_string();
                if !url.is_empty() {
                    util::open_url(&url);
                }
            }
        });
    }

    // cancelar o login (para o loop de poll + fecha o modal).
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.global::<Acc>().on_cancel_login(move || {
            acc_stop.borrow().store(true, Ordering::SeqCst);
            if let Some(app) = weak.upgrade() {
                app.global::<Acc>().set_modal_open(false);
                app.global::<Acc>().set_polling(false);
                app.global::<Acc>().set_status(SharedString::new());
            }
        });
    }

    // sair (logout): encerra a sessão + atualiza a UI + recomputa o badge do sino.
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.global::<Acc>().on_logout(move || {
            // por segurança, para qualquer poll em andamento.
            acc_stop.borrow().store(true, Ordering::SeqCst);
            account::logout();
            if let Some(app) = weak.upgrade() {
                app.global::<Acc>().set_logged_in(false);
                app.global::<Acc>().set_sub(SharedString::new());
                app.global::<Acc>().set_polling(false);
                app.global::<Acc>().set_modal_open(false);
                app.global::<Acc>().set_status(SharedString::new());
                // notificações do servidor somem quando deslogado → recomputa o badge.
                app.global::<Notif>().invoke_refresh();
            }
        });
    }

}
