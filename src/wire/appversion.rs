//! Fiação do rodapé do app: versão instalada, self-update, sininho de notificações
//! e a comparação fork vs oficial.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;

/// Resultado achatado de `skills::compare_update`, pronto pra cruzar o event loop:
/// (slug, versão, mudanças `nome→descrição`, resumo) ou o erro. Existe como alias porque
/// o tipo cru é ilegível na assinatura e o lint reclamava com razão.
type ComparacaoDeSkill = Result<(String, String, Vec<(String, String)>, String), String>;
use schematize::updaterboot;
use crate::wire::{set_rows, Ctx};

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
    app.global::<App>().set_updater_missing(!updaterboot::present());
    // ...e se FALTAR, instala SOZINHO em segundo plano. O botão continua ali (pra
    // retentar na mão), mas ninguém deveria precisar dele: quem instalou o app não
    // tem de saber que existe um gestor de atualizações separado — se ele não está
    // na máquina, o update degrada pro fluxo interno e vira "cliquei e não
    // aconteceu nada". Presente = só um stat (sem rede); ausente = uma tentativa,
    // limitada por carimbo em disco (ver `updaterboot`), pra máquina offline não
    // bater no GitHub a cada abertura.
    if !updaterboot::present() {
        let weak = app.as_weak();
        std::thread::spawn(move || {
            let outcome = updaterboot::ensure_now();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else { return };
                match outcome {
                    updaterboot::Outcome::Instalado(_) | updaterboot::Outcome::JaTinha => {
                        app.global::<App>().set_updater_missing(false);
                        app.global::<App>().set_updater_status(
                            tor("gui.updater_installed", "Gestor de atualizações instalado.").into(),
                        );
                    }
                    // Adiado/Falhou: mantém o prompt visível pra tentativa manual.
                    _ => app.global::<App>().set_updater_missing(true),
                }
            });
        });
    }
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
    app.global::<Notif>().set_historico(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));

    // BADGE: lê o CACHE (instantâneo, sem rede) e só DEPOIS sincroniza em thread.
    //
    // Era `notifications::count()`, que fazia a coleta de rede inteira — e o painel a
    // refazia ao abrir. Duas idas independentes pra a mesma pergunta, com este timer
    // repetindo a cada 90s; quando a segunda falhava, o badge dizia "3" e o painel
    // vinha vazio. Agora badge e painel leem a MESMA fonte.
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_refresh(move || {
            let Some(app) = weak.upgrade() else { return };
            // 1) resposta imediata, do disco.
            app.global::<Notif>().set_count(notifications::count() as i32);
            // 2) rede em segundo plano; se falhar, o cache continua valendo.
            let weak = weak.clone();
            std::thread::spawn(move || {
                let n = notifications::sincronizar();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Notif>().set_count(n as i32);
                        // Painel aberto durante a sincronização: repinta com o que chegou.
                        if app.global::<Notif>().get_open() {
                            preenche_painel(&app);
                        }
                    }
                });
            });
        });
    }
    // ABRIR O PAINEL: preenche do cache na hora e marca as novas como lidas.
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_toggle(move || {
            let Some(app) = weak.upgrade() else { return };
            let open = !app.global::<Notif>().get_open();
            app.global::<Notif>().set_open(open);
            if !open {
                return;
            }
            // Sem estado de "carregando": o conteúdo já está no disco. O spinner era o
            // sintoma — ele existia porque abrir o painel disparava rede.
            preenche_painel(&app);
            // Viu, então leu. Não apaga nada: só sai da contagem.
            notifications::marcar_lidas();
            app.global::<Notif>().set_count(notifications::count() as i32);
            // E aproveita pra buscar o que houver de novo, em segundo plano.
            app.global::<Notif>().invoke_refresh();
        });
    }
    // Concluir manualmente (o "✓" de cada item): vai pro histórico, não some.
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_concluir(move |id| {
            let Some(app) = weak.upgrade() else { return };
            if notifications::concluir(&id) {
                preenche_painel(&app);
                app.global::<Notif>().set_count(notifications::count() as i32);
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Notif>().on_action(move |kind, action, id| {
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
                //
                // A URL JÁ foi validada na fronteira (`notificacoes::formato`): só https,
                // sem userinfo, sem controle. Aqui não se revalida — um segundo lugar
                // decidindo o que é seguro é um segundo lugar pra divergir do primeiro.
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
                // Rótulo livre do servidor: INERTE de propósito. Ele chega saneado e
                // serve pra texto/ícone — nunca pra decidir comportamento.
                _ => {}
            }
            // Agiu sobre ela: vira histórico. Não some — some da fila.
            if !id.is_empty() {
                notifications::concluir(&id);
                app.global::<Notif>().set_count(notifications::count() as i32);
                preenche_painel(&app);
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
                let out: ComparacaoDeSkill =
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

/// Repinta os três modelos do painel a partir do CACHE — sem rede, no event loop.
///
/// Três listas porque são três estados de atenção: o que é novo/lido fica na frente
/// (agrupado por escopo, como sempre foi) e o que já foi resolvido vai pro histórico.
/// Concluída NÃO é apagada: some da fila, não da memória do projeto.
pub(crate) fn preenche_painel(app: &AppWindow) {
    use schematize::notificacoes::cache::Estado;
    let todas = notifications::listar();
    let linha = |r: &schematize::notificacoes::cache::Registro| NotifItem {
        id: r.id.clone().into(),
        scope: r.escopo.clone().into(),
        title: r.titulo.clone().into(),
        body: r.corpo.clone().into(),
        kind: r.kind.clone().into(),
        action: r.acao.clone().into(),
        has_action: !r.acao.is_empty(),
        estado: match r.estado {
            Estado::Nova => "nova",
            Estado::Lida => "lida",
            Estado::Concluida => "concluida",
        }
        .into(),
    };
    let pendentes: Vec<_> = todas.iter().filter(|r| r.estado != Estado::Concluida).collect();
    let g: Vec<NotifItem> = pendentes.iter().filter(|r| r.escopo == "global").map(|r| linha(r)).collect();
    let p: Vec<NotifItem> = pendentes.iter().filter(|r| r.escopo == "personal").map(|r| linha(r)).collect();
    let h: Vec<NotifItem> = todas.iter().filter(|r| r.estado == Estado::Concluida).map(linha).collect();
    let n = app.global::<Notif>();
    n.set_total(pendentes.len() as i32);
    n.set_loading(false);
    set_rows(&n.get_global(), g);
    set_rows(&n.get_personal(), p);
    set_rows(&n.get_historico(), h);
}
