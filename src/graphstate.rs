//! Estado da simulação do grafo: nós, arestas, transformação de tela e o passo
//! da física força-dirigida. A repulsão em si mora em `repulsion.rs`.

use crate::prelude::*;

/// Raio de um nó EM PIXELS de tela (constante ao zoom) — idêntico ao egui.
pub(crate) fn nsize(deg: f32) -> f32 {
    3.0 + (deg.sqrt() * 1.7).min(9.0)
}

/// Rótulo curto do nó (id truncado, seguro a UTF-8) — o egui truncava em 33+"…".
pub(crate) fn trunc_label(id: &str) -> String {
    if id.chars().count() > 34 {
        let s: String = id.chars().take(33).collect();
        format!("{s}…")
    } else {
        id.to_string()
    }
}

/// Nó com estado de simulação + flags de realce (recomputadas em `refresh_flags`).
pub(crate) struct GNode {
    pub(crate) id: String,
    pub(crate) loc: Option<String>,
    pub(crate) label: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) vx: f32,
    pub(crate) vy: f32,
    pub(crate) deg: f32,
    pub(crate) selected: bool,
    pub(crate) hot: bool, // vizinho do selecionado OU casa com a busca
    pub(crate) dim: bool, // apagado (fora do foco/da busca)
}

/// Estado inteiro do grafo (dono no Rust). Um por app; compartilhado via Rc<RefCell>.
#[derive(Default)]
pub(crate) struct GraphState {
    pub(crate) nodes: Vec<GNode>,
    pub(crate) edges: Vec<(usize, usize)>,
    pub(crate) project: Option<PathBuf>,
    // Drill-down: `Some(servico)` = vendo o grafo DETALHADO daquele microserviço
    // (`.schematize/grafos/<servico>.md`); `None` = a visão GLOBAL da aplicação.
    pub(crate) service: Option<String>,
    // Descrição por nó (nome -> "O quê"), vinda do índice/MAPA (§39). Carregada
    // junto do grafo; consultada ao selecionar um nó pra mostrar no bloco lateral.
    pub(crate) descs: HashMap<String, String>,
    pub(crate) sel: Option<usize>,
    pub(crate) search: String,
    pub(crate) scale: f32,
    pub(crate) ox: f32,
    pub(crate) oy: f32,
    pub(crate) alpha: f32,
    pub(crate) drag_node: Option<usize>,
    pub(crate) drag_off: (f32, f32),
    pub(crate) last_ptr: (f32, f32),
    pub(crate) moved: bool,
    pub(crate) canvas_w: f32,
    pub(crate) canvas_h: f32,
    pub(crate) fit_pending: bool,
    /// Instante mais recente entre os arquivos do grafo quando ele foi lido do disco.
    /// Serve pra `graph_enter` decidir se REENTRAR na aba precisa reler: a idempotência
    /// era só por caminho de projeto, então mudança nos `.md` do MESMO projeto (o overdev
    /// regenerando o índice, por exemplo) não reaparecia até trocar de projeto ou clicar
    /// em recarregar. `None` = nunca lido / dir ausente.
    pub(crate) carregado_em: Option<std::time::SystemTime>,
    // `true` quando o grafo global foi AGREGADO por microserviço (índice flat > cap, sem
    // GRAFO_GLOBAL.md): cada nó é um serviço ("<serviço> · N funções") e o drill abre o detalhe.
    pub(crate) aggregated: bool,
}

impl GraphState {
    /// Um passo da física (idêntico ao `step_graph` do egui). No-op se relaxado.
    pub(crate) fn step(&mut self) {
        if self.alpha < 0.02 {
            return;
        }
        let n = self.nodes.len();
        const REP: f32 = 1400.0;
        const SPR: f32 = 0.02;
        const LEN: f32 = 70.0;
        const G: f32 = 0.015;
        // Repulsão por GRADE (O(n · densidade)) em vez do par-a-par O(n²) que
        // congelava o app em grafos de verdade — ver `repulsion.rs`. Gravidade
        // segue global (é O(n) e é ela que segura os nós longe do infinito).
        let xs: Vec<f32> = self.nodes.iter().map(|nd| nd.x).collect();
        let ys: Vec<f32> = self.nodes.iter().map(|nd| nd.y).collect();
        let rep = repulsion::forces(&xs, &ys, REP);
        for i in 0..n {
            let (mut fx, mut fy) = rep[i];
            fx -= xs[i] * G;
            fy -= ys[i] * G;
            self.nodes[i].vx += fx;
            self.nodes[i].vy += fy;
        }
        for &(a, b) in &self.edges {
            let dx = self.nodes[b].x - self.nodes[a].x;
            let dy = self.nodes[b].y - self.nodes[a].y;
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = (d - LEN) * SPR;
            let (fx, fy) = (f * dx / d, f * dy / d);
            self.nodes[a].vx += fx;
            self.nodes[a].vy += fy;
            self.nodes[b].vx -= fx;
            self.nodes[b].vy -= fy;
        }
        for nd in &mut self.nodes {
            nd.vx *= 0.86;
            nd.vy *= 0.86;
            nd.x += nd.vx * self.alpha;
            nd.y += nd.vy * self.alpha;
        }
        self.alpha *= 0.994;
    }

    /// Tela (px relativo ao canvas) → mundo, desfazendo o centro + pan + zoom.
    pub(crate) fn to_world(&self, mx: f32, my: f32) -> (f32, f32) {
        let cx = self.canvas_w / 2.0;
        let cy = self.canvas_h / 2.0;
        ((mx - cx - self.ox) / self.scale, (my - cy - self.oy) / self.scale)
    }

    /// Nó sob o ponto de mundo (raio de tela convertido pra mundo) — como o egui.
    pub(crate) fn hit(&self, wx: f32, wy: f32) -> Option<usize> {
        let mut best = None;
        let mut bd = f32::MAX;
        for (i, n) in self.nodes.iter().enumerate() {
            let d = ((n.x - wx).powi(2) + (n.y - wy).powi(2)).sqrt();
            let r = nsize(n.deg) + 6.0 / self.scale;
            if d < r && d < bd {
                bd = d;
                best = Some(i);
            }
        }
        best
    }

    /// Enquadra todos os nós no canvas atual (idêntico ao `fit` do egui).
    pub(crate) fn fit(&mut self) {
        if self.nodes.is_empty() || self.canvas_w < 1.0 {
            return;
        }
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for n in &self.nodes {
            minx = minx.min(n.x);
            miny = miny.min(n.y);
            maxx = maxx.max(n.x);
            maxy = maxy.max(n.y);
        }
        let w = (maxx - minx).max(1.0);
        let h = (maxy - miny).max(1.0);
        self.scale = (0.86 * (self.canvas_w / w).min(self.canvas_h / h)).clamp(0.12, 6.0);
        self.ox = -self.scale * (minx + maxx) / 2.0;
        self.oy = -self.scale * (miny + maxy) / 2.0;
    }

    /// Recomputa selected/hot/dim (só muda em seleção/busca — não a cada tick).
    pub(crate) fn refresh_flags(&mut self) {
        let q = self.search.trim().to_lowercase();
        let focus = self.sel;
        let nb: HashSet<usize> = match focus {
            Some(i) => self
                .edges
                .iter()
                .filter_map(|&(a, b)| if a == i { Some(b) } else if b == i { Some(a) } else { None })
                .collect(),
            None => HashSet::new(),
        };
        for (i, n) in self.nodes.iter_mut().enumerate() {
            let matched = !q.is_empty() && n.id.to_lowercase().contains(&q);
            n.selected = focus == Some(i);
            n.hot = nb.contains(&i) || matched;
            n.dim = (focus.is_some() && focus != Some(i) && !nb.contains(&i)) || (!q.is_empty() && !matched);
        }
    }
}
