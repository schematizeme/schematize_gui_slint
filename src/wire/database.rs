//! Fiação do Database builder: introspecção/importação de schema, edição das
//! tabelas e o grafo do schema.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let od_current = cx.od_current.clone();
    // ==================== Database builder (tela 6) ====================
    // Schema canônico compartilhado (Send+Sync → cruza pra a thread do Postgres e é
    // lido pelos callbacks na UI thread). Grafo do schema em estado DEDICADO.
    let db_schema: Arc<Mutex<database::Schema>> = Arc::new(Mutex::new(database::Schema::default()));
    let db_graph_state = Rc::new(RefCell::new(GraphState { scale: 1.0, alpha: 1.0, ..Default::default() }));
    let db_graph_nodes = Rc::new(VecModel::<GraphNode>::from(Vec::new()));
    let db_graph_edges = Rc::new(VecModel::<GraphEdge>::from(Vec::new()));
    let db_graph_timer = Rc::new(slint::Timer::default());
    app.set_db_graph_nodes(ModelRc::from(db_graph_nodes.clone()));
    app.set_db_graph_edges(ModelRc::from(db_graph_edges.clone()));
    db_rebuild(&app, &db_schema.lock().unwrap());

    // escolher arquivo SQLite (picker nativo).
    {
        let weak = app.as_weak();
        app.on_db_pick_sqlite(move || {
            if let Some(path) = rfd::FileDialog::new().set_title(t("gui.open_folder")).pick_file() {
                if let Some(app) = weak.upgrade() {
                    app.set_db_sqlite_path(path.to_string_lossy().into_owned().into());
                }
            }
        });
    }
    // introspectar SQLite — LOCAL e rápido (arquivo); roda síncrono e mutaciona o
    // schema direto (como env status / ssh). Erro claro se o arquivo não abrir.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_introspect_sqlite(move || {
            let Some(app) = weak.upgrade() else { return };
            let path = app.get_db_sqlite_path().to_string();
            if path.is_empty() {
                return;
            }
            app.set_db_error(SharedString::new());
            match database::introspect_sqlite(Path::new(&path)) {
                Ok(schema) => {
                    let n = schema.tables.len();
                    *sh.lock().unwrap() = schema;
                    db_rebuild(&app, &sh.lock().unwrap());
                    app.set_db_status(format!("{} — {} tabela(s)", tor("gui.db_loaded", "Schema carregado"), n).into());
                    app.set_db_view(1);
                }
                Err(e) => app.set_db_error(e.into()),
            }
        });
    }
    // introspectar Postgres — usa `psql` (subprocesso, pode bloquear) → THREAD. O
    // Schema (Send) volta e é gravado no lock DENTRO do event loop; a UI é remontada.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_introspect_postgres(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_db_busy() {
                return;
            }
            let conn = app.get_db_pg_conn().to_string();
            if conn.trim().is_empty() {
                return;
            }
            app.set_db_busy(true);
            app.set_db_error(SharedString::new());
            app.set_db_status(SharedString::new());
            let weak = weak.clone();
            let sh = sh.clone();
            std::thread::spawn(move || {
                let res = database::introspect_postgres(&conn);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_db_busy(false);
                        match res {
                            Ok(schema) => {
                                let n = schema.tables.len();
                                *sh.lock().unwrap() = schema;
                                db_rebuild(&app, &sh.lock().unwrap());
                                app.set_db_status(format!("{} — {} tabela(s)", tor("gui.db_loaded", "Schema carregado"), n).into());
                                app.set_db_view(1);
                            }
                            Err(e) => app.set_db_error(e.into()),
                        }
                    }
                });
            });
        });
    }
    // carregar schema.json (picker → serde). Erro claro se o JSON não casar.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_load_json(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("schema", &["json"]).pick_file() else {
                return;
            };
            app.set_db_error(SharedString::new());
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<database::Schema>(&s).map_err(|e| e.to_string()))
            {
                Ok(schema) => {
                    let n = schema.tables.len();
                    *sh.lock().unwrap() = schema;
                    db_rebuild(&app, &sh.lock().unwrap());
                    app.set_db_status(format!("{} — {} tabela(s)", tor("gui.db_loaded", "Schema carregado"), n).into());
                    app.set_db_view(1);
                }
                Err(e) => app.set_db_error(tf("err.prefix", &[("e", &e)]).into()),
            }
        });
    }
    // salvar schema.json (picker → serde pretty).
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_save_json(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new()
                .add_filter("schema", &["json"])
                .set_file_name("schema.json")
                .save_file()
            else {
                return;
            };
            let json = serde_json::to_string_pretty(&*sh.lock().unwrap()).unwrap_or_default();
            match std::fs::write(&path, json) {
                Ok(()) => app.set_db_status(
                    format!("{} {}", tor("gui.db_saved", "schema.json salvo em"), path.display()).into(),
                ),
                Err(e) => app.set_db_error(tf("err.prefix", &[("e", &e.to_string())]).into()),
            }
        });
    }
    // escolher a tabela alvo do editor (coluna/FK).
    {
        let weak = app.as_weak();
        app.on_db_pick_table(move |name| {
            if let Some(app) = weak.upgrade() {
                app.set_db_sel_table(name);
            }
        });
    }
    // adicionar uma tabela vazia.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_add_table(move || {
            let Some(app) = weak.upgrade() else { return };
            let name = app.get_db_new_table().to_string();
            if name.trim().is_empty() {
                return;
            }
            {
                let mut s = sh.lock().unwrap();
                if s.tables.iter().any(|t| t.name == name) {
                    app.set_db_error(tor("gui.db_table_exists", "já existe uma tabela com esse nome").into());
                    return;
                }
                s.tables.push(database::Table { name: name.clone(), ..Default::default() });
            }
            app.set_db_error(SharedString::new());
            app.set_db_new_table(SharedString::new());
            app.set_db_sel_table(name.into());
            db_rebuild(&app, &sh.lock().unwrap());
        });
    }
    // adicionar uma coluna à tabela alvo.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_add_column(move || {
            let Some(app) = weak.upgrade() else { return };
            let table = app.get_db_sel_table().to_string();
            let name = app.get_db_new_col().to_string();
            if table.is_empty() || name.trim().is_empty() {
                return;
            }
            let ty = app.get_db_new_col_type().to_string();
            let col = database::Column {
                name: name.clone(),
                ty: if ty.trim().is_empty() { "TEXT".into() } else { ty },
                nullable: app.get_db_new_col_nullable(),
                pk: app.get_db_new_col_pk(),
                unique: app.get_db_new_col_unique(),
            };
            {
                let mut s = sh.lock().unwrap();
                if let Some(t) = s.tables.iter_mut().find(|t| t.name == table) {
                    t.columns.push(col);
                }
            }
            app.set_db_new_col(SharedString::new());
            db_rebuild(&app, &sh.lock().unwrap());
        });
    }
    // adicionar uma FK à tabela alvo.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_add_fk(move || {
            let Some(app) = weak.upgrade() else { return };
            let table = app.get_db_sel_table().to_string();
            let column = app.get_db_fk_col().to_string();
            let ref_table = app.get_db_fk_reftable().to_string();
            let ref_column = app.get_db_fk_refcol().to_string();
            if table.is_empty() || column.trim().is_empty() || ref_table.trim().is_empty() {
                return;
            }
            {
                let mut s = sh.lock().unwrap();
                if let Some(t) = s.tables.iter_mut().find(|t| t.name == table) {
                    t.fks.push(database::Fk {
                        column,
                        ref_table,
                        ref_column: if ref_column.trim().is_empty() { "id".into() } else { ref_column },
                    });
                }
            }
            app.set_db_fk_col(SharedString::new());
            app.set_db_fk_reftable(SharedString::new());
            app.set_db_fk_refcol(SharedString::new());
            db_rebuild(&app, &sh.lock().unwrap());
        });
    }
    // gerar SQL (to_sql) → visor read-only.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_gen_sql(move || {
            if let Some(app) = weak.upgrade() {
                app.set_db_gen_title(tor("gui.db_gen_sql", "Gerar SQL").into());
                app.set_db_gen_content(database::to_sql(&sh.lock().unwrap()).into());
                app.set_db_gen_open(true);
            }
        });
    }
    // gerar migration (to_migration) → visor read-only.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_gen_migration(move || {
            if let Some(app) = weak.upgrade() {
                app.set_db_gen_title(tor("gui.db_gen_migration", "Gerar migration").into());
                app.set_db_gen_content(database::to_migration(&sh.lock().unwrap()).into());
                app.set_db_gen_open(true);
            }
        });
    }
    // salvar o conteúdo do visor num arquivo (picker nativo).
    {
        let weak = app.as_weak();
        app.on_db_gen_save(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("sql", &["sql"]).set_file_name("schema.sql").save_file()
            else {
                return;
            };
            let _ = std::fs::write(&path, app.get_db_gen_content().to_string());
        });
    }
    // (re)construir o grafo do schema atual (tabela=nó, FK=aresta) e ligar a física.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        let gt = db_graph_timer.clone();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_show_graph(move || {
            let (nodes, edges, descs) = {
                let s = sh.lock().unwrap();
                let (n, e) = database::to_graph(&s);
                (n, e, database::table_descriptions(&s))
            };
            load_db_graph_into(&mut gs.borrow_mut(), nodes, edges, descs);
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &gs.borrow(), &gn, &ge);
            }
            db_graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
        });
    }
    // gerar por descrição (IA): dispara a skill schematize-database num TERMINAL
    // EXTERNO (processo próprio do claude, fora do app). Usa o projeto atual (od_current).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_db_ai_generate(move || {
            let Some(app) = weak.upgrade() else { return };
            let desc = app.get_db_ai_desc().to_string();
            if desc.trim().is_empty() {
                return;
            }
            let Some(root) = cur.borrow().clone() else {
                app.set_db_ai_status(tor("gui.db_ai_no_project", "Selecione um projeto na tela Overdev/Grafo primeiro.").into());
                return;
            };
            let base = basename_of(&root);
            let prompt = db_ai_prompt(&base, &desc);
            let w = weak.clone();
            std::thread::spawn(move || {
                let res = agentrun::launch_prompt_in_terminal(&root, &prompt);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = w.upgrade() {
                        let msg = match res {
                            Ok(term) => format!(
                                "{}{}{}",
                                tor("gui.db_ai_running_pre", "schematize-database rodando no terminal "),
                                term,
                                tor("gui.db_ai_running_post", " — carregue o schema.json quando terminar."),
                            ),
                            Err(e) => e,
                        };
                        app.set_db_ai_status(msg.into());
                    }
                });
            });
        });
    }
    // ---- grafo do schema: interação (pan/zoom/arrasto/clique) — estado dedicado ----
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_canvas_resized(move |w, h| {
            let mut st = gs.borrow_mut();
            st.canvas_w = w;
            st.canvas_h = h;
            if st.fit_pending && w > 1.0 && h > 1.0 {
                st.fit();
                st.fit_pending = false;
            }
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let gs = db_graph_state.clone();
        app.on_db_graph_press(move |mx, my| {
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
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        let gt = db_graph_timer.clone();
        app.on_db_graph_move(move |mx, my| {
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
                    db_graph_sync(&app, &st, &gn, &ge);
                }
            }
            if need_kick {
                db_graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_release(move || {
            let mut st = gs.borrow_mut();
            if !st.moved {
                let (wx, wy) = st.to_world(st.last_ptr.0, st.last_ptr.1);
                st.sel = st.hit(wx, wy);
                st.refresh_flags();
            }
            st.drag_node = None;
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_scroll(move |mx, my, dy| {
            let mut st = gs.borrow_mut();
            let cx = st.canvas_w / 2.0;
            let cy = st.canvas_h / 2.0;
            let m = (mx - cx - st.ox, my - cy - st.oy);
            let f = (dy * 0.0015).exp();
            st.ox -= m.0 * (f - 1.0);
            st.oy -= m.1 * (f - 1.0);
            st.scale = (st.scale * f).clamp(0.12, 6.0);
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_fit(move || {
            let mut st = gs.borrow_mut();
            st.fit();
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_clear_sel(move || {
            let mut st = gs.borrow_mut();
            st.sel = None;
            st.refresh_flags();
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }

}
