//! Projetos do hub: detecção/seletor de projeto, troca do projeto atual e as ações declaradas
//! por skills. O checklist e o resto do estado do overdev são da janela `schematize-overdev-gui`.

use crate::prelude::*;

/// Basename de um caminho como String (fallback: o caminho inteiro).
pub(crate) fn basename_of(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// Cabeçalho de grupo do seletor (Detectados / Recentes).
pub(crate) fn proj_header(label: &str) -> ProjItem {
    ProjItem {
        is_header: true,
        label: label.into(),
        name: SharedString::new(),
        path: SharedString::new(),
        marker: SharedString::new(),
    }
}

/// Monta o modelo do seletor: grupo "detectados" (marcadores) + grupo "recentes"
/// (os que não estão já entre os detectados). Espelha o combo do project_bar egui.
pub(crate) fn build_proj_items(projects: &[projects::Project], recent: &[String]) -> Vec<ProjItem> {
    let mut out = Vec::new();
    if !projects.is_empty() {
        out.push(proj_header(&t("gui.detected_projects")));
        for pr in projects {
            out.push(ProjItem {
                is_header: false,
                label: SharedString::new(),
                name: pr.name.clone().into(),
                path: pr.path.clone().into(),
                marker: pr.marker.clone().into(),
            });
        }
    }
    let known: std::collections::HashSet<&str> = projects.iter().map(|p| p.path.as_str()).collect();
    let recents: Vec<&String> = recent.iter().filter(|r| !known.contains(r.as_str())).collect();
    if !recents.is_empty() {
        out.push(proj_header(&t("gui.recent_projects")));
        for r in recents {
            out.push(ProjItem {
                is_header: false,
                label: SharedString::new(),
                name: basename_of(&PathBuf::from(r)).into(),
                path: r.clone().into(),
                marker: SharedString::new(),
            });
        }
    }
    out
}

/// Re-sonda dev_dirs + pins + projetos e reconstrói os modelos do seletor, da lista
/// de dev_dirs e da lista de pastas FIXADAS. O scan agora inclui os pins (pastas
/// fixadas pelo usuário) — elas aparecem no seletor mesmo sem marcador git.
pub(crate) fn refresh_proj_models(
    proj_model: &VecModel<ProjItem>,
    dev_model: &VecModel<SharedString>,
    pin_model: &VecModel<SharedString>,
) {
    let dev = config::dev_dirs();
    let pins = config::projects();
    let projs = projects::scan_with_pins(&dev, &pins);
    let recent = config::recent_projects();
    proj_model.set_vec(build_proj_items(&projs, &recent));
    dev_model.set_vec(dev.into_iter().map(SharedString::from).collect::<Vec<SharedString>>());
    pin_model.set_vec(pins.into_iter().map(SharedString::from).collect::<Vec<SharedString>>());
}

/// Ações de skills instaladas (gui.json) → linhas do modelo Slint. Cada uma vira um botão na aba do
/// projeto; Q.A./Pentest aparecem quando as skills schematize-engineering/pentest estão instaladas.
pub(crate) fn skill_action_rows() -> Vec<SkillAction> {
    // **Lido do BINÁRIO, e não do `gui.json` direto** (E5 da extradição). O formato é contrato
    // com software de TERCEIRO, e um contrato com dois leitores é um conserto que só aparece
    // metade — ver `crate::skillactions`.
    crate::skillactions::ler()
        .into_iter()
        .map(|a| SkillAction {
            label: a.rotulo.into(),
            command: a.comando.into(),
            needs_project: a.precisa_projeto,
            skill: a.skill.into(),
        })
        .collect()
}

/// **O quê:** faz a tela passar a mostrar `proj` (ou nenhum): `has-project`, o nome e o aviso de
/// skills atrasadas.
///
/// **Onde:** [`select_project`], a reabertura do projeto recente no arranque (`main`) e o botão
/// Recarregar. É a função ÚNICA que significa «a tela passou a mostrar este projeto» — ligar a
/// recontagem em cada chamador garantiria esquecer um.
///
/// O estado do overdev (checklist, objetivo, contadores) NÃO é lido aqui: é da janela dele, que
/// lê o projeto por conta própria. **Efeitos:** muda globais da UI; roda `schematize-skills`.
pub(crate) fn load_projeto(app: &AppWindow, proj: Option<&Path>) {
    let od = app.global::<Od>();
    od.set_has_project(proj.is_some());
    od.set_current(proj.map(basename_of).unwrap_or_default().into());
    crate::wire::projeto::atualiza_skills(app, proj);
}

/// **O quê:** zera o que pertencia ao projeto anterior (status das ações, linha de upstream).
///
/// **Onde:** [`select_project`], antes de `load_projeto`. Trocar de projeto tem que apagar o
/// anterior da tela, não sobrepor.
pub(crate) fn reset_projeto(app: &AppWindow) {
    let od = app.global::<Od>();
    od.set_run_status(SharedString::new());
    od.set_upstream_line(SharedString::new());
}

/// **O quê:** escolhe um projeto: canoniza, persiste como recente e o mostra.
///
/// **Onde:** o seletor de projeto e «abrir pasta avulsa».
pub(crate) fn select_project(
    app: &AppWindow,
    proj_model: &VecModel<ProjItem>,
    dev_model: &VecModel<SharedString>,
    pin_model: &VecModel<SharedString>,
    cur: &RefCell<Option<PathBuf>>,
    path: PathBuf,
) {
    let abs = std::fs::canonicalize(&path).unwrap_or(path);
    config::add_recent_project(&abs.to_string_lossy());
    *cur.borrow_mut() = Some(abs.clone());
    reset_projeto(app);
    load_projeto(app, Some(&abs));
    // reflete o novo recente no seletor.
    refresh_proj_models(proj_model, dev_model, pin_model);
}
