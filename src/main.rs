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

// Módulos da GUI. O piso da casa é <=750 linhas e uma unidade lógica por arquivo:
// este `main.rs` guarda só o arranque e a FIAÇÃO das telas; toda a lógica mora nos
// módulos abaixo. `prelude` centraliza os imports comuns (inclusive os tipos que o
// `include_modules!()` acima gera).
mod lockpin; // guarda: o Cargo.lock tem que pinar um commit do CLI, não um caminho local
mod prelude;
mod procedencia; // de qual commit (e se árvore suja) este binário saiu — o `--version`

mod envrows; // linhas de Environments/SSH/idiomas + ações em terminal
mod fmt; // formatação de valores pra UI (puro)
mod gitlog; // histórico de commits, lido do BINÁRIO do git (E2 da extradição)
mod graphstate; // estado + passo da física do grafo
mod graphview; // ponte do grafo com a UI (modelos, carga preguiçosa, timer)
mod i18nbind; // catálogo i18n -> propriedades do `global L`
mod marketstatus; // versões, lidas do `market status --json` (E4 M5 da extradição)
mod odhistory; // histórico do overdev (snapshots + commits), paginado
mod odproj; // projetos do hub: seletor, troca de projeto, ações de skills
mod overdevcli; // a porta única para o `schematize-overdev` (leituras --json e ações)
mod repulsion; // repulsão do grafo em grade espacial (era O(n²)/quadro)
mod skillactions; // os botões que as skills declaram, lidos do BINÁRIO (E5)
mod skillsapplied; // skills atrasadas no projeto, lidas do BINÁRIO (E5)
mod spiral; // semente de posição dos nós do grafo (espiral áurea)
mod sysenv; // integração com o sistema (processo, PATH, terminal, editor)
mod wire; // FIAÇÃO da janela: um módulo por recorte da UI (ver wire/mod.rs)

use crate::prelude::*;
use envrows::*;
use fmt::*;
use graphstate::*;
use graphview::*;
use i18nbind::*;
use odhistory::*;
use odproj::*;
use sysenv::*;

/// **O quê:** o número da tela pedida em `--tela <nome>`, ou `None` se ninguém pediu.
///
/// **Onde:** [`main`], ao subir. Função PURA — os testes passam o vetor de argumentos.
///
/// **Os nomes são os da INTERFACE, não os do código.** Quem digita a flag vê "Banco de dados"
/// na tela, não `screen == 6`. Expor o número cravaria um detalhe do `.slint` numa interface
/// pública, e ele mudaria calado no dia em que uma tela entrasse no meio.
///
/// **Nome desconhecido devolve `None`, e não a tela 0.** Cair na Home seria a janela obedecendo
/// a um comando que ninguém entendeu, e quem digitou errado não saberia disso — acharia que a
/// tela pedida é a Home. Sem entender, ela abre onde abriria de qualquer forma.
fn tela_pedida(args: &[String]) -> Option<i32> {
    let i = args.iter().position(|a| a == "--tela")?;
    match args.get(i + 1)?.as_str() {
        "home" => Some(0),
        "skills" | "mercado" => Some(1),
        "overdev" => Some(2),
        "grafo" => Some(3),
        "config" | "settings" => Some(5),
        "banco" | "database" => Some(6),
        "disco" => Some(7),
        "git" => Some(8),
        "hosts" => Some(9),
        _ => None,
    }
}

fn main() -> Result<(), slint::PlatformError> {
    // `--version`/`-V` ANTES de qualquer coisa: sem isto o binário ABRIA A JANELA quando
    // alguém perguntava a versão. O coletor de debug do CLI (`sec_sistema`) faz exatamente
    // essa pergunta, então gerar um relatório tentava subir a GUI e o campo de versão vinha
    // com a primeira linha do log de ambiente (`(WAYLAND_DISPLAY=wayland-0))`) no lugar do
    // número. Responder e sair é o contrato mínimo de um binário de linha de comando.
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("schematize-gui {}", procedencia::rotulo_versao());
        return Ok(());
    }
    // Parasita do market: antes de abrir a janela (bloqueante só quando falta, uma vez por máquina).
    schematize_plataforma::parasita::garantir(&schematize_plataforma::parasita::Config::new(
        "schematize",
    ));
    detect_display_env();
    set_window_app_id();

    let app = AppWindow::new()?;
    install_i18n(&app);
    // Logo da janela (título/taskbar) — mesma marca do egui.
    // Ícone da janela DESENHADO em código (resiliente — sem depender de arquivo).
    app.set_app_icon(make_app_icon());
    // Ações declaradas por skills instaladas → botões (Q.A., Pentest, …) na aba do projeto.
    //
    // **Isto NÃO saiu na E5, e é de propósito.** É o único ponto em que uma skill de TERCEIRO
    // declara UI dentro desta janela; o contrato é dela com o hub, não com o app de skills.
    // Movê-lo quebraria software de fora — ver `src/skillactions.rs`.
    app.global::<Od>()
        .set_skill_actions(ModelRc::from(Rc::new(VecModel::from(skill_action_rows()))));
    // Versão do app (Configurações) — ex.: "schematize v0.49.0".
    app.global::<App>().set_version(format!("schematize v{}", upgrade::app_version()).into());
    // O CATÁLOGO, a paginação, as notas e o modal de instalação saíram daqui na E5.
    //
    // Este arquivo montava a lista inteira de skills ao subir — leitura de catálogo, resolução
    // de versão por rede e badges de nota — antes de a janela aparecer. Quem faz isso agora é o
    // `schematize-skills`, na janela dele, e este hub só aponta o caminho.

    // ==================== aba Grafo ====================
    // Estado (dono da física + transformação), dois VecModel (nós/arestas) e o
    // Timer da física. O grafo COMPARTILHA o projeto com a aba Overdev — carregado
    // junto na seleção/restauração de projeto (mais abaixo).
    let graph_state =
        Rc::new(RefCell::new(GraphState { scale: 1.0, alpha: 1.0, ..Default::default() }));
    let graph_nodes = Rc::new(VecModel::<GraphNode>::from(Vec::new()));
    let graph_edges = Rc::new(VecModel::<GraphEdge>::from(Vec::new()));
    let graph_timer = Rc::new(slint::Timer::default());
    // Projeto cujo grafo JÁ está carregado no `graph_state`. None = pendente: a
    // carga (parse do índice + física) acontece na 1ª entrada na aba Grafo.
    let graph_loaded: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    app.global::<G>().set_nodes(ModelRc::from(graph_nodes.clone()));
    app.global::<G>().set_edges(ModelRc::from(graph_edges.clone()));

    // ==================== aba Overdev ====================
    // Modelos: seletor de projeto (detectados + recentes), dev_dirs e pins.
    let od_proj_model = Rc::new(VecModel::<ProjItem>::from(Vec::new()));
    let od_dev_model = Rc::new(VecModel::<SharedString>::from(Vec::new()));
    let od_pin_model = Rc::new(VecModel::<SharedString>::from(Vec::new()));
    app.global::<Od>().set_projects(ModelRc::from(od_proj_model.clone()));
    app.global::<Od>().set_dev_dirs(ModelRc::from(od_dev_model.clone()));
    app.global::<Od>().set_pinned(ModelRc::from(od_pin_model.clone()));
    refresh_proj_models(&od_proj_model, &od_dev_model, &od_pin_model);
    // Projeto atual (lado Rust) — persiste entre execuções via recent_projects.
    let od_current: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    // Commits do projeto (e a linha de upstream): estado completo no Rust + modelo com a
    // PÁGINA atual (paginação Rust-side).
    let od_commits_all: Rc<RefCell<Vec<crate::gitlog::Commit>>> = Rc::new(RefCell::new(Vec::new()));
    let od_commits_model = Rc::new(VecModel::<CommitRow>::from(Vec::new()));
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
    // `--tela <nome>`: abre DIRETO numa tela, pelo NOME e não pelo número.
    //
    // O hub já sabia fazer isso para uma tela só, e só de um jeito: `--project` cravava a aba
    // Overdev. Quem quisesse qualquer outra tinha de clicar. Isto generaliza, e o destino é o
    // mesmo do `--aba` da janela do database: um lançador, um `.desktop` ou o ops abrir a tela
    // certa sem depender de alguém navegar.
    //
    // **Pelo nome, e não pelo número**, porque o número é detalhe interno do `.slint`: um
    // `--tela 6` viraria mentira no dia em que uma tela entrasse no meio, e ninguém notaria.
    let arg_tela = tela_pedida(&std::env::args().skip(1).collect::<Vec<_>>());

    let initial = arg_project
        .clone()
        .or_else(|| config::recent_projects().into_iter().next().map(PathBuf::from));
    match initial {
        Some(p) => {
            let abs = std::fs::canonicalize(&p).unwrap_or_else(|_| PathBuf::from(&p));
            *od_current.borrow_mut() = Some(abs.clone());
            load_projeto(&app, Some(&abs));
            refresh_od_history(&app, &od_commits_all, &od_commits_model, Some(&abs));
            // grafo compartilha o projeto restaurado.
            graph_mark_dirty(&graph_loaded); // grafo carrega só quando a aba Grafo abrir

            if arg_project.is_some() {
                app.set_screen(2); // veio de --project → abre na aba Overdev
            }
        }
        None => {
            load_projeto(&app, None);
        }
    }
    // DEPOIS do bloco acima, e de propósito: quem escreveu `--tela` disse o que quer com mais
    // precisão do que o `--project`, que cai na Overdev por default. Antes, o default o
    // sobrescreveria — e a flag mais específica perderia para a mais genérica.
    if let Some(n) = arg_tela {
        app.set_screen(n);
    }

    // ---- FIAÇÃO: registra os callbacks de cada recorte da UI ----
    // O estado compartilhado (modelos, grafo, commits) vai num `Ctx`; cada
    // módulo clona (Rc/Arc, barato) só o que usa. Fiação DEPOIS do estado inicial,
    // pra nenhum callback disparar antes de a janela estar consistente.
    let cx = wire::Ctx {
        graph_state,
        graph_nodes,
        graph_edges,
        graph_timer,
        graph_loaded,
        od_proj_model,
        od_dev_model,
        od_pin_model,
        od_current,
        od_commits_all,
        od_commits_model,
    };
    wire::envs::wire(&app, &cx);
    wire::overdev::wire(&app, &cx);
    wire::odhistory::wire(&app, &cx);
    wire::graph::wire(&app, &cx);
    wire::projeto::wire(&app, &cx);
    wire::settings::wire(&app, &cx);
    wire::appversion::wire(&app, &cx);
    wire::notificacoes::wire(&app, &cx);
    wire::account::wire(&app, &cx);

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// **`--tela` abre pelo NOME, e o nome é o da interface.**
    ///
    /// Quem digita a flag vê "Banco de dados" na tela, não `screen == 6`. Expor o número
    /// cravaria um detalhe do `.slint` numa interface pública, e ele mudaria calado no dia em
    /// que uma tela entrasse no meio.
    #[test]
    fn a_tela_pedida_abre_pelo_nome() {
        assert_eq!(tela_pedida(&v(&["--tela", "banco"])), Some(6));
        assert_eq!(tela_pedida(&v(&["--tela", "database"])), Some(6), "o apelido em inglês vale");
        assert_eq!(tela_pedida(&v(&["--tela", "overdev"])), Some(2));
        assert_eq!(tela_pedida(&v(&["--tela", "home"])), Some(0));
        // A ordem não importa: a flag pode vir depois do projeto.
        assert_eq!(tela_pedida(&v(&["/tmp/proj", "--tela", "grafo"])), Some(3));
    }

    /// **Nome desconhecido devolve `None`, e não a tela 0.**
    ///
    /// Cair na Home seria a janela obedecendo a um comando que ninguém entendeu, e quem digitou
    /// errado acharia que a tela pedida É a Home. E `--tela` no fim da linha não pode panicar
    /// antes de a janela aparecer (§37.48: invocação não prevista é bug do software).
    #[test]
    fn nome_desconhecido_nao_abre_tela_inventada() {
        assert_eq!(tela_pedida(&v(&["--tela", "turbinada"])), None);
        assert_eq!(tela_pedida(&v(&["--tela"])), None, "flag no fim da linha");
        assert_eq!(tela_pedida(&v(&[])), None);
        assert_eq!(tela_pedida(&v(&["--project", "/tmp"])), None);
    }

    /// **Todo número devolvido é uma tela que EXISTE no `.slint`.**
    ///
    /// Este teste lê o `app.slint` e cobra que cada destino tenha um `root.screen == N` lá. Sem
    /// ele, um `--tela` apontando para um número sem tela abriria a janela em BRANCO — e nada
    /// reprovaria, porque o Rust compila e o Slint não sabe que o número veio de uma flag.
    #[test]
    fn todo_destino_existe_no_slint() {
        let ui = include_str!("../ui/app.slint");
        for nome in [
            "home", "skills", "mercado", "overdev", "grafo", "config", "banco", "disco", "git",
            "hosts",
        ] {
            let n =
                tela_pedida(&v(&["--tela", nome])).unwrap_or_else(|| panic!("{nome} não mapeia"));
            assert!(
                ui.contains(&format!("root.screen == {n}")),
                "`--tela {nome}` aponta para a tela {n}, que não existe no app.slint — a janela \
                 abriria em BRANCO"
            );
        }
        // Self-check: o varredor precisa saber reprovar. A tela 99 não existe, e se esta
        // asserção parar de valer é porque a busca no `.slint` deixou de medir alguma coisa.
        assert!(!ui.contains("root.screen == 99"), "o self-check parou de valer");
    }
}
