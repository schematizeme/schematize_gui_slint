//! Fiação da aba Grafo: pan/zoom/arrasto/clique, busca, drill-down por serviço,
//! reindexar e recarregar.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let graph_state = cx.graph_state.clone();
    let graph_nodes = cx.graph_nodes.clone();
    let graph_edges = cx.graph_edges.clone();
    let graph_timer = cx.graph_timer.clone();
    let graph_loaded = cx.graph_loaded.clone();
    // ==================== aba Grafo — interação ====================
    // Ponteiro/roda chegam crus do Slint (coords relativas ao canvas, em px). O
    // hit-test e a decisão pan-vs-arrasto acontecem AQUI (como no egui). Cada
    // handler sincroniza o modelo/props no fim.

    // canvas mudou de tamanho → guarda; se havia fit pendente (carga), enquadra.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_canvas_resized(move |w, h| {
            let mut st = gs.borrow_mut();
            st.canvas_w = w;
            st.canvas_h = h;
            if st.fit_pending && w > 1.0 && h > 1.0 {
                st.fit();
                st.fit_pending = false;
            }
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // mouse-down: hit-test → fixa o nó a arrastar (com offset de pega) ou nada.
    {
        let gs = graph_state.clone();
        app.global::<G>().on_press(move |mx, my| {
            let mut st = gs.borrow_mut();
            let (wx, wy) = st.to_world(mx, my);
            st.last_ptr = (mx, my);
            st.moved = false;
            match st.hit(wx, wy) {
                Some(i) => {
                    st.drag_node = Some(i);
                    st.drag_off = (st.nodes[i].x - wx, st.nodes[i].y - wy);
                }
                None => st.drag_node = None,
            }
        });
    }
    // arrasto: nó fixo → move o nó (reaquece a física); senão → pan do fundo.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        let gt = graph_timer.clone();
        app.global::<G>().on_move(move |mx, my| {
            let need_kick;
            {
                let mut st = gs.borrow_mut();
                let (dx, dy) = (mx - st.last_ptr.0, my - st.last_ptr.1);
                if dx.abs() + dy.abs() > 3.0 {
                    st.moved = true;
                }
                match st.drag_node {
                    Some(i) => {
                        let (wx, wy) = st.to_world(mx, my);
                        st.nodes[i].x = wx + st.drag_off.0;
                        st.nodes[i].y = wy + st.drag_off.1;
                        st.nodes[i].vx = 0.0;
                        st.nodes[i].vy = 0.0;
                        if st.alpha < 0.3 {
                            st.alpha = 0.3;
                        }
                        need_kick = true;
                    }
                    None => {
                        st.ox += dx;
                        st.oy += dy;
                        need_kick = false;
                    }
                }
                st.last_ptr = (mx, my);
                if let Some(app) = weak.upgrade() {
                    graph_sync(&app, &st, &gn, &ge);
                }
            }
            if need_kick {
                graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
            }
        });
    }
    // mouse-up: se não houve arrasto, é um CLIQUE → seleciona/deseleciona o nó.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_release(move || {
            let mut st = gs.borrow_mut();
            if !st.moved {
                let (wx, wy) = st.to_world(st.last_ptr.0, st.last_ptr.1);
                st.sel = st.hit(wx, wy);
                st.refresh_flags();
            }
            st.drag_node = None;
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // roda: zoom centrado no cursor (mesma matemática do egui).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_scroll(move |mx, my, dy| {
            let mut st = gs.borrow_mut();
            let cx = st.canvas_w / 2.0;
            let cy = st.canvas_h / 2.0;
            let m = (mx - cx - st.ox, my - cy - st.oy);
            let f = (dy * 0.0015).exp();
            st.ox -= m.0 * (f - 1.0);
            st.oy -= m.1 * (f - 1.0);
            st.scale = (st.scale * f).clamp(0.12, 6.0);
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // botão "ajustar": enquadra tudo no canvas.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_fit(move || {
            let mut st = gs.borrow_mut();
            st.fit();
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // busca: realça/apaga nós por nome (recomputa flags e sincroniza).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_search_changed(move |s| {
            let mut st = gs.borrow_mut();
            st.search = s.to_string();
            st.refresh_flags();
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // clique em "abrir no editor" do nó selecionado → vscode://file/<abs>/…:<linha>.
    {
        let gs = graph_state.clone();
        app.global::<G>().on_open_editor(move || {
            let st = gs.borrow();
            if let (Some(i), Some(proj)) = (st.sel, st.project.clone()) {
                if let Some(loc) = st.nodes[i].loc.clone() {
                    let abs = std::fs::canonicalize(&proj).unwrap_or(proj);
                    util::open_url(&format!("vscode://file/{}/{}", abs.to_string_lossy(), loc));
                }
            }
        });
    }
    // exportar vault Obsidian do índice (bônus — via panel::export_obsidian_at).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        app.global::<G>().on_export(move || {
            let proj = gs.borrow().project.clone();
            if let Some(p) = proj {
                match panel::export_obsidian_at(&p, None) {
                    Ok(dir) => {
                        if let Some(app) = weak.upgrade() {
                            app.global::<Sk>().set_status(
                                tf("gui.exported", &[("p", &dir.to_string_lossy())]).into(),
                            );
                        }
                        util::open_url(&dir.to_string_lossy());
                    }
                    Err(e) => {
                        if let Some(app) = weak.upgrade() {
                            app.global::<Sk>().set_status(tf("err.prefix", &[("e", &e)]).into());
                        }
                    }
                }
            }
        });
    }
    // abrir o painel HTML (com o mesmo grafo) no navegador (bônus).
    {
        let gs = graph_state.clone();
        app.global::<G>().on_open_browser(move || {
            let proj = gs.borrow().project.clone();
            if let Some(p) = proj {
                let _ = panel::open_in_browser(&p);
            }
        });
    }
    // "Reindexar" — chama a skill que organiza o grafo: dispara o índice §39 (prompt NL) num
    // TERMINAL EXTERNO (processo próprio do `claude`, fora do app), numa thread. Só dados Send
    // cruzam (PathBuf + String). Sucesso → banner "índice rodando no terminal <nome> — clique em
    // Recarregar quando terminar."; erro → a msg da lib (claude/terminal ausente).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        app.global::<G>().on_reindex(move || {
            let Some(root) = gs.borrow().project.clone() else {
                return;
            };
            let w = weak.clone();
            std::thread::spawn(move || {
                let res = agentrun::launch_prompt_in_terminal(&root, &agentrun::reindex_prompt());
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = w.upgrade() {
                        let msg = match res {
                            Ok(term) => format!(
                                "{}{}{}",
                                tor("gui.g_reindex_pre", "índice rodando no terminal "),
                                term,
                                tor(
                                    "gui.g_reindex_post",
                                    " — clique em Recarregar quando terminar.",
                                ),
                            ),
                            Err(e) => e,
                        };
                        app.global::<G>().set_reindex_status(msg.into());
                    }
                });
            });
        });
    }
    // "Recarregar grafo" — re-roda load_graph + node_descriptions e atualiza a UI (após o índice
    // terminar no terminal). Limpa o banner do reindex.
    {
        let weak = app.as_weak();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        let gl = graph_loaded.clone();
        app.global::<G>().on_reload(move || {
            let proj = gs.borrow().project.clone();
            graph_load_and_kick(proj.as_deref(), &gl, &gt, &weak, &gs, &gn, &ge);
            if let Some(app) = weak.upgrade() {
                app.global::<G>().set_reindex_status(SharedString::new());
            }
        });
    }
    // Drill-down: abre o grafo DETALHADO do microserviço do nó selecionado
    // (`.schematize/grafos/<servico>.md`). Sem grafo detalhado desse serviço → avisa e mantém a visão.
    {
        let weak = app.as_weak();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_drill(move || {
            let (proj, servico) = {
                let st = gs.borrow();
                let servico = st.sel.map(|i| st.nodes[i].id.clone());
                (st.project.clone(), servico)
            };
            let Some(servico) = servico else { return };
            let carregou = load_service_into(&mut gs.borrow_mut(), proj.as_deref(), &servico);
            if let Some(app) = weak.upgrade() {
                if carregou {
                    graph_sync(&app, &gs.borrow(), &gn, &ge);
                    graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
                    app.global::<G>().set_reindex_status(SharedString::new());
                } else {
                    app.global::<G>().set_reindex_status(
                        tor(
                            "gui.g_no_service_graph",
                            "sem grafo detalhado para este serviço (rode Reindexar).",
                        )
                        .into(),
                    );
                }
            }
        });
    }
    // Volta da visão por-serviço para o GRAFO GLOBAL da aplicação.
    {
        let weak = app.as_weak();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        let gl = graph_loaded.clone();
        app.global::<G>().on_global(move || {
            let proj = gs.borrow().project.clone();
            graph_load_and_kick(proj.as_deref(), &gl, &gt, &weak, &gs, &gn, &ge);
            if let Some(app) = weak.upgrade() {
                app.global::<G>().set_reindex_status(SharedString::new());
            }
        });
    }
    // "x" do bloco de info → deseleciona o nó (fecha o bloco) e ressincroniza.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_clear_sel(move || {
            let mut st = gs.borrow_mut();
            st.sel = None;
            st.refresh_flags();
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
}
