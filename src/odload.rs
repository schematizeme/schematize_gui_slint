//! Carga do estado do overdev de um projeto nas propriedades da aba Overdev,
//! incluindo o editor acoplado (que só lê o disco quando está aberto).

use crate::prelude::*;

pub(crate) fn load_overdev_into(app: &AppWindow, cl: &ChecklistView, proj: Option<&Path>) {
    let Some(p) = proj else {
        app.global::<Od>().set_has_project(false);
        app.global::<Od>().set_has_overdev(false);
        app.global::<Od>().set_current(SharedString::new());
        app.global::<Od>().set_editor_content(SharedString::new());
        app.global::<Od>().set_editor_status(SharedString::new());
        app.global::<Od>().set_notes(SharedString::new());
        cl.clear(app);
        // Sem projeto, zera os contadores da caixa e o aviso de skills — senão os
        // números do projeto anterior ficariam na tela falando de outro lugar.
        crate::wire::caixa::atualiza_caixa(app, None);
        return;
    };
    app.global::<Od>().set_has_project(true);
    // Recontagem da caixa de entrada e das skills desatualizadas. Fica AQUI, e não em
    // cada chamador, porque esta é a função única que significa "a tela passou a
    // mostrar este projeto" — ligar em cada ponto de chamada garantiria esquecer um.
    crate::wire::caixa::atualiza_caixa(app, Some(p));
    apply_agent_budget(app); // linha do governador (teto/livre/load) na aba Overdev
    app.global::<Od>().set_current(basename_of(p).into());
    let ov = panel::load_overdev(p);
    // Checklist 2-níveis parseado direto (o panel::load_overdev do lib ignora os
    // marcadores humanos `- [H ]`/`- [H x]`; aqui a GUI precisa deles).
    // O checklist COMPLETO fica no Rust; a UI recebe só a página corrente (e as
    // contagens, que a `ChecklistView` publica na mesma passada).
    cl.set_all(app, parse_checklist_items(p));
    // Sem run: objetivo vazio E sem itens (mesma regra do egui).
    let has = !(ov.objetivo.trim().is_empty() && cl.is_empty());
    app.global::<Od>().set_has_overdev(has);
    if !has {
        cl.clear(app);
        app.global::<Od>().set_editor_content(SharedString::new());
        app.global::<Od>().set_editor_status(SharedString::new());
        app.global::<Od>().set_notes(SharedString::new());
        return;
    }
    app.global::<Od>().set_objetivo(ov.objetivo.clone().into());
    app.global::<Od>().set_mode(ov.mode.clone().into());
    app.global::<Od>().set_decisoes(ov.decisoes.clone().into());
    app.global::<Od>().set_plano(ov.plano.clone().into());
    app.global::<Od>().set_perguntas(ov.perguntas.clone().into());
    // Editor (arquivo atualmente escolhido) + notas do humano.
    load_editor_content(app, p);
    app.global::<Od>().set_notes(overdev::read_notes(p).into());
}

/// Teto do que entra no `TextEdit` do editor acoplado. O `TextEdit` do Slint NÃO é
/// virtualizado: ele faz layout do texto INTEIRO (quebra de linha por caractere) a
/// cada medida. Um CHECKLIST.md de projeto grande passa fácil de 60 KB e sozinho
/// trava o event loop. Acima deste teto a GUI se recusa a editar inline e manda pro
/// editor externo — é o mesmo princípio de "prever o macaco": em vez de travar, o app
/// explica e oferece o caminho que funciona.
pub(crate) const EDITOR_MAX_BYTES: usize = 96 * 1024;

/// Carrega no editor o conteúdo do arquivo escolhido (`od-editor-target`) de `<root>/.overdev`.
/// Limpa o feedback de status. Arquivo ausente → editor vazio (o Salvar cria).
///
/// SÓ toca no disco quando o editor está ABERTO (`od-editor-open`): fechado, o
/// `TextEdit` nem existe na árvore do Slint e segurar o conteúdo custaria layout à
/// toa. Acima de [`EDITOR_MAX_BYTES`] marca `od-editor-too-big` e NÃO carrega —
/// a UI mostra o tamanho e o botão de abrir no editor externo.
pub(crate) fn load_editor_content(app: &AppWindow, root: &Path) {
    app.global::<Od>().set_editor_status(SharedString::new());
    app.global::<Od>().set_editor_error(false);
    if !app.global::<Od>().get_editor_open() {
        app.global::<Od>().set_editor_content(SharedString::new());
        app.global::<Od>().set_editor_too_big(false);
        app.global::<Od>().set_editor_size(SharedString::new());
        return;
    }
    let target = app.global::<Od>().get_editor_target().to_string();
    let content = std::fs::read_to_string(overdev_file_path(root, &target)).unwrap_or_default();
    if content.len() > EDITOR_MAX_BYTES {
        app.global::<Od>().set_editor_too_big(true);
        app.global::<Od>().set_editor_size(fmt_size(content.len() as i64).into());
        app.global::<Od>().set_editor_content(SharedString::new());
        return;
    }
    app.global::<Od>().set_editor_too_big(false);
    app.global::<Od>().set_editor_size(SharedString::new());
    app.global::<Od>().set_editor_content(content.into());
}

/// Zera TUDO que pertence ao projeto anterior e para o monitor dele.
///
/// O quê: sinaliza o stop do monitor, larga o flag de sessão e limpa os globais de escopo
/// de projeto (objetivo/estado/plano/decisões/perguntas, painel do monitor, log de
/// conclusões, tokens, status, editor e campos de digitação).
/// Onde: [`select_project`], antes de `load_overdev_into`.
///
/// ## Por que existe
/// Ao trocar de projeto NADA parava o monitor do anterior. A thread seguia viva lendo o
/// `.schematize/overdev/` do projeto ANTIGO e, desde a v0.7.2 — quando os contadores
/// viraram fonte única —, ela **reescrevia** `done/open/hold/human-open` e `mode` a cada 3
/// segundos. O usuário via os números do projeto anterior aparecerem sobre o novo e voltarem
/// sozinhos depois de qualquer refresh: não era dado velho na tela, era dado sendo
/// ativamente sobrescrito por um monitor zumbi.
///
/// Somado a isso, `load_overdev_into` retorna cedo quando o projeto novo NÃO tem overdev —
/// e nesse caminho `objetivo`, `mode`, `decisoes`, `plano` e `perguntas` nunca eram tocados,
/// então continuavam falando do projeto anterior.
///
/// **Entrada:** `app` e o sinal de parada compartilhado com o monitor.
/// **Saída:** nenhuma. **Efeitos:** muda globais da UI e sinaliza a thread do monitor.
pub(crate) fn reset_projeto(app: &AppWindow, stop: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    // O monitor testa este flag no topo de cada ciclo e encerra postando "stopped".
    stop.store(true, Ordering::SeqCst);
    let od = app.global::<Od>();
    od.set_session_running(false);

    // Escopo de projeto: sem isto, sobra do anterior.
    for setter in [
        Od::set_objetivo,
        Od::set_mode,
        Od::set_decisoes,
        Od::set_plano,
        Od::set_perguntas,
        Od::set_editor_content,
        Od::set_editor_status,
        Od::set_editor_target,
        Od::set_notes,
        Od::set_note_input,
        Od::set_correction_input,
        Od::set_run_status,
        Od::set_split_status,
        Od::set_agent_cmdline,
        Od::set_mon_mode,
        Od::set_usage_line,
        Od::set_upstream_line,
    ] {
        setter(&od, SharedString::new());
    }
    od.set_mon_iter(0);
    od.set_mon_max(0);
    od.set_confirm_open(false);
    od.set_editor_open(false);
    let vazio = || ModelRc::from(Rc::new(VecModel::<SharedString>::from(Vec::new())));
    od.set_mon_items(vazio());
    od.set_completions(vazio());
}

/// Escolhe um projeto: canoniza, persiste como recente e carrega o overdev.
///
/// Passa pelo [`reset_projeto`] ANTES de carregar: trocar de projeto tem que apagar o
/// anterior da tela e da memória, não sobrepor.
// 8 parâmetros: são os modelos da tela + o estado compartilhado. Agrupá-los num struct só
// pra calar o lint criaria um tipo sem significado próprio — mesma convenção do
// `graphview::graph_enter`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_project(
    app: &AppWindow,
    cl: &ChecklistView,
    proj_model: &VecModel<ProjItem>,
    dev_model: &VecModel<SharedString>,
    pin_model: &VecModel<SharedString>,
    cur: &RefCell<Option<PathBuf>>,
    stop: &std::sync::atomic::AtomicBool,
    path: PathBuf,
) {
    let abs = std::fs::canonicalize(&path).unwrap_or(path);
    config::add_recent_project(&abs.to_string_lossy());
    *cur.borrow_mut() = Some(abs.clone());
    reset_projeto(app, stop); // para o monitor do anterior e limpa o que era dele
    cl.reset_view(app); // projeto novo → filtro/página do anterior não valem mais
    load_overdev_into(app, cl, Some(&abs));
    // reflete o novo recente no seletor.
    refresh_proj_models(proj_model, dev_model, pin_model);
}
