//! Fiação do SININHO de notificações.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI.
//!
//! **Saiu do `appversion.rs` quando ele foi separado por assunto.** Aquele arquivo juntava
//! versão, sininho e comparação de fork, e o inventário da extradição contou as 12 unidades
//! inteiras como "market — o market é o dono de versão". Só as de versão são. Este recorte é
//! de NOTIFICAÇÕES, e sai na fase que levar o overdev, não na do market.
//!
//! A separação foi feita ANTES de mexer neles: com os três assuntos no mesmo arquivo, cada
//! remoção futura seria cirurgia num arquivo que três fases disputam.
//!
//! Os modelos (Global/Pessoal) são REMONTADOS no event loop a cada abertura: eles não cruzam a
//! fronteira da thread, que é o padrão thread→UI do resto desta janela.

use crate::prelude::*;
use crate::wire::{set_rows, Ctx};

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // ==================== Sininho de notificações ====================
    // Os modelos (Global/Pessoal) são REMONTADOS no event loop a cada abertura (não
    // cruzam a fronteira da thread — padrão thread→UI do resto da GUI). A ação de
    // cada item viaja pelo próprio callback (kind, action), sem estado Rust extra.
    app.global::<Notif>()
        .set_global(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));
    app.global::<Notif>()
        .set_personal(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));
    app.global::<Notif>()
        .set_historico(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));

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
                // Skill desatualizada → leva à aba de Skills, que agora DELEGA.
                //
                // A paginação e a lista saíram na E5: quem desenha skill é a janela do
                // `schematize-skills`. O que a notificação faz é o que ela sempre fez —
                // levar a pessoa ao lugar certo —, e o lugar certo passou a ser a tela que
                // abre aquela janela.
                "skill_outdated" => {
                    app.global::<Notif>().set_open(false);
                    app.set_screen(1);
                    app.set_active_tab(0);
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
    let g: Vec<NotifItem> =
        pendentes.iter().filter(|r| r.escopo == "global").map(|r| linha(r)).collect();
    let p: Vec<NotifItem> =
        pendentes.iter().filter(|r| r.escopo == "personal").map(|r| linha(r)).collect();
    let h: Vec<NotifItem> =
        todas.iter().filter(|r| r.estado == Estado::Concluida).map(linha).collect();
    let n = app.global::<Notif>();
    n.set_total(pendentes.len() as i32);
    n.set_loading(false);
    set_rows(&n.get_global(), g);
    set_rows(&n.get_personal(), p);
    set_rows(&n.get_historico(), h);
}
