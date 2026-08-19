//! Fiação do rodapé do app: versão instalada, self-update, sininho de notificações
//! e a comparação fork vs oficial.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // ==================== Versão do app + self-update ====================
    // "Verificar atualização" → app_update_available() em thread; se há versão nova,
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
                let res = upgrade::app_update_available();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<App>().set_checking(false);
                        match res {
                            Some((_cur, new)) => {
                                app.global::<App>().set_has_update(true);
                                app.global::<App>().set_update_status(
                                    format!("{} v{new}", tor("gui.app_new_version", "Nova versão disponível:")).into(),
                                );
                            }
                            None => {
                                app.global::<App>().set_has_update(false);
                                app.global::<App>().set_update_status(tor("gui.app_up_to_date", "Você está atualizado").into());
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
                                app.global::<App>().set_update_status(tf("err.prefix", &[("e", &e)]).into());
                            }
                        }
                    }
                });
            });
        });
    }

    // Gestor de atualizações (schematize-updater): checa na ABERTURA se está instalado; se faltar,
    // a UI mostra o prompt "instalar". Cobre instalação limpa E update — sem o updater, o update
    // central não roda. O botão baixa o binário do updater (ensure_updater) numa thread.
    {
        let weak = app.as_weak();
        app.global::<App>().on_install_updater(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<App>().get_updater_installing() {
                return;
            }
            app.global::<App>().set_updater_installing(true);
            app.global::<App>().set_updater_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = selfupdate::ensure_updater();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<App>().set_updater_installing(false);
                        match res {
                            Ok(_p) => {
                                app.global::<App>().set_updater_missing(false);
                                app.global::<App>().set_updater_status(
                                    tor("gui.updater_installed", "Gestor de atualizações instalado.").into(),
                                );
                            }
                            Err(e) => app.global::<App>().set_updater_status(tf("err.prefix", &[("e", &e)]).into()),
                        }
                    }
                });
            });
        });
    }
    // Estado inicial do prompt: o updater está presente?
    app.global::<App>().set_updater_missing(selfupdate::updater_bin().is_none());
    // Startup: checa update do app em background pra a bolinha de update do header (versão) acender
    // sozinha, sem o usuário precisar clicar "Verificar atualização".
    {
        let weak = app.as_weak();
        std::thread::spawn(move || {
            let has = upgrade::app_update_available().is_some();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    app.global::<App>().set_has_update(has);
                }
            });
        });
    }

    // ==================== Sininho de notificações ====================
    // Os modelos (Global/Pessoal) são REMONTADOS no event loop a cada abertura (não
    // cruzam a fronteira da thread — padrão thread→UI do resto da GUI). A ação de
    // cada item viaja pelo próprio callback (kind, action), sem estado Rust extra.
    app.global::<Notif>().set_global(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));
    app.global::<Notif>().set_personal(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));

    // recompute só a contagem (badge) — barato de disparar, roda em thread.
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_refresh(move || {
            let weak = weak.clone();
            std::thread::spawn(move || {
                let n = notifications::count();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Notif>().set_count(n as i32);
                    }
                });
            });
        });
    }
    // abrir o painel: mostra loading e colhe collect() em thread; ao voltar, monta
    // os dois modelos (Global/Pessoal) no event loop e atualiza o badge.
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_toggle(move || {
            let Some(app) = weak.upgrade() else { return };
            let open = !app.global::<Notif>().get_open();
            app.global::<Notif>().set_open(open);
            if !open {
                return;
            }
            app.global::<Notif>().set_loading(true);
            let weak = weak.clone();
            std::thread::spawn(move || {
                let notifs = notifications::collect();
                // extrai o que cruza a fronteira da thread (tudo String/bool/Send).
                let rows: Vec<(bool, String, String, String, String, bool)> = notifs
                    .iter()
                    .map(|n| {
                        (
                            matches!(n.scope, notifications::NotifScope::Global),
                            n.title.clone(),
                            n.body.clone(),
                            n.kind.clone(),
                            n.action.clone().unwrap_or_default(),
                            n.action.is_some(),
                        )
                    })
                    .collect();
                let total = rows.len();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        let mut gv: Vec<NotifItem> = Vec::new();
                        let mut pv: Vec<NotifItem> = Vec::new();
                        for (idx, (global, title, body, kind, action, has_action)) in rows.into_iter().enumerate() {
                            let item = NotifItem {
                                idx: idx as i32,
                                scope: if global { "global".into() } else { "personal".into() },
                                title: title.into(),
                                body: body.into(),
                                kind: kind.into(),
                                action: action.into(),
                                has_action,
                            };
                            if global {
                                gv.push(item);
                            } else {
                                pv.push(item);
                            }
                        }
                        app.global::<Notif>().set_global(ModelRc::from(Rc::new(VecModel::from(gv))));
                        app.global::<Notif>().set_personal(ModelRc::from(Rc::new(VecModel::from(pv))));
                        app.global::<Notif>().set_total(total as i32);
                        app.global::<Notif>().set_count(total as i32);
                        app.global::<Notif>().set_loading(false);
                    }
                });
            });
        });
    }
    // executar a ação de uma notificação — (kind, action) vêm do próprio item.
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_action(move |kind, action| {
            let Some(app) = weak.upgrade() else { return };
            match kind.as_str() {
                // nova versão do app → fecha o painel, vai pra Configurações e dispara o update.
                "app_update" => {
                    app.global::<Notif>().set_open(false);
                    app.set_screen(5);
                    app.global::<App>().set_has_update(true);
                    app.global::<App>().invoke_do_update();
                }
                // post do blog → abre a URL no navegador.
                "news" => {
                    let url = action.to_string();
                    if !url.is_empty() {
                        util::open_url(&url);
                    }
                }
                // skill desatualizada → leva pra aba Instaladas do Mercado.
                "skill_outdated" => {
                    app.global::<Notif>().set_open(false);
                    app.set_screen(1);
                    app.set_active_tab(0);
                    app.global::<Mp>().set_page(0);
                    recompute_pagination(&app);
                }
                _ => {}
            }
        });
    }
    // contagem inicial + refresh periódico (a cada 90s) do badge, em thread.
    app.global::<Notif>().invoke_refresh();
    let notif_timer = Rc::new(slint::Timer::default());
    {
        let weak = app.as_weak();
        notif_timer.start(TimerMode::Repeated, Duration::from_secs(90), move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Notif>().invoke_refresh();
            }
        });
    }

    // ==================== Comparar fork vs oficial ====================
    // "Comparar com oficial" → compare_update(slug) em thread; abre o painel com
    // base→nova, arquivos (status) e o diff. NÃO sobrescreve nada.
    app.global::<Cmp>().set_files(ModelRc::from(Rc::new(VecModel::<CmpFile>::from(Vec::new()))));
    {
        let weak = app.as_weak();
        app.global::<Cmp>().on_request(move |slug| {
            let Some(app) = weak.upgrade() else { return };
            let slug = slug.to_string();
            if slug.is_empty() {
                return;
            }
            app.global::<Cmp>().set_open(true);
            app.global::<Cmp>().set_loading(true);
            app.global::<Cmp>().set_error(SharedString::new());
            app.global::<Cmp>().set_diff(SharedString::new());
            app.global::<Cmp>().set_versions(SharedString::new());
            app.global::<Cmp>().set_slug(slug.clone().into());
            app.global::<Cmp>().set_title(format!("{} {slug}", tor("gui.compare_title", "Comparar:")).into());
            app.global::<Cmp>().set_files(ModelRc::from(Rc::new(VecModel::<CmpFile>::from(Vec::new()))));
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skills::compare_update(&slug);
                // extrai os campos (String/bool) antes de cruzar pro event loop.
                let out: Result<(String, String, Vec<(String, String)>, String), String> =
                    res.map(|c| {
                        (
                            c.base_version,
                            c.new_version,
                            c.files.into_iter().map(|f| (f.path, f.status)).collect(),
                            c.diff_text,
                        )
                    });
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Cmp>().set_loading(false);
                        match out {
                            Ok((base, new, files, diff)) => {
                                app.global::<Cmp>().set_versions(format!("v{base} → v{new}").into());
                                app.global::<Cmp>().set_diff(if diff.trim().is_empty() {
                                    tor("gui.compare_identical", "(sem diferenças de conteúdo)").into()
                                } else {
                                    diff.into()
                                });
                                app.global::<Cmp>().set_files(ModelRc::from(Rc::new(VecModel::from(
                                    files
                                        .into_iter()
                                        .map(|(path, status)| CmpFile { path: path.into(), status: status.into() })
                                        .collect::<Vec<CmpFile>>(),
                                ))));
                            }
                            Err(e) => app.global::<Cmp>().set_error(e.into()),
                        }
                    }
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Cmp>().on_close(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Cmp>().set_open(false);
            }
        });
    }

}
