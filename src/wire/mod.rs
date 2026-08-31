//! FIAÇÃO da janela — um módulo por recorte da UI.
//!
//! O quê: registrar os callbacks que o `ui/app.slint` declara. Onde: chamado uma
//! vez por `main()`, depois de criar a janela e antes do `run()`.
//!
//! Por quê existe o [`Ctx`]: os callbacks compartilham estado que vive no Rust
//! (modelos do Slint, estado do grafo, o checklist completo, timers, flags de
//! parada). Antes tudo isso eram locais de um `fn main` de 3.000 linhas — o que
//! fere o piso da casa (<=750 linhas por arquivo, uma unidade lógica por arquivo)
//! e tornava impossível ler um recorte sem carregar o resto na cabeça. O `Ctx`
//! junta esse estado num lugar só e cada `wire` clona (Rc/Arc, barato) o que usa.

use crate::prelude::*;

pub(crate) mod account;
pub(crate) mod appversion;
pub(crate) mod caixa;
pub(crate) mod database;
pub(crate) mod disco;
pub(crate) mod envs;
pub(crate) mod git;
pub(crate) mod graph;
pub(crate) mod manage;
pub(crate) mod odhistory;
pub(crate) mod overdev;
pub(crate) mod settings;
pub(crate) mod skills;
pub(crate) mod ssh;
pub(crate) mod vps;

/// Estado compartilhado pelos callbacks da janela.
///
/// Tudo aqui é `Rc`/`Arc`: clonar é barato e é assim que cada callback leva
/// sua referência pro `move`. Nada de dado grande por valor.
pub(crate) struct Ctx {
    pub(crate) row_items: Rc<Vec<Option<Item>>>,
    pub(crate) model: Rc<VecModel<SkillRow>>,
    pub(crate) modal: Rc<RefCell<ModalState>>,
    pub(crate) env_model: Rc<VecModel<EnvRow>>,
    pub(crate) env_methods: Rc<HashMap<String, Vec<String>>>,
    pub(crate) env_langs: Rc<HashSet<String>>,
    pub(crate) graph_state: Rc<RefCell<GraphState>>,
    pub(crate) graph_nodes: Rc<VecModel<GraphNode>>,
    pub(crate) graph_edges: Rc<VecModel<GraphEdge>>,
    pub(crate) graph_timer: Rc<slint::Timer>,
    pub(crate) graph_loaded: Rc<RefCell<Option<PathBuf>>>,
    pub(crate) od_proj_model: Rc<VecModel<ProjItem>>,
    pub(crate) od_dev_model: Rc<VecModel<SharedString>>,
    pub(crate) od_pin_model: Rc<VecModel<SharedString>>,
    pub(crate) od_cl: Rc<ChecklistView>,
    pub(crate) od_current: Rc<RefCell<Option<PathBuf>>>,
    pub(crate) od_stop_flag: Arc<AtomicBool>,
    pub(crate) od_snaps_all: Rc<RefCell<Vec<overdevdb::SnapshotMeta>>>,
    pub(crate) od_snaps_model: Rc<VecModel<SnapRow>>,
    pub(crate) od_commits_all: Rc<RefCell<Vec<githist::Commit>>>,
    pub(crate) od_commits_model: Rc<VecModel<CommitRow>>,
}

/// Troca TODAS as linhas de um modelo da UI, recuperando o `VecModel` concreto
/// por trás do `ModelRc` do global.
///
/// Existe por causa de uma restrição real: `Rc` não é `Send`, então um resultado
/// que volta de uma thread por `invoke_from_event_loop` NÃO pode trazer o modelo
/// consigo. Em vez de duplicar o estado em `Arc`, pegamos o modelo de volta do
/// global — já na thread da UI, que é a única que pode tocá-lo.
pub(crate) fn set_rows<T: Clone + 'static>(m: &ModelRc<T>, v: Vec<T>) {
    if let Some(vm) = m.as_any().downcast_ref::<VecModel<T>>() {
        vm.set_vec(v);
    }
}

/// Trava um `Mutex` de estado da UI ignorando envenenamento.
///
/// O que guardamos aqui são listas simples (achados, projetos): se uma thread
/// entrou em pânico segurando a trava, o `Vec` continua íntegro — não há
/// invariante pra proteger. Propagar o pânico só derrubaria a janela inteira.
pub(crate) fn trava<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
