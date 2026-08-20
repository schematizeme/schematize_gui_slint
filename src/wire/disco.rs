//! Fiação da tela Disco: varrer, filtrar, paginar e apagar o lixo recriável.
//!
//! Três decisões deste recorte, porque são o que o torna seguro e usável:
//!
//!  1. **Nada roda sozinho.** Medir GB de árvore de diretórios é caro (é `stat` em
//!     centenas de milhares de arquivos); a tela abre pedindo "Varrer". Comparar
//!     com a tela Git, que abre já preenchida porque o resumo dela é barato.
//!  2. **A varredura e as remoções vivem em THREAD.** `inventario` e um
//!     `remove_dir_all` de 40 GB bloqueariam o event loop por segundos. O estado
//!     compartilhado é `Arc<Mutex<…>>` (não `Rc`) exatamente por atravessar a
//!     fronteira; os modelos voltam do global via [`set_rows`].
//!  3. **A trava de segurança NÃO está aqui.** Quem recusa apagar fora dos
//!     diretórios de dev cadastrados é `disco::remover`, no lib, com teste. Esta
//!     camada só confirma com o usuário — uma UI não é lugar de guardar invariante.

use crate::prelude::*;
use crate::wire::{set_rows, trava, Ctx};
use schematize::disco::{self, docker, tamanho::legivel, Achado};

/// O que está esperando confirmação.
enum Pendente {
    /// Apagar o achado nesta posição da lista completa.
    Achado(usize),
    /// Rodar esta poda do docker (rótulo de `docker::podas`).
    Poda(String),
}

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // Dono da lista COMPLETA (a UI só vê a página) e dos índices filtrados.
    let achados: Arc<Mutex<Vec<Achado>>> = Arc::new(Mutex::new(Vec::new()));
    let idxs: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let pendente: Rc<RefCell<Option<Pendente>>> = Rc::new(RefCell::new(None));

    let d = app.global::<Dsk>();
    d.set_mounts(ModelRc::from(Rc::new(VecModel::<DiscoTotal>::from(Vec::new()))));
    d.set_kinds(ModelRc::from(Rc::new(VecModel::<DiscoTotal>::from(Vec::new()))));
    d.set_rows(ModelRc::from(Rc::new(VecModel::<DiscoRow>::from(Vec::new()))));
    d.set_docker(ModelRc::from(Rc::new(VecModel::<DockerRow>::from(Vec::new()))));
    d.set_per_page(discorows::PER_PAGE as i32);

    // ---- varrer (thread) ------------------------------------------------
    {
        let weak = app.as_weak();
        let (a, i) = (achados.clone(), idxs.clone());
        app.global::<Dsk>().on_scan(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<Dsk>().get_scanning() {
                return;
            }
            app.global::<Dsk>().set_scanning(true);
            app.global::<Dsk>().set_banner(SharedString::new());
            let devs = config::dev_dirs();
            let weak2 = weak.clone();
            let (a, i) = (a.clone(), i.clone());
            std::thread::spawn(move || {
                let v = disco::inventario(&devs, discorows::MINIMO);
                let dk = discorows::docker_rows();
                let tem_docker = docker::disponivel();
                *trava(&a) = v;
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak2.upgrade() else { return };
                    set_rows(&app.global::<Dsk>().get_docker(), dk);
                    app.global::<Dsk>().set_docker_available(tem_docker);
                    app.global::<Dsk>().set_scanning(false);
                    app.global::<Dsk>().set_scanned(true);
                    app.global::<Dsk>().set_page(0);
                    reaplica(&app, &a, &i);
                });
            });
        });
    }

    // ---- filtro de tempo parado + paginação ------------------------------
    {
        let weak = app.as_weak();
        let (a, i) = (achados.clone(), idxs.clone());
        app.global::<Dsk>().on_set_min_days(move |n| {
            let Some(app) = weak.upgrade() else { return };
            app.global::<Dsk>().set_min_days(n);
            app.global::<Dsk>().set_page(0);
            reaplica(&app, &a, &i);
        });
    }
    {
        let weak = app.as_weak();
        let (a, i) = (achados.clone(), idxs.clone());
        app.global::<Dsk>().on_page_prev(move || {
            let Some(app) = weak.upgrade() else { return };
            let p = (app.global::<Dsk>().get_page() - 1).max(0);
            app.global::<Dsk>().set_page(p);
            set_rows(&app.global::<Dsk>().get_rows(), discorows::page_rows(&trava(&a), &trava(&i), p));
        });
    }
    {
        let weak = app.as_weak();
        let (a, i) = (achados.clone(), idxs.clone());
        app.global::<Dsk>().on_page_next(move || {
            let Some(app) = weak.upgrade() else { return };
            let p = app.global::<Dsk>().get_page() + 1;
            if (p as usize) * discorows::PER_PAGE >= trava(&i).len() {
                return;
            }
            app.global::<Dsk>().set_page(p);
            set_rows(&app.global::<Dsk>().get_rows(), discorows::page_rows(&trava(&a), &trava(&i), p));
        });
    }

    // ---- abrir a pasta do achado no gerenciador de arquivos ---------------
    {
        let a = achados.clone();
        app.global::<Dsk>().on_open_folder(move |idx| {
            if let Some(x) = trava(&a).get(idx as usize) {
                open_path_in_files(&x.caminho);
            }
        });
    }

    // ---- pedir confirmação: apagar um achado ------------------------------
    {
        let weak = app.as_weak();
        let a = achados.clone();
        let pend = pendente.clone();
        app.global::<Dsk>().on_remove_request(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let lista = trava(&a);
            let Some(x) = lista.get(idx as usize) else { return };
            // A confirmação nunca é genérica: diz o caminho, quanto libera e como se refaz.
            app.global::<Dsk>().set_confirm_msg(
                format!(
                    "{} {}?\n\n{} · {} · {}: {}",
                    tor("gui.disk_confirm_delete", "Apagar"),
                    x.caminho.display(),
                    legivel(x.bytes),
                    discorows::dias_label(x.dias_parado),
                    tor("gui.disk_refaz", "refaz com"),
                    x.refaz,
                )
                .into(),
            );
            app.global::<Dsk>().set_confirm_destructive(false);
            app.global::<Dsk>().set_confirm_open(true);
            *pend.borrow_mut() = Some(Pendente::Achado(idx as usize));
        });
    }

    // ---- pedir confirmação: poda do docker --------------------------------
    {
        let weak = app.as_weak();
        let pend = pendente.clone();
        app.global::<Dsk>().on_docker_prune_request(move |rotulo| {
            let Some(app) = weak.upgrade() else { return };
            let rotulo = rotulo.to_string();
            let destrutiva = docker::podas()
                .into_iter()
                .find(|(r, _, _)| *r == rotulo)
                .map(|(_, _, d)| d)
                .unwrap_or(false);
            app.global::<Dsk>().set_confirm_msg(
                format!("{} '{}'?", tor("gui.disk_confirm_prune", "Rodar a poda"), rotulo).into(),
            );
            app.global::<Dsk>().set_confirm_destructive(destrutiva);
            app.global::<Dsk>().set_confirm_open(true);
            *pend.borrow_mut() = Some(Pendente::Poda(rotulo));
        });
    }

    // ---- confirmar / cancelar ---------------------------------------------
    {
        let weak = app.as_weak();
        let pend = pendente.clone();
        let (a, i) = (achados.clone(), idxs.clone());
        app.global::<Dsk>().on_confirm_yes(move || {
            let Some(app) = weak.upgrade() else { return };
            app.global::<Dsk>().set_confirm_open(false);
            match pend.borrow_mut().take() {
                Some(Pendente::Achado(idx)) => apaga_achado(&app, idx, &a, &i),
                Some(Pendente::Poda(rotulo)) => poda_docker(&app, rotulo),
                None => {}
            }
        });
    }
    {
        let weak = app.as_weak();
        let pend = pendente.clone();
        app.global::<Dsk>().on_confirm_no(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Dsk>().set_confirm_open(false);
            }
            *pend.borrow_mut() = None;
        });
    }
}

/// Recomputa filtro, totais, página e a linha de resumo a partir da lista completa.
fn reaplica(app: &AppWindow, achados: &Mutex<Vec<Achado>>, idxs: &Mutex<Vec<usize>>) {
    let todos = trava(achados);
    let min = app.global::<Dsk>().get_min_days().max(0) as u64;
    let filtrados = discorows::filtrados(&todos, min);
    // Os totais seguem o MESMO filtro da lista — senão o topo da tela promete GB
    // que a lista de baixo não oferece.
    let visiveis: Vec<Achado> = filtrados.iter().filter_map(|&i| todos.get(i).cloned()).collect();
    let d = app.global::<Dsk>();
    set_rows(&d.get_mounts(), discorows::totais_por_montagem(&visiveis));
    set_rows(&d.get_kinds(), discorows::totais_por_tipo(&visiveis));
    set_rows(&d.get_rows(), discorows::page_rows(&todos, &filtrados, d.get_page()));
    let total: u64 = visiveis.iter().map(|a| a.bytes).sum();
    d.set_total(filtrados.len() as i32);
    d.set_status(format!("{}: {}", tor("gui.disk_total", "recuperável"), legivel(total)).into());
    *trava(idxs) = filtrados;
}

/// Apaga um achado em thread e tira a linha da lista (sem re-varrer o disco inteiro).
fn apaga_achado(app: &AppWindow, idx: usize, achados: &Arc<Mutex<Vec<Achado>>>, idxs: &Arc<Mutex<Vec<usize>>>) {
    let Some(alvo) = trava(achados).get(idx).cloned() else { return };
    app.global::<Dsk>().set_banner(
        format!("{} {}…", tor("gui.disk_deleting", "apagando"), alvo.caminho.display()).into(),
    );
    app.global::<Dsk>().set_banner_error(false);
    let devs = config::dev_dirs();
    let weak = app.as_weak();
    let (a, i) = (achados.clone(), idxs.clone());
    std::thread::spawn(move || {
        let res = disco::remover(&alvo, &devs);
        // Some da lista o que já não existe. Re-varrer levaria minutos e a resposta
        // seria a mesma — a única coisa que mudou é este item.
        if res.is_ok() {
            trava(&a).retain(|x| x.caminho != alvo.caminho);
        }
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            match res {
                Ok(bytes) => {
                    app.global::<Dsk>().set_banner(
                        format!("{} {}", tor("gui.disk_freed", "liberado"), legivel(bytes)).into(),
                    );
                    app.global::<Dsk>().set_banner_error(false);
                    reaplica(&app, &a, &i);
                }
                Err(e) => {
                    app.global::<Dsk>().set_banner(e.into());
                    app.global::<Dsk>().set_banner_error(true);
                }
            }
        });
    });
}

/// Roda uma poda do docker em thread e recarrega as linhas do `system df`.
fn poda_docker(app: &AppWindow, rotulo: String) {
    app.global::<Dsk>()
        .set_banner(format!("{} '{rotulo}'…", tor("gui.disk_pruning", "podando")).into());
    app.global::<Dsk>().set_banner_error(false);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let res = docker::podar(&rotulo);
        // Depois da poda os números do `system df` mudaram — recarrega.
        let novas = discorows::docker_rows();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            set_rows(&app.global::<Dsk>().get_docker(), novas);
            match res {
                // A saída do docker traz o "Total reclaimed space" — é a resposta útil,
                // e é dele, não nossa: mostramos a última linha não-vazia.
                Ok(saida) => {
                    let resumo = saida.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ok");
                    app.global::<Dsk>().set_banner(resumo.trim().into());
                    app.global::<Dsk>().set_banner_error(false);
                }
                Err(e) => {
                    app.global::<Dsk>().set_banner(e.into());
                    app.global::<Dsk>().set_banner_error(true);
                }
            }
        });
    });
}
