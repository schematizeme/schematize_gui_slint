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
pub(crate) mod envs;
pub(crate) mod graph;
pub(crate) mod manage;
pub(crate) mod odhistory;
pub(crate) mod overdev;
pub(crate) mod quiz;
pub(crate) mod settings;
pub(crate) mod skills;

/// Estado compartilhado pelos callbacks da janela.
///
/// Tudo aqui é `Rc`/`Arc`: clonar é barato e é assim que cada callback leva
/// sua referência pro `move`. Nada de dado grande por valor.
pub(crate) struct Ctx {
    pub(crate) row_items: Rc<Vec<Option<Item>>>,
    pub(crate) model: Rc<VecModel<SkillRow>>,
    pub(crate) modal: Rc<RefCell<ModalState>>,
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
    pub(crate) od_commits_all: Rc<RefCell<Vec<crate::gitlog::Commit>>>,
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

// O `trava()` saiu com a tela de Disco (E3 M5): ela era a última que tinha estado em
// `Mutex` — a varredura do disco roda em thread e devolvia a lista por ali. O que sobra neste
// hub é estado de UI em `Rc<RefCell<…>>`, que vive só na thread da janela e não precisa de
// trava nenhuma.
//
// Ele existia por uma razão boa (ignorar envenenamento, porque um `Vec` de achados não tem
// invariante a proteger e propagar o pânico derrubaria a janela inteira). A razão morre junto
// com o único chamador: um helper de concorrência num módulo que deixou de ter concorrência é
// um convite a reintroduzi-la sem pensar.
