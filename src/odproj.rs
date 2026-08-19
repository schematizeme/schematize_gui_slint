//! Projetos e caminhos do overdev: detecção/seletor de projeto, resolução do dir
//! `.schematize/overdev`, parse do CHECKLIST 2-níveis e fechamento de item humano.

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

/// Dir de overdev do projeto pela regra "ler ambos" do lib: `.schematize/overdev` (novo, vivo) se
/// existir; senão `.overdev` legado; senão o novo default. Era hardcoded `.overdev` — por isso a GUI
/// mostrava 0/0/0 e editor vazio em projetos já migrados pro `.schematize/` (gerava no novo, lia no
/// antigo). Usa o MESMO resolvedor do CLI (`panel::load_overdev`), então GUI e CLI concordam.
pub(crate) fn overdev_dir(root: &Path) -> PathBuf {
    schematize::paths::overdev_dir_at(root)
}

/// Caminho do CHECKLIST do overdev do projeto (dir resolvido por `overdev_dir`).
pub(crate) fn checklist_path(root: &Path) -> PathBuf {
    overdev_dir(root).join("CHECKLIST.md")
}

/// Caminho de um arquivo do editor (`PLAN.md`/`CHECKLIST.md`) no dir de overdev resolvido.
/// Sanitiza `target` a um basename simples pra a GUI nunca escrever fora do dir de overdev.
pub(crate) fn overdev_file_path(root: &Path, target: &str) -> PathBuf {
    let name = Path::new(target).file_name().and_then(|s| s.to_str()).unwrap_or("PLAN.md");
    overdev_dir(root).join(name)
}

/// Parseia o CHECKLIST 2-níveis de `<root>` em `OverItem`s (kind + origem + índice).
/// Casa `- [H ...]` ANTES de `- [ ]`/`- [x]` (senão o humano cai no ramo de máquina).
/// `hindex` numera 1-based só os HUMANOS ABERTOS (- [H ]) — é o arg de `od-mark-human`.
pub(crate) fn parse_checklist_items(root: &Path) -> Vec<OverItem> {
    // Multi-arquivo: CHECKLIST.md E/OU a pasta checklist/*.md (granularidade / split multiagent) —
    // mesmo resolvedor do lib, pra a GUI contar certo depois de um split.
    let cl = schematize::paths::read_multidoc(&overdev_dir(root), "CHECKLIST.md", "checklist");
    let mut out = Vec::new();
    let mut hopen = 0i32; // contador de humanos abertos (1-based)
    for line in cl.lines() {
        let t = line.trim_start();
        let (kind, machine, hidx, rest): (&str, bool, i32, &str) =
            if let Some(r) = t.strip_prefix("- [H ]") {
                hopen += 1;
                ("hopen", false, hopen, r)
            } else if let Some(r) = t.strip_prefix("- [H x]").or_else(|| t.strip_prefix("- [H X]")) {
                ("hdone", false, -1, r)
            } else if let Some(r) = t.strip_prefix("- [ ]") {
                ("open", true, -1, r)
            } else if let Some(r) = t.strip_prefix("- [x]").or_else(|| t.strip_prefix("- [X]")) {
                ("done", true, -1, r)
            } else if let Some(r) = t.strip_prefix("- [~]") {
                ("hold", true, -1, r)
            } else {
                continue;
            };
        out.push(OverItem {
            kind: kind.into(),
            text: rest.trim().into(),
            machine,
            hindex: hidx,
        });
    }
    out
}

/// Fecha o `index`-ésimo (1-based) item HUMANO ABERTO de `<root>`: `- [H ]`→`- [H x]`.
/// Path-aware (o `overdev::human_done` do lib opera no cwd, não serve à GUI que
/// monitora outro projeto) — replica a regra do lib editando o arquivo direto.
pub(crate) fn mark_human_done_at(root: &Path, index: i32) -> Result<(), String> {
    if index < 1 {
        return Err("índice humano inválido".into());
    }
    let path = checklist_path(root);
    let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut seen = 0i32;
    let mut hit = false;
    let out: Vec<String> = s
        .lines()
        .map(|l| {
            if !hit && l.trim_start().starts_with("- [H ]") {
                seen += 1;
                if seen == index {
                    hit = true;
                    return l.replacen("- [H ]", "- [H x]", 1);
                }
            }
            l.to_string()
        })
        .collect();
    if !hit {
        return Err(format!("não há {index}º item humano aberto"));
    }
    std::fs::write(&path, out.join("\n")).map_err(|e| e.to_string())
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

/// Carrega o estado do overdev do `proj` (ou limpa se None) nas propriedades do app.
/// Espelha o overdev_view do egui: objetivo, mode, progresso, checklist e seções.
/// Ações de skills instaladas (gui.json) → linhas do modelo Slint. Cada uma vira um botão na aba do
/// projeto; Q.A./Pentest aparecem quando as skills schematize-engineering/pentest estão instaladas.
pub(crate) fn skill_action_rows() -> Vec<SkillAction> {
    schematize::guiactions::gui_actions()
        .into_iter()
        .map(|a| SkillAction {
            label: a.label.into(),
            command: a.command.into(),
            needs_project: a.needs_project,
            skill: a.skill.into(),
        })
        .collect()
}

/// Recomputa o orçamento do governador de concorrência e joga nos props da GUI (linha "máquina:
/// teto/livre/load/rodando" + clampa o K do split ao teto). Persiste ~/.schematize/agents.json.
pub(crate) fn apply_agent_budget(app: &AppWindow) {
    let b = schematize::agents::budget();
    let _ = schematize::agents::persist(&b);
    app.set_od_agent_cap(b.total_cap as i32);
    app.set_od_agent_avail(b.available as i32);
    app.set_od_agent_running(b.snap.running_claudes as i32);
    app.set_od_agent_load(format!("{:.2}", b.snap.load1).into());
    let cap = (b.total_cap as i32).max(2);
    let k = app.get_od_split_k().clamp(2, cap);
    app.set_od_split_k(k);
}
