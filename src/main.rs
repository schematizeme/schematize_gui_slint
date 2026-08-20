//! schematize — aba Skills em Slint (1º incremento REAL da migração egui→Slint).
//!
//! O quê: a aba Skills como GESTOR de verdade. Reusa a LÓGICA do crate irmão
//! `schematize` (sem a GUI egui): catálogo (`registry::catalog`), versões e
//! ações (`skills::installed_version` / `resolve_latest` / `install` / `remove`),
//! e i18n (`schematize::i18n`, 11 locales). O visual é Slint (ver `ui/app.slint`).
//!
//! Assíncrono: `resolve_latest` e as ações (instalar/remover) são REDE/IO — rodam
//! em threads e devolvem resultado à UI via `slint::invoke_from_event_loop` +
//! `Weak<AppWindow>::upgrade` (o padrão do Slint pra thread→UI). O event loop
//! nunca bloqueia. As ações em massa disparam em PARALELO (thread::scope),
//! espelhando o `run_batch` do egui; o lib serializa o `state.json` (STATE_LOCK).
//!
//! Escopo deste incremento: SÓ a aba Skills funcional. Overdev/Grafo ficam como
//! placeholders "em breve" na barra de abas (próximos incrementos).

slint::include_modules!(); // gera AppWindow, SkillRow, Theme, L a partir de ui/app.slint

// Fatiamento do checklist do overdev (paginação pura + a ligação com a UI). Vive fora deste
// arquivo porque é a peça que segura o custo de render CONSTANTE por página — ver checklist.rs.
// Módulos da GUI. O piso da casa é <=750 linhas e uma unidade lógica por arquivo:
// este `main.rs` guarda só o arranque e a FIAÇÃO das telas; toda a lógica mora nos
// módulos abaixo. `prelude` centraliza os imports comuns (inclusive os tipos que o
// `include_modules!()` acima gera).
mod prelude;

mod checklist;      // paginação PURA do checklist (o que segura o custo de render)
mod checklistview;  // ligação do checklist fatiado com as propriedades da UI
mod dbbuilder;      // Database builder: linhas de schema + grafo do schema
mod discorows;      // linhas/paginação da tela Disco (o Rust é dono da lista inteira)
mod envrows;        // linhas de Environments/SSH/idiomas + ações em terminal
mod fmt;            // formatação de valores pra UI (puro)
mod gitrows;        // linhas da tela Git (contas, estado dos repos, commits)
mod graphstate;     // estado + passo da física do grafo
mod graphview;      // ponte do grafo com a UI (modelos, carga preguiçosa, timer)
mod i18nbind;       // catálogo i18n -> propriedades do `global L`
mod odhistory;      // histórico do overdev (snapshots + commits), paginado
mod odload;         // carga do estado do overdev + editor acoplado
mod odmonitor;      // monitor leve do .schematize/overdev/ (thread -> UI)
mod odproj;         // projetos, caminhos e parse do CHECKLIST 2-níveis
mod repulsion;      // repulsão do grafo em grade espacial (era O(n²)/quadro)
mod skilljobs;      // trabalho de skills fora do event loop (rede/IO em thread)
mod skillrows;      // linhas e paginação da lista de skills
mod spiral;         // semente de posição dos nós do grafo (espiral áurea)
mod sysenv;         // integração com o sistema (processo, PATH, terminal, editor)
mod wire;           // FIAÇÃO da janela: um módulo por recorte da UI (ver wire/mod.rs)

use crate::prelude::*;
use checklistview::ChecklistView;
use dbbuilder::*;
use envrows::*;
use fmt::*;
use graphstate::*;
use graphview::*;
use i18nbind::*;
use odhistory::*;
use odload::*;
use odmonitor::*;
use odproj::*;
use skilljobs::*;
use skillrows::*;
use sysenv::*;

fn main() -> Result<(), slint::PlatformError> {
    detect_display_env();
    set_window_app_id();

    let items = registry::catalog();
    eprintln!("[catalog] {} skills (via schematize::registry::catalog)", items.len());
    let (rows, row_items) = build_rows(&items);
    let row_items = Rc::new(row_items);
    let model = Rc::new(VecModel::from(rows));

    let app = AppWindow::new()?;
    install_i18n(&app);
    // Logo da janela (título/taskbar) — mesma marca do egui.
    // Ícone da janela DESENHADO em código (resiliente — sem depender de arquivo).
    app.set_app_icon(make_app_icon());
    // Ações declaradas por skills instaladas (gui.json) → botões (Q.A., Pentest, …) na aba do projeto.
    app.global::<Od>().set_skill_actions(ModelRc::from(Rc::new(VecModel::from(skill_action_rows()))));
    // Versão do app (Configurações) — ex.: "Overflow v0.45.0".
    app.global::<App>().set_version(format!("Overflow v{}", upgrade::app_version()).into());
    app.global::<Sk>().set_rows(ModelRc::from(model.clone()));
    update_status(&app);
    recompute_headers(&app); // esconde cabeçalhos de página sem itens

    // Página inicial: Instaladas (0). Se NADA estiver instalado, abre no
    // Marketplace (1) — senão o usuário cai numa lista vazia.
    if !model.iter().any(|r| !r.is_header && is_installed(&r)) {
        app.set_active_tab(1);
    }
    // Recomputa a paginação para a aba inicial efetiva (o handler `changed active-tab`
    // ainda não está ligado neste ponto — recomputa explicitamente).
    recompute_pagination(&app);

    // Resolve o latest de todas as skills assim que a janela sobe (não bloqueia).
    kick_resolve_all(&app.as_weak(), &row_items);
    // Busca as notas do marketplace (1 request, thread) e preenche os badges por slug.
    kick_market_ratings(app.as_weak());

    // ---- aba Environments: modelo + índices auxiliares p/ o modal ----
    // Sonda a máquina UMA vez (local, rápido pra command -v). O refresh re-sonda.
    let env_status = environments::status();
    let env_model = Rc::new(VecModel::from(build_env_rows_from(&env_status)));
    app.global::<Cfg>().set_rows(ModelRc::from(env_model.clone()));
    // lang → métodos disponíveis (slugs), pra o modal montar os chips sem re-sondar.
    let env_methods: Rc<std::collections::HashMap<String, Vec<String>>> = Rc::new(
        env_status
            .iter()
            .map(|le| {
                (
                    le.lang.to_string(),
                    le.methods_available.iter().map(|m| m.slug().to_string()).collect(),
                )
            })
            .collect(),
    );
    // conjunto das 7 linguagens que TÊM environment (pra decidir a oferta no modal).
    // Só categoria "language" — ferramentas (claude/code/codex) não entram na oferta.
    let env_langs: Rc<std::collections::HashSet<String>> = Rc::new(
        env_status
            .iter()
            .filter(|le| le.category == "language")
            .map(|le| le.lang.to_string())
            .collect(),
    );
    // estado do modal de instalação (lado Rust).
    let modal = Rc::new(RefCell::new(ModalState::default()));

    // ==================== aba Grafo ====================
    // Estado (dono da física + transformação), dois VecModel (nós/arestas) e o
    // Timer da física. O grafo COMPARTILHA o projeto com a aba Overdev — carregado
    // junto na seleção/restauração de projeto (mais abaixo).
    let graph_state = Rc::new(RefCell::new(GraphState { scale: 1.0, alpha: 1.0, ..Default::default() }));
    let graph_nodes = Rc::new(VecModel::<GraphNode>::from(Vec::new()));
    let graph_edges = Rc::new(VecModel::<GraphEdge>::from(Vec::new()));
    let graph_timer = Rc::new(slint::Timer::default());
    // Projeto cujo grafo JÁ está carregado no `graph_state`. None = pendente: a
    // carga (parse do índice + física) acontece na 1ª entrada na aba Grafo.
    let graph_loaded: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    app.global::<G>().set_nodes(ModelRc::from(graph_nodes.clone()));
    app.global::<G>().set_edges(ModelRc::from(graph_edges.clone()));

    // ==================== aba Overdev ====================
    // Modelos: seletor de projeto (detectados + recentes), dev_dirs, e checklist.
    let od_proj_model = Rc::new(VecModel::<ProjItem>::from(Vec::new()));
    let od_dev_model = Rc::new(VecModel::<SharedString>::from(Vec::new()));
    let od_pin_model = Rc::new(VecModel::<SharedString>::from(Vec::new()));
    let od_items_model = Rc::new(VecModel::<OverItem>::from(Vec::new()));
    app.global::<Od>().set_projects(ModelRc::from(od_proj_model.clone()));
    app.global::<Od>().set_dev_dirs(ModelRc::from(od_dev_model.clone()));
    app.global::<Od>().set_pinned(ModelRc::from(od_pin_model.clone()));
    app.global::<Od>().set_items(ModelRc::from(od_items_model.clone()));
    // Dona do checklist COMPLETO (Rust) — o modelo acima só recebe a PÁGINA visível.
    let od_cl = Rc::new(ChecklistView::new(od_items_model.clone()));
    refresh_proj_models(&od_proj_model, &od_dev_model, &od_pin_model);
    // Projeto atual (lado Rust) — persiste entre execuções via recent_projects.
    let od_current: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    // Fase 4: flag de parada do MONITOR (o botão Parar a levanta; o monitor a checa
    // a cada fatia e encerra). `Arc` porque cruza pra a thread do monitor. NÃO mata o
    // `claude` — ele roda no terminal externo (processo próprio); só para o espelho.
    let od_stop_flag = Arc::new(AtomicBool::new(false));
    // Histórico do DB do overdev + commits (aditivos): estado completo no Rust +
    // modelos com a PÁGINA atual (paginação Rust-side).
    let od_snaps_all: Rc<RefCell<Vec<overdevdb::SnapshotMeta>>> = Rc::new(RefCell::new(Vec::new()));
    let od_snaps_model = Rc::new(VecModel::<SnapRow>::from(Vec::new()));
    let od_commits_all: Rc<RefCell<Vec<githist::Commit>>> = Rc::new(RefCell::new(Vec::new()));
    let od_commits_model = Rc::new(VecModel::<CommitRow>::from(Vec::new()));
    app.global::<Od>().set_snaps(ModelRc::from(od_snaps_model.clone()));
    app.global::<Od>().set_commits(ModelRc::from(od_commits_model.clone()));
    // `--project <path>` (ou 1º argumento posicional que seja um dir): abre DIRETO nesse projeto e na
    // aba Overdev — é o multi-janela (cada projeto no seu processo). Senão, restaura o mais recente.
    let arg_project: Option<PathBuf> = {
        let mut it = std::env::args().skip(1);
        let mut found: Option<String> = None;
        while let Some(a) = it.next() {
            if a == "--project" || a == "-p" {
                found = it.next();
                break;
            } else if !a.starts_with('-') {
                found = Some(a);
                break;
            }
        }
        found.map(PathBuf::from).filter(|p| p.is_dir())
    };
    let initial = arg_project
        .clone()
        .or_else(|| config::recent_projects().into_iter().next().map(PathBuf::from));
    match initial {
        Some(p) => {
            let abs = std::fs::canonicalize(&p).unwrap_or_else(|_| PathBuf::from(&p));
            *od_current.borrow_mut() = Some(abs.clone());
            load_overdev_into(&app, &od_cl, Some(&abs));
            refresh_od_history(&app, &od_snaps_all, &od_snaps_model, &od_commits_all, &od_commits_model, Some(&abs));
            // grafo compartilha o projeto restaurado.
            graph_mark_dirty(&graph_loaded); // grafo carrega só quando a aba Grafo abrir
            if arg_project.is_some() {
                app.set_screen(2); // veio de --project → abre na aba Overdev
            }
        }
        None => load_overdev_into(&app, &od_cl, None),
    }

    // ---- FIAÇÃO: registra os callbacks de cada recorte da UI ----
    // O estado compartilhado (modelos, grafo, checklist, flags) vai num `Ctx`; cada
    // módulo clona (Rc/Arc, barato) só o que usa. Fiação DEPOIS do estado inicial,
    // pra nenhum callback disparar antes de a janela estar consistente.
    let cx = wire::Ctx {
        row_items,
        model,
        modal,
        env_model,
        env_methods,
        env_langs,
        graph_state,
        graph_nodes,
        graph_edges,
        graph_timer,
        graph_loaded,
        od_proj_model,
        od_dev_model,
        od_pin_model,
        od_cl,
        od_current,
        od_stop_flag,
        od_snaps_all,
        od_snaps_model,
        od_commits_all,
        od_commits_model,
    };
    wire::skills::wire(&app, &cx);
    wire::envs::wire(&app, &cx);
    wire::manage::wire(&app, &cx);
    wire::overdev::wire(&app, &cx);
    wire::odhistory::wire(&app, &cx);
    wire::graph::wire(&app, &cx);
    wire::database::wire(&app, &cx);
    wire::disco::wire(&app, &cx);
    wire::git::wire(&app, &cx);
    wire::ssh::wire(&app, &cx);
    wire::settings::wire(&app, &cx);
    wire::appversion::wire(&app, &cx);
    wire::account::wire(&app, &cx);

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cria um `.overdev/CHECKLIST.md` temporário e ÚNICO (testes rodam em paralelo).
    fn scratch(checklist: &str) -> std::path::PathBuf {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let uniq = SEQ.fetch_add(1, Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("gui-od-test-{}-{}", std::process::id(), uniq));
        let od = base.join(".overdev");
        std::fs::create_dir_all(&od).unwrap();
        std::fs::write(od.join("CHECKLIST.md"), checklist).unwrap();
        base
    }

    const FIX: &str = "\
# OVERDEV
- [ ] item máquina aberto A
- [x] item máquina feito B
- [~] item on-hold C
- [H ] item humano aberto D
- [H x] item humano feito E
- [H ] item humano aberto F
não é item
  - [ ] item indentado aberto G
";

    #[test]
    fn parse_2niveis_classifica_e_indexa_humanos() {
        let root = scratch(FIX);
        let its = parse_checklist_items(&root);
        // 7 itens de checklist (a linha "não é item" é ignorada).
        assert_eq!(its.len(), 7);
        let by_kind = |k: &str| its.iter().filter(|i| i.kind == k).count();
        assert_eq!(by_kind("open"), 2, "máquina abertos (inclui indentado)");
        assert_eq!(by_kind("done"), 1, "máquina feito");
        assert_eq!(by_kind("hold"), 1, "on-hold");
        assert_eq!(by_kind("hopen"), 2, "humanos abertos");
        assert_eq!(by_kind("hdone"), 1, "humano feito");
        // hindex numera só os humanos abertos, 1-based, na ordem do arquivo.
        let hopen: Vec<i32> = its.iter().filter(|i| i.kind == "hopen").map(|i| i.hindex).collect();
        assert_eq!(hopen, vec![1, 2]);
        // itens de máquina não têm origem-humano nem índice.
        let mo = its.iter().find(|i| i.kind == "open").unwrap();
        assert!(mo.machine && mo.hindex == -1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn marca_humano_por_indice_so_o_que_casa() {
        let root = scratch(FIX);
        // fecha o 2º humano aberto (F) → vira - [H x]; D segue aberto.
        mark_human_done_at(&root, 2).unwrap();
        let its = parse_checklist_items(&root);
        assert_eq!(its.iter().filter(|i| i.kind == "hopen").count(), 1, "sobra 1 humano aberto");
        assert!(its.iter().any(|i| i.kind == "hopen" && i.text.contains("aberto D")));
        assert!(its.iter().any(|i| i.kind == "hdone" && i.text.contains("aberto F")));
        // não toca itens de máquina.
        assert_eq!(its.iter().filter(|i| i.kind == "open").count(), 2);
        // índice fora de faixa → erro, arquivo intacto.
        assert!(mark_human_done_at(&root, 9).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn editor_path_nunca_escapa_do_overdev() {
        // Sem `.overdev` nem `.schematize/overdev` no disco, o resolvedor "ler ambos" devolve o novo
        // default `.schematize/overdev`. O que o teste garante é a sanitização a basename (anti-traversal).
        let root = std::path::Path::new("/proj");
        let od = root.join(".schematize").join("overdev");
        assert_eq!(overdev_file_path(root, "PLAN.md"), od.join("PLAN.md"));
        assert_eq!(overdev_file_path(root, "CHECKLIST.md"), od.join("CHECKLIST.md"));
        // tentativa de path traversal é reduzida ao basename.
        assert_eq!(overdev_file_path(root, "../../etc/passwd"), od.join("passwd"));
    }
}
