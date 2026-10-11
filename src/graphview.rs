//! Ponte do grafo com a UI: converte o estado em linhas de modelo, sincroniza,
//! carrega o grafo do projeto (preguiçosamente) e liga/desliga o timer da física.

use crate::prelude::*;

/// Constrói uma linha do modelo de nós a partir do estado de simulação.
pub(crate) fn graph_node_row(n: &GNode) -> GraphNode {
    GraphNode {
        id: n.id.clone().into(),
        label: n.label.clone().into(),
        x: n.x,
        y: n.y,
        r: nsize(n.deg),
        selected: n.selected,
        hot: n.hot,
        dim: n.dim,
        has_loc: n.loc.is_some(),
    }
}

/// Constrói uma linha do modelo de arestas (pontas em mundo + realce).
pub(crate) fn graph_edge_row(st: &GraphState, a: usize, b: usize) -> GraphEdge {
    let on = st.sel == Some(a) || st.sel == Some(b);
    GraphEdge { x1: st.nodes[a].x, y1: st.nodes[a].y, x2: st.nodes[b].x, y2: st.nodes[b].y, on }
}

/// Empurra TUDO pro Slint: transformação (props), seleção (info) e os dois
/// VecModel (nós/arestas). Atualiza in-place quando o tamanho casa (sem realloc
/// no loop da física); senão troca o vec inteiro (carga/relayout).
pub(crate) fn graph_sync(
    app: &AppWindow,
    st: &GraphState,
    nodes: &VecModel<GraphNode>,
    edges: &VecModel<GraphEdge>,
) {
    app.global::<G>().set_scale(st.scale);
    app.global::<G>().set_ox(st.ox);
    app.global::<G>().set_oy(st.oy);
    app.global::<G>().set_has_graph(!st.nodes.is_empty());
    app.global::<G>().set_node_count(st.nodes.len() as i32);
    // Drill-down: `service` Some = vendo o grafo detalhado daquele microserviço.
    app.global::<G>().set_in_service(st.service.is_some());
    app.global::<G>().set_service_name(st.service.clone().unwrap_or_default().into());
    match st.sel {
        Some(i) => {
            app.global::<G>().set_has_sel(true);
            app.global::<G>().set_sel_id(st.nodes[i].id.clone().into());
            app.global::<G>().set_sel_loc(st.nodes[i].loc.clone().unwrap_or_default().into());
            // descrição do nó selecionado (por nome). "" → o Slint mostra a dica de reindexar.
            let desc = st.descs.get(&st.nodes[i].id).cloned().unwrap_or_default();
            app.global::<G>().set_sel_desc(desc.into());
        }
        None => {
            app.global::<G>().set_has_sel(false);
            app.global::<G>().set_sel_id(SharedString::new());
            app.global::<G>().set_sel_loc(SharedString::new());
            app.global::<G>().set_sel_desc(SharedString::new());
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

/// Zera o estado do grafo (seleção, busca, arrasto, zoom, física) para uma carga nova.
/// **Onde:** [`load_graph_into`] e [`load_service_into`] — o mesmo reinício para os dois.
fn reiniciar(st: &mut GraphState, proj: Option<&Path>, servico: Option<&str>) {
    st.nodes.clear();
    st.edges.clear();
    st.descs.clear();
    st.sel = None;
    st.search.clear();
    st.drag_node = None;
    st.moved = false;
    st.scale = 1.0;
    st.ox = 0.0;
    st.oy = 0.0;
    st.alpha = 1.0;
    st.service = servico.map(str::to_string); // None = visão GLOBAL (drill-down sai)
    st.project = proj.map(|p| p.to_path_buf());
}

/// Povoa o estado com os nós e arestas lidos do app: posição em espiral inicial (semente
/// espalhada, ver `spiral.rs`), graus e fit pendente. **Onde:** as duas cargas.
fn povoar(st: &mut GraphState, g: crate::overdevcli::Grafo) {
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, n) in g.nos.into_iter().enumerate() {
        id_to_idx.insert(n.id.clone(), i);
        let (sx, sy) = spiral::position(i);
        st.nodes.push(GNode {
            label: trunc_label(&n.id),
            id: n.id,
            loc: n.loc,
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
    for e in &g.arestas {
        if let (Some(&a), Some(&b)) = (id_to_idx.get(&e.de), id_to_idx.get(&e.para)) {
            st.edges.push((a, b));
            st.nodes[a].deg += 1.0;
            st.nodes[b].deg += 1.0;
        }
    }
    // descrições dos nós (nome -> "O quê") do índice/MAPA, guardadas p/ o bloco lateral.
    st.descs = g.descricoes;
    st.fit_pending = true;
    st.refresh_flags();
}

/// Carrega o grafo do `proj` (ou limpa se None) no estado.
///
/// Grafo global PAGINADO: `GRAFO_GLOBAL.md` autorado; senão, se o flat passar do cap, AGREGA por
/// microserviço (1 nó por serviço) — o "1600 nós ilegíveis" vira mapa mental; o drill ("Grafo do
/// serviço") abre o detalhe via [`load_service_into`]. **Quem lê e parseia o índice é o app**
/// (`schematize-overdev graph json`); o hub só desenha. Se o app não responde, o estado fica
/// LIMPO e o motivo volta no `Err` — a tela diz «instale», não finge um projeto sem grafo.
pub(crate) fn load_graph_into(st: &mut GraphState, proj: Option<&Path>) -> Result<(), String> {
    reiniciar(st, proj, None);
    let Some(p) = proj else {
        return Ok(());
    };
    let g = crate::overdevcli::grafo(p, None)?;
    st.aggregated = g.agregado;
    povoar(st, g);
    Ok(())
}

/// Drill-down: carrega o grafo DETALHADO de UM microserviço (`.schematize/grafos/<servico>.md`)
/// no estado. Devolve `Ok(true)` se achou/carregou (nós > 0); `Ok(false)` se não há grafo
/// detalhado desse serviço — nesse caso o chamador mantém a visão atual e avisa (não zera a tela
/// à toa). `Err` é o app que não respondeu: a visão atual também fica intacta.
pub(crate) fn load_service_into(
    st: &mut GraphState,
    proj: Option<&Path>,
    servico: &str,
) -> Result<bool, String> {
    let Some(p) = proj else {
        return Ok(false);
    };
    let g = crate::overdevcli::grafo(p, Some(servico))?;
    if g.nos.is_empty() {
        return Ok(false);
    }
    reiniciar(st, Some(p), Some(servico));
    povoar(st, g);
    Ok(true)
}

/// (Re)liga o Timer da física se estiver parado. O tick roda um passo, sincroniza,
/// e PARA (relaxa) quando alpha < 0.02 — não queima CPU parado. Reinicia via arrasto/carga.
pub(crate) fn graph_kick(
    timer: &Rc<slint::Timer>,
    weak: Weak<AppWindow>,
    state: Rc<RefCell<GraphState>>,
    nodes: Rc<VecModel<GraphNode>>,
    edges: Rc<VecModel<GraphEdge>>,
) {
    if timer.running() {
        return;
    }
    // A física só gira com a aba Grafo VISÍVEL. As outras telas nem instanciam os
    // nós (o `if root.screen == 3` do .slint), então rodar a 60 fps aqui roubaria
    // event loop de quem está trabalhando no Overdev — era por isso que ABRIR um
    // projeto grande travava o app inteiro, e não só a aba do grafo.
    if weak.upgrade().map(|a| a.get_screen()) != Some(GRAPH_SCREEN) {
        return;
    }
    let timer2 = timer.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(app) = weak.upgrade() else {
            timer2.stop();
            return;
        };
        if app.get_screen() != GRAPH_SCREEN {
            timer2.stop(); // saiu da aba: para na hora, sem esperar relaxar
            return;
        }
        let mut st = state.borrow_mut();
        st.step();
        graph_sync(&app, &st, &nodes, &edges);
        if st.alpha < 0.02 {
            timer2.stop();
        }
    });
}

/// Índice da tela do Grafo (o `screen` do `.slint`: 0 Home · 1 Mercado · 2 Overdev
/// · 3 Grafo). Constante nomeada porque a física depende dela.
///
/// **O `DB_SCREEN` que ficava ao lado saiu com a tela** (E1 M5 da extradição): o grafo do
/// schema era o SEGUNDO grafo desta janela, com a mesma física e estado à parte, e foi embora
/// junto com o resto do banco. O que sobra é este — o do índice do projeto.
pub(crate) const GRAPH_SCREEN: i32 = 3;

/// Marca o grafo como PENDENTE de carga (troca/recarga de projeto).
///
/// A carga de verdade — parsear o índice do archive, que num projeto real passa de
/// 1 MB de markdown, montar nós/arestas e ligar a física — só acontece quando a aba
/// Grafo é REALMENTE aberta (`graph_enter`). Antes isso rodava ao escolher o projeto
/// na aba Overdev, no event loop, para uma tela que o usuário talvez nem abrisse.
pub(crate) fn graph_mark_dirty(loaded: &RefCell<Option<PathBuf>>) {
    *loaded.borrow_mut() = None;
}

/// Instante mais recente entre os arquivos de grafo do projeto (`.schematize/grafos/`).
///
/// O quê: varre o dir (sem recursão) e devolve o maior `mtime`. Onde: [`graph_enter`],
/// pra saber se o disco mudou desde a última leitura.
/// Por que existe: a aba só relia quando o CAMINHO do projeto mudava, então um overdev
/// regenerando o índice do projeto ABERTO não aparecia — a tela ficava mostrando o grafo
/// de minutos antes e a única saída era trocar de projeto e voltar.
/// **Entrada:** raiz do projeto. **Saída:** `None` se não há dir/arquivo legível.
/// **Efeitos:** só `stat` no dir; nunca panica.
fn grafos_mtime(proj: &Path) -> Option<std::time::SystemTime> {
    let dir = proj.join(".schematize").join("grafos");
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| e.metadata().ok()?.modified().ok()).max()
}

/// Entrada na aba Grafo: carrega o grafo do projeto corrente se ainda não estiver
/// carregado OU se os arquivos mudaram no disco, e (re)liga a física.
///
/// A idempotência era por CAMINHO apenas — reentrar na aba nunca relia. Isso mantinha o
/// layout estável (bom), mas escondia toda alteração do próprio projeto (ruim). Agora o
/// gatilho é caminho OU `mtime`: sem mudança no disco, segue idempotente e o grafo não
/// "pula"; com mudança, relê.
#[allow(clippy::too_many_arguments)]
pub(crate) fn graph_enter(
    proj: Option<&Path>,
    loaded: &RefCell<Option<PathBuf>>,
    timer: &Rc<slint::Timer>,
    weak: &Weak<AppWindow>,
    state: &Rc<RefCell<GraphState>>,
    nodes: &Rc<VecModel<GraphNode>>,
    edges: &Rc<VecModel<GraphEdge>>,
) {
    let mtime_agora = proj.and_then(grafos_mtime);
    let mesmo_projeto = loaded.borrow().as_deref() == proj;
    let disco_igual = state.borrow().carregado_em == mtime_agora;
    if !mesmo_projeto || !disco_igual {
        let res = load_graph_into(&mut state.borrow_mut(), proj);
        state.borrow_mut().carregado_em = mtime_agora;
        *loaded.borrow_mut() = proj.map(|p| p.to_path_buf());
        if let Some(app) = weak.upgrade() {
            avisa_falha_do_grafo(&app, res);
            graph_sync(&app, &state.borrow(), nodes, edges);
        }
    }
    graph_kick(timer, weak.clone(), state.clone(), nodes.clone(), edges.clone());
}

/// Mostra no banner do grafo o motivo de a carga ter falhado (app ausente ou ilegível).
/// **Onde:** [`graph_enter`] e [`graph_load_and_kick`]. Sucesso não toca o banner.
fn avisa_falha_do_grafo(app: &AppWindow, res: Result<(), String>) {
    if let Err(e) = res {
        app.global::<G>().set_reindex_status(e.into());
    }
}

/// Carrega o grafo do projeto no estado, sincroniza e (re)liga a física.
#[allow(clippy::too_many_arguments)]
pub(crate) fn graph_load_and_kick(
    proj: Option<&Path>,
    loaded: &RefCell<Option<PathBuf>>,
    timer: &Rc<slint::Timer>,
    weak: &Weak<AppWindow>,
    state: &Rc<RefCell<GraphState>>,
    nodes: &Rc<VecModel<GraphNode>>,
    edges: &Rc<VecModel<GraphEdge>>,
) {
    let res = load_graph_into(&mut state.borrow_mut(), proj);
    *loaded.borrow_mut() = proj.map(|p| p.to_path_buf());
    if let Some(app) = weak.upgrade() {
        avisa_falha_do_grafo(&app, res);
        graph_sync(&app, &state.borrow(), nodes, edges);
    }
    graph_kick(timer, weak.clone(), state.clone(), nodes.clone(), edges.clone());
}
