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
/// antigo). Usa o MESMO resolvedor do hub (`schematize::paths`), que é o do app.
pub(crate) fn overdev_dir(root: &Path) -> PathBuf {
    schematize::paths::overdev_dir_at(root)
}

/// Caminho de um arquivo do editor (`PLAN.md`/`CHECKLIST.md`) no dir de overdev resolvido.
/// Sanitiza `target` a um basename simples pra a GUI nunca escrever fora do dir de overdev.
pub(crate) fn overdev_file_path(root: &Path, target: &str) -> PathBuf {
    let name = Path::new(target).file_name().and_then(|s| s.to_str()).unwrap_or("PLAN.md");
    overdev_dir(root).join(name)
}

/// Os itens do CHECKLIST 2-níveis do app como `OverItem`s (kind + origem + índice).
///
/// **Quem classifica é o app** (`schematize-overdev checklist --json`, lido em
/// [`crate::overdevcli::checklist`]): `- [H ]` casado antes de `- [ ]`, vínculo
/// `<!-- ovf:q:… -->` removido do texto, `humano_n` 1-based só nos humanos abertos (é o arg de
/// `od-mark-human`). O hub não tem segunda cópia da lei dos marcadores — a que divergisse
/// contaria diferente do gate.
/// **O quê:** o documento do app → as linhas da UI. PURA. **Onde:** `odload::load_overdev_into`.
pub(crate) fn itens_de(c: &crate::overdevcli::Checklist) -> Vec<OverItem> {
    c.itens
        .iter()
        .map(|i| OverItem {
            kind: i.tipo.as_str().into(),
            text: i.texto.as_str().into(),
            machine: i.maquina,
            hindex: if i.humano_n > 0 { i.humano_n } else { -1 },
            sub: i.sub,
        })
        .collect()
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

/// Recomputa o orçamento do governador de concorrência (`agents --json`, que também persiste
/// `~/.schematize/agents.json`) e joga nos props da GUI (linha "máquina: teto/livre/load/rodando"
/// e clampa o K do split ao teto). Sem o app, ou com falha dele, a linha NÃO é zerada: o motivo
/// vai para o status da execução (zero seria afirmar «a máquina não tem teto»).
pub(crate) fn apply_agent_budget(app: &AppWindow, projeto: Option<&Path>) {
    let dir = projeto.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let b = match crate::overdevcli::orcamento(&dir) {
        Ok(b) => b,
        Err(e) => {
            app.global::<Od>().set_run_status(e.into());
            return;
        }
    };
    app.global::<Od>().set_agent_cap(b.teto as i32);
    app.global::<Od>().set_agent_avail(b.livres as i32);
    app.global::<Od>().set_agent_running(b.rodando as i32);
    app.global::<Od>().set_agent_load(format!("{:.2}", b.load1).into());
    let cap = (b.teto as i32).max(2);
    let k = app.global::<Od>().get_split_k().clamp(2, cap);
    app.global::<Od>().set_split_k(k);
}
