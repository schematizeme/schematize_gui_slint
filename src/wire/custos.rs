//! Fiação da aba Custos: lê o `custos.json` do projeto atual, monta a visão e dispara o
//! `schematize cost sync` em thread. Nada de cálculo aqui — ver [`crate::custos`] (modelo),
//! [`crate::custosview`] (texto pronto) e [`crate::custoscli`] (o CLI).

use crate::custos::{self, Estado, Estilo, Periodo};
use crate::custosi18n::tc;
use crate::custosview::{self, Tabela};
use crate::prelude::*;
use crate::wire::Ctx;

/// Escreve os rótulos no idioma atual. Chamado por `install_i18n` (arranque e troca de idioma).
pub(crate) fn instala_rotulos(app: &AppWindow) {
    let c = app.global::<Cu>();
    let s = |k: &str| SharedString::from(tc(k));
    c.set_l_title(s("title"));
    c.set_l_home_desc(s("home_desc"));
    c.set_l_sync(s("sync"));
    c.set_l_syncing(s("syncing"));
    c.set_l_disclaimer(s("disclaimer"));
    c.set_l_total(s("card_total"));
    c.set_l_tokens(s("card_tokens"));
    c.set_l_unit(s("unit_tokens"));
    c.set_l_today(s("card_today"));
    c.set_l_week(s("card_week"));
    c.set_l_period(s("card_period"));
    c.set_l_periodos(ModelRc::from(Rc::new(VecModel::from(vec![
        s("per_all"),
        s("per_7"),
        s("per_30"),
        s("per_month"),
    ]))));
    c.set_l_t_model(s("t_model"));
    c.set_l_t_day(s("t_day"));
    c.set_l_t_week(s("t_week"));
    c.set_l_t_task(s("t_task"));
    c.set_l_in(s("c_in"));
    c.set_l_out(s("c_out"));
    c.set_l_cache(s("c_cache"));
    c.set_l_usd(s("c_usd"));
    c.set_l_item(s("c_item"));
    c.set_l_no_rows(s("no_rows"));
    c.set_l_note(s("all_time_note"));
    c.set_l_empty_title(s("empty_title"));
    c.set_l_empty_body(s("empty_body"));
}

fn linhas(t: &Tabela) -> ModelRc<CuRow> {
    ModelRc::from(Rc::new(VecModel::from(
        t.linhas
            .iter()
            .map(|l| CuRow {
                chave: l.chave.as_str().into(),
                entrada: l.entrada.as_str().into(),
                saida: l.saida.as_str().into(),
                cache: l.cache.as_str().into(),
                usd: l.usd.as_str().into(),
            })
            .collect::<Vec<_>>(),
    )))
}

fn trunc(t: &Tabela) -> SharedString {
    if t.total > t.linhas.len() {
        tc("truncated").replace("{n}", &t.linhas.len().to_string()).into()
    } else {
        SharedString::new()
    }
}

/// Relê o arquivo do projeto e repinta a aba. **Mantém** a mensagem de `Sincronizar` quando
/// `manter_msg` (o resultado da ação não some ao repintar).
pub(crate) fn recarregar(app: &AppWindow, root: Option<&Path>, manter_msg: bool) {
    let c = app.global::<Cu>();
    let estilo = Estilo::do_idioma(&i18n::current_code());
    let periodo = Periodo::de_indice(c.get_periodo());
    if !manter_msg {
        c.set_msg(SharedString::new());
        c.set_msg_erro(false);
    }
    let (estado, msg) = match custos::ler(root) {
        Estado::Pronto(cu) => {
            let ledger = root
                .and_then(custos::dir_custos)
                .and_then(|d| std::fs::read_to_string(d.join("ledger.jsonl")).ok());
            let hoje = chrono::Local::now().date_naive();
            let v = custosview::montar(&cu, periodo, hoje, ledger.as_deref(), estilo);
            c.set_filtrado(v.filtrado);
            c.set_nota_sem_filtro(v.sem_filtro_modelo_task);
            c.set_usd_total(v.usd_total.as_str().into());
            c.set_tokens_total(v.tokens_total.as_str().into());
            c.set_tokens_detalhe(
                format!(
                    "{} {} · {} {} · {} {}",
                    tc("c_in"),
                    v.tokens_entrada,
                    tc("c_out"),
                    v.tokens_saida,
                    tc("c_cache"),
                    v.tokens_cache
                )
                .into(),
            );
            c.set_usd_hoje(v.usd_hoje.as_str().into());
            c.set_tokens_hoje(v.tokens_hoje.as_str().into());
            c.set_usd_semana(v.usd_semana.as_str().into());
            c.set_tokens_semana(v.tokens_semana.as_str().into());
            c.set_modelos(linhas(&v.modelos));
            c.set_dias(linhas(&v.dias));
            c.set_semanas(linhas(&v.semanas));
            c.set_tasks(linhas(&v.tasks));
            c.set_trunc_modelos(trunc(&v.modelos));
            c.set_trunc_dias(trunc(&v.dias));
            c.set_trunc_semanas(trunc(&v.semanas));
            c.set_trunc_tasks(trunc(&v.tasks));
            c.set_avisos(ModelRc::from(Rc::new(VecModel::from(
                v.avisos.iter().map(|a| SharedString::from(a.as_str())).collect::<Vec<_>>(),
            ))));
            let mut rodape = Vec::new();
            if !v.gerado_em.is_empty() {
                rodape.push(format!("{} {}", tc("updated"), v.gerado_em));
            }
            if !v.pricing_versao.is_empty() {
                rodape.push(format!("{} {}", tc("pricing"), v.pricing_versao));
            }
            if !cu.avisos.is_empty() {
                rodape.push(format!("{} {}", cu.avisos.len(), tc("warnings")));
            }
            c.set_rodape(rodape.join(" · ").into());
            ("ok", String::new())
        }
        Estado::SemArchive if root.is_none() => ("sem-projeto", tc("no_project")),
        Estado::SemArchive => ("sem-archive", tc("no_archive")),
        Estado::SemArquivo => ("sem-arquivo", String::new()),
        Estado::Ilegivel(det) => {
            eprintln!("custos: custos.json ilegível: {det}");
            ("ilegivel", tc("unreadable"))
        }
    };
    c.set_estado(estado.into());
    // Estado sem texto próprio (ok, sem-arquivo) preserva a mensagem de «Sincronizar».
    if !msg.is_empty() {
        c.set_msg(msg.into());
    }
}

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    // Entrou na tela (ou trocou de projeto): relê o arquivo.
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        app.global::<Cu>().on_enter(move || {
            let Some(app) = weak.upgrade() else { return };
            let root = cur.borrow().clone();
            recarregar(&app, root.as_deref(), false);
        });
    }
    // Período: só repinta (o arquivo é pequeno e já está em disco).
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        app.global::<Cu>().on_set_periodo(move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.global::<Cu>().set_periodo(i);
            let root = cur.borrow().clone();
            recarregar(&app, root.as_deref(), true);
        });
    }
    // Sincronizar: o CLI roda em thread (rede/disco não travam a janela).
    {
        let weak = app.as_weak();
        let cur = cx.od_current.clone();
        app.global::<Cu>().on_sincronizar(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(root) = cur.borrow().clone() else { return };
            let c = app.global::<Cu>();
            if c.get_busy() {
                return;
            }
            c.set_busy(true);
            c.set_msg(SharedString::new());
            let w = app.as_weak();
            std::thread::spawn(move || {
                let r = crate::custoscli::sincronizar(&root);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = w.upgrade() else { return };
                    let c = app.global::<Cu>();
                    c.set_busy(false);
                    recarregar(&app, Some(&root), true);
                    match r {
                        Ok(()) => {
                            c.set_msg_erro(false);
                            c.set_msg(tc("sync_ok").into());
                        }
                        Err(e) => {
                            c.set_msg_erro(true);
                            c.set_msg(format!("{}. {}", tc("sync_fail"), e.texto()).into());
                        }
                    }
                });
            });
        });
    }
}
