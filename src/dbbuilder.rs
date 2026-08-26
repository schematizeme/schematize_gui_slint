//! Database builder: linhas de tabela/coluna/FK/índice e o grafo do schema
//! (mesma engine de física do grafo do índice, em estado dedicado).

use crate::prelude::*;

/// Constrói as linhas do modelo de tabelas (colunas/FKs/índices aninhados) a partir
/// de um `database::Schema`. Roda no event loop (cria VecModels novos por tabela).
pub(crate) fn build_db_table_rows(schema: &database::Schema) -> Vec<DbTableRow> {
    schema
        .tables
        .iter()
        .map(|t| {
            let cols: Vec<DbColumn> = t
                .columns
                .iter()
                .map(|c| DbColumn {
                    name: c.name.clone().into(),
                    ty: c.ty.clone().into(),
                    nullable: c.nullable,
                    pk: c.pk,
                    unique: c.unique,
                })
                .collect();
            let fks: Vec<DbFkRow> = t
                .fks
                .iter()
                .map(|f| DbFkRow {
                    column: f.column.clone().into(),
                    ref_table: f.ref_table.clone().into(),
                    ref_column: f.ref_column.clone().into(),
                })
                .collect();
            let idxs: Vec<DbIndexRow> = t
                .indexes
                .iter()
                .map(|ix| DbIndexRow {
                    name: ix.name.clone().into(),
                    columns_text: ix.columns.join(", ").into(),
                    unique: ix.unique,
                })
                .collect();
            DbTableRow {
                name: t.name.clone().into(),
                columns: ModelRc::from(Rc::new(VecModel::from(cols))),
                fks: ModelRc::from(Rc::new(VecModel::from(fks))),
                indexes: ModelRc::from(Rc::new(VecModel::from(idxs))),
            }
        })
        .collect()
}

/// Reflete o `database::Schema` na UI: modelo de tabelas + nomes (dropdown) + flag
/// has-schema. Mantém a tabela alvo do editor se ainda existir; senão pega a 1ª.
pub(crate) fn db_rebuild(app: &AppWindow, schema: &database::Schema) {
    app.global::<Db>().set_tables(ModelRc::from(Rc::new(VecModel::from(build_db_table_rows(schema)))));
    let names: Vec<String> = schema.tables.iter().map(|t| t.name.clone()).collect();
    app.global::<Db>().set_table_names(strings_model(names.clone()));
    app.global::<Db>().set_has_schema(!schema.tables.is_empty());
    let sel = app.global::<Db>().get_sel_table().to_string();
    if !names.iter().any(|n| n == &sel) {
        app.global::<Db>().set_sel_table(names.first().cloned().unwrap_or_default().into());
    }
}

/// Carrega um grafo já pronto (nós/arestas/descrições vindos de `database::to_graph`
/// + `table_descriptions`) no estado — sem projeto no disco.
///
/// Espelha o arranjo em espiral + graus + fit pendente do `load_graph_into`.
pub(crate) fn load_db_graph_into(
    st: &mut GraphState,
    nodes: Vec<panel::Node>,
    edges: Vec<panel::Edge>,
    descs: HashMap<String, String>,
) {
    st.nodes.clear();
    st.edges.clear();
    st.descs = descs;
    st.sel = None;
    st.search.clear();
    st.drag_node = None;
    st.moved = false;
    st.scale = 1.0;
    st.ox = 0.0;
    st.oy = 0.0;
    st.alpha = 1.0;
    st.project = None;
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.clone(), i);
        let (sx, sy) = spiral::position(i); // semente espalhada (ver spiral.rs)
        st.nodes.push(GNode {
            id: n.id.clone(),
            loc: n.loc.clone(),
            label: trunc_label(&n.id),
            x: sx,
            y: sy,
            vx: 0.0,
            vy: 0.0,
            deg: 0.0,
            selected: false,
            hot: false,
            dim: false,
        });
    }
    for e in &edges {
        if let (Some(&a), Some(&b)) = (id_to_idx.get(&e.from), id_to_idx.get(&e.to)) {
            st.edges.push((a, b));
            st.nodes[a].deg += 1.0;
            st.nodes[b].deg += 1.0;
        }
    }
    st.fit_pending = true;
    st.refresh_flags();
}

/// Como `graph_sync`, mas escreve as propriedades `db-g-*` do grafo do SCHEMA (sem
/// arquivo:linha — tabela não tem local no código; só nome + descrição das colunas).
pub(crate) fn db_graph_sync(app: &AppWindow, st: &GraphState, nodes: &VecModel<GraphNode>, edges: &VecModel<GraphEdge>) {
    app.global::<Db>().set_g_scale(st.scale);
    app.global::<Db>().set_g_ox(st.ox);
    app.global::<Db>().set_g_oy(st.oy);
    app.global::<Db>().set_g_has_graph(!st.nodes.is_empty());
    app.global::<Db>().set_g_node_count(st.nodes.len() as i32);
    match st.sel {
        Some(i) => {
            app.global::<Db>().set_g_has_sel(true);
            app.global::<Db>().set_g_sel_id(st.nodes[i].id.clone().into());
            let desc = st.descs.get(&st.nodes[i].id).cloned().unwrap_or_default();
            app.global::<Db>().set_g_sel_desc(desc.into());
        }
        None => {
            app.global::<Db>().set_g_has_sel(false);
            app.global::<Db>().set_g_sel_id(SharedString::new());
            app.global::<Db>().set_g_sel_desc(SharedString::new());
        }
    }
    if nodes.row_count() == st.nodes.len() {
        for (i, n) in st.nodes.iter().enumerate() {
            nodes.set_row_data(i, graph_node_row(n));
        }
    } else {
        nodes.set_vec(st.nodes.iter().map(graph_node_row).collect::<Vec<_>>());
    }
    if edges.row_count() == st.edges.len() {
        for (i, &(a, b)) in st.edges.iter().enumerate() {
            edges.set_row_data(i, graph_edge_row(st, a, b));
        }
    } else {
        edges.set_vec(st.edges.iter().map(|&(a, b)| graph_edge_row(st, a, b)).collect::<Vec<_>>());
    }
}

/// (Re)liga o Timer da física do grafo do SCHEMA (relaxa quando alpha < 0.02).
pub(crate) fn db_graph_kick(
    timer: &Rc<slint::Timer>,
    weak: Weak<AppWindow>,
    state: Rc<RefCell<GraphState>>,
    nodes: Rc<VecModel<GraphNode>>,
    edges: Rc<VecModel<GraphEdge>>,
) {
    if timer.running() {
        return;
    }
    // Mesma trava do grafo do índice: física só com a tela do Database visível.
    if weak.upgrade().map(|a| a.get_screen()) != Some(DB_SCREEN) {
        return;
    }
    let timer2 = timer.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(app) = weak.upgrade() else {
            timer2.stop();
            return;
        };
        if app.get_screen() != DB_SCREEN {
            timer2.stop();
            return;
        }
        let mut st = state.borrow_mut();
        st.step();
        db_graph_sync(&app, &st, &nodes, &edges);
        if st.alpha < 0.02 {
            timer2.stop();
        }
    });
}

/// Monta o prompt em LINGUAGEM NATURAL do "gerar por descrição (IA)": pede pra seguir
/// a disciplina de modelagem da casa (schematize-database) e emitir schema.json +
/// schema.sql + migration em `<projeto>_archive/database/`, a partir da descrição do
/// usuário. NÃO usa o slash `/database-design` (não roda como arg do claude).
pub(crate) fn db_ai_prompt(project_basename: &str, desc: &str) -> String {
    format!(
        "Modele o banco de dados deste projeto a partir da descrição de domínio no fim desta mensagem, \
         usando a DISCIPLINA DE MODELAGEM DA CASA (a skill schematize-database): normalização 1FN–3FN, \
         PK surrogate ULID/UUIDv7 interna + a chave natural como UNIQUE (identidade ≠ email), tipos corretos \
         por coluna (dinheiro em inteiro/numeric, tempo em timestamptz UTC, enum como domínio), constraints \
         conscientes (NOT NULL/default, UNIQUE, CHECK, FOREIGN KEY com ON DELETE consciente), índices \
         conscientes (sem redundância; PII nunca vira chave de índice) e o piso de privacidade (coluna PII \
         marcada, base legal + retenção — LGPD). \
         EMITA os artefatos na pasta `{proj}_archive/database/` (crie se não existir): \
         (1) `schema.json` no formato do database builder — um objeto JSON com `tables` (array), cada tabela \
         com `name`, `columns` (cada uma {{name, ty, nullable, pk, unique}}), `fks` (cada uma \
         {{column, ref_table, ref_column}}) e `indexes` (cada um {{name, columns[], unique}}); \
         (2) `schema.sql` com os CREATE TABLE; (3) `migration.sql` no estilo expand-contract reversível. \
         Não pare enquanto os três arquivos não estiverem gravados e consistentes.\n\n\
         Descrição do domínio:\n{desc}",
        proj = project_basename,
        desc = desc,
    )
}
