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

/// Escolhe um projeto: canoniza, persiste como recente e carrega o overdev.
pub(crate) fn select_project(
    app: &AppWindow,
    cl: &ChecklistView,
    proj_model: &VecModel<ProjItem>,
    dev_model: &VecModel<SharedString>,
    pin_model: &VecModel<SharedString>,
    cur: &RefCell<Option<PathBuf>>,
    path: PathBuf,
) {
    let abs = std::fs::canonicalize(&path).unwrap_or(path);
    config::add_recent_project(&abs.to_string_lossy());
    *cur.borrow_mut() = Some(abs.clone());
    cl.reset_view(app); // projeto novo → filtro/página do anterior não valem mais
    load_overdev_into(app, cl, Some(&abs));
    // reflete o novo recente no seletor.
    refresh_proj_models(proj_model, dev_model, pin_model);
}
