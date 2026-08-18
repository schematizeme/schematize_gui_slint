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

use schematize::agentrun;
use schematize::i18n::{self, t, tf};
use schematize::registry::{self, Item};
use schematize::{
    account, autostart, config, environments, githist, market, notifications, overdev, overdevdb,
    panel, projects, selfupdate, settings, skilledit, skills, sshkeys, upgrade, usage, util,
};
use slint::{Model, ModelRc, SharedString, TimerMode, VecModel, Weak};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::unix::process::CommandExt; // process_group: desacopla o restart
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

slint::include_modules!(); // gera AppWindow, SkillRow, Theme, L a partir de ui/app.slint

// ---------------------------------------------------------------------------
// Detecção de ambiente gráfico (Wayland vs X11 + desktop). Só loga — o backend
// winit do Slint escolhe sozinho o transporte certo em runtime.
// ---------------------------------------------------------------------------
fn detect_display_env() {
    let wayland = std::env::var("WAYLAND_DISPLAY").ok().filter(|s| !s.is_empty());
    let x11 = std::env::var("DISPLAY").ok().filter(|s| !s.is_empty());
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into());

    let server = match (&wayland, &x11) {
        (Some(w), _) => format!("Wayland (WAYLAND_DISPLAY={w})"),
        (None, Some(d)) => format!("X11 (DISPLAY={d})"),
        (None, None) => "NENHUM display detectado (headless/sandbox)".into(),
    };
    eprintln!("[env] servidor gráfico : {server}");
    eprintln!("[env] XDG_SESSION_TYPE : {}", if session.is_empty() { "?".into() } else { session });
    eprintln!("[env] desktop          : {desktop}");
    eprintln!("[env] idioma i18n      : {}", i18n::current_code());
    eprintln!("[env] backend Slint    : winit (default) — cobre Wayland E X11; renderer femtovg (OpenGL/GLES)");
    if wayland.is_none() && x11.is_none() {
        eprintln!("[env] AVISO: sem display, a janela não abre. Este incremento valida COMPILAÇÃO; a exibição precisa de um servidor Wayland/X11.");
    }
}

/// Traduz uma chave; se ela AINDA não existe no lib (o `t()` do lib devolve a
/// própria chave quando não acha), cai no `fallback` embutido. Usado só para as
/// chaves NOVAS desta fase (Home/navegação) — assim a UI já mostra texto decente
/// e, quando o lib ganhar essas chaves, passa a usar a tradução automaticamente.
/// As chaves novas estão listadas no relatório de entrega.
fn tor(key: &str, fallback: &str) -> String {
    let v = t(key);
    if v == key {
        fallback.to_string()
    } else {
        v
    }
}

// ---------------------------------------------------------------------------
// i18n: injeta TODOS os rótulos estáticos da UI no `global L` do .slint. Nada de
// texto hardcoded no Slint — as strings vêm de `schematize::i18n` (11 locales).
// ---------------------------------------------------------------------------
fn install_i18n(app: &AppWindow) {
    let l = app.global::<L>();
    l.set_window_title("schematize".into());
    l.set_subtitle(t("app.tagline").into());
    l.set_theme_light(t("gui.theme_light").into());
    l.set_theme_dark(t("gui.theme_dark").into());
    l.set_check(t("gui.check").into());
    l.set_update_all(t("gui.update_all").into());
    l.set_update_installed_only(t("gui.update_installed_only").into());
    l.set_update_sel(t("gui.update_sel").into());
    l.set_install_sel_market(t("gui.install_sel_market").into());
    l.set_remove_sel(t("gui.remove_sel").into());
    l.set_sel_label(t("gui.sel_label").into());
    l.set_sel_all(t("gui.sel_all").into());
    l.set_sel_pending(t("gui.sel_pending").into());
    l.set_sel_none(t("gui.sel_none").into());
    l.set_col_skill(t("gui.col_skill").into());
    l.set_col_author(t("gui.col_author").into());
    l.set_col_installed(t("gui.col_installed").into());
    l.set_col_latest(t("gui.col_latest").into());
    l.set_col_state(t("gui.col_state").into());
    l.set_col_actions(t("gui.col_actions").into());
    // Tooltip do selo verificado (só o check + hover; sem texto ao lado).
    l.set_verified(t("gui.verified_badge").into());
    l.set_tab_installed(t("gui.tab_installed").into());
    l.set_tab_marketplace(t("gui.tab_marketplace").into());
    l.set_tab_overdev(t("gui.tab_overdev").into());
    l.set_tab_graph(t("gui.tab_graph").into());
    l.set_coming_soon(t("gui.coming_soon").into());
    l.set_act_install(t("gui.install").into());
    l.set_act_update(t("gui.update").into());
    l.set_act_remove(t("gui.uninstall").into());
    // aba Environments
    l.set_tab_environments(t("gui.tab_environments").into());
    l.set_env_intro(t("gui.env_intro").into());
    l.set_env_method(t("gui.env_method").into());
    l.set_env_no_methods(t("gui.env_no_methods").into());
    // modal de instalação do Marketplace
    l.set_mp_recommends_note(t("gui.mp_recommends_note").into());
    l.set_mp_env_note(t("gui.mp_env_note").into());
    l.set_mp_confirm(t("gui.mp_confirm").into());
    l.set_mp_cancel(t("gui.mp_cancel").into());
    // aba Overdev (seletor de projeto + view) — reusa as chaves do egui.
    l.set_project(t("gui.project").into());
    l.set_no_project(t("gui.no_project").into());
    l.set_detected_projects(t("gui.detected_projects").into());
    l.set_recent_projects(t("gui.recent_projects").into());
    l.set_open_folder(t("gui.open_folder").into());
    l.set_reload(t("gui.reload").into());
    l.set_dev_dirs(t("gui.dev_dirs").into());
    l.set_add_dev_dir(t("gui.add_dev_dir").into());
    l.set_dev_dirs_empty(t("gui.dev_dirs_empty").into());
    l.set_remove(t("gui.remove").into());
    // Projetos fixados (pins) — chaves NOVAS com fallback embutido via `tor`.
    l.set_pinned_projects(tor("gui.pinned_projects", "Projetos fixados").into());
    l.set_pin_folder(tor("gui.pin_folder", "Fixar pasta…").into());
    l.set_unpin(tor("gui.unpin", "Desafixar").into());
    l.set_pin_hint(tor(
        "gui.pin_hint",
        "Uma pasta fixada vira UM projeto no selector — útil pra workspace de microserviços.",
    ).into());
    l.set_no_overdev(t("gui.no_overdev").into());
    l.set_od_decisions(t("gui.od_decisions").into());
    l.set_od_plan(t("gui.od_plan").into());
    l.set_od_questions(t("gui.od_questions").into());
    l.set_open_browser(t("gui.open_browser").into());
    // aba Overdev — Fase 3 (editor + tasks + checklist 2-níveis). Chaves NOVAS com
    // fallback embutido via `tor` até serem adicionadas ao lib (ver relatório).
    l.set_od_human(tor("gui.od_human", "humano").into());
    l.set_od_machine(tor("gui.od_machine", "máquina").into());
    l.set_od_mark_human(tor("gui.od_mark_human", "marcar como feito").into());
    l.set_od_editor(tor("gui.od_editor", "Editor").into());
    l.set_od_save_plan(tor("gui.od_save_plan", "Salvar").into());
    l.set_od_tasks(tor("gui.od_tasks", "Tarefas e notas").into());
    l.set_od_add_note(tor("gui.od_add_note", "Adicionar nota").into());
    l.set_od_note(tor("gui.od_note", "Nota para esta tarefa…").into());
    l.set_od_correction(tor("gui.od_correction", "Prompt de correção do overdev").into());
    l.set_od_notes_title(tor("gui.od_notes", "Notas e correções").into());
    // aba Overdev — Fase 4 (terminal externo + monitor leve). Chaves NOVAS via `tor`.
    l.set_od_run(tor("gui.od_run", "Executar overdev").into());
    l.set_od_stop(tor("gui.od_stop", "Parar").into());
    l.set_od_running(tor("gui.od_running", "monitorando…").into());
    l.set_od_mon_active(tor("gui.od_mon_active", "rodando").into());
    l.set_od_confirm_run(tor(
        "gui.od_confirm_run",
        "Isto abre o `claude` num TERMINAL EXTERNO (processo próprio, fora do app) e roda o overdev \
         neste projeto com acesso ao seu ambiente — ele pode editar arquivos. O app apenas MONITORA o \
         progresso. Confira o comando abaixo antes de confirmar.",
    ).into());
    l.set_od_run_done(tor("gui.od_done", "concluído").into());
    l.set_od_agent_cmd(tor("gui.od_agent_cmd", "Comando do agente").into());
    l.set_od_ext_terminal(tor(
        "gui.od_ext_terminal",
        "claude rodando em terminal externo (processo próprio) — o load dele fica fora do app.",
    ).into());
    l.set_od_mon_iters(tor("gui.od_mon_iters", "iterações").into());
    l.set_od_mon_open_title(tor("gui.od_mon_open_title", "Itens abertos (máquina)").into());
    // Reload/Acompanhar + log de conclusões + tokens (anexar monitor a run externo).
    l.set_od_attach(tor("gui.od_attach", "Reload / Acompanhar").into());
    l.set_od_refresh_tokens(tor("gui.od_refresh_tokens", "Atualizar tokens").into());
    l.set_od_completions_title(tor("gui.od_completions_title", "Conclusões").into());
    // aba Grafo — reusa as chaves do egui (todas já nos 11 locales do lib).
    l.set_g_search_hint(t("gui.search").into());
    l.set_g_nodes_suffix(t("gui.graph_nodes").into());
    l.set_g_fit(t("gui.fit").into());
    l.set_g_no_graph(t("gui.no_graph").into());
    l.set_g_export_obsidian(t("gui.export_obsidian").into());
    l.set_g_open_editor(t("gui.open_editor").into());
    l.set_g_no_loc(t("gui.no_loc").into());
    // Home + navegação (Fase 1) — chaves NOVAS, com fallback embutido via `tor`
    // até serem adicionadas ao lib. Ver lista no relatório de entrega.
    l.set_home(tor("gui.home", "Início").into());
    l.set_home_title(tor("gui.home_title", "O que você quer fazer?").into());
    l.set_home_market(tor("gui.home_market", "Mercado de Skills").into());
    l.set_home_overdev_desc(tor("gui.home_overdev_desc", "Acompanhe o desenvolvimento contínuo do projeto.").into());
    l.set_home_market_desc(tor("gui.home_market_desc", "Instale, atualize e descubra skills e environments.").into());
    l.set_home_graph_desc(tor("gui.home_graph_desc", "Explore o grafo de microfunções do projeto.").into());
    l.set_home_environments(tor("gui.home_environments", "Environments").into());
    l.set_home_environments_desc(tor("gui.home_environments_desc", "Gerencie os runtimes de linguagem.").into());
    l.set_home_ssh(tor("gui.home_ssh", "SSH").into());
    l.set_home_ssh_desc(tor("gui.home_ssh_desc", "Chaves e acesso remoto.").into());
    l.set_home_settings(tor("gui.home_settings", "Configurações").into());
    l.set_home_settings_desc(tor("gui.home_settings_desc", "Idioma, tema e preferências.").into());
    l.set_open_vscode(tor("gui.open_vscode", "Abrir no VSCode").into());
    l.set_open_loose_project(tor("gui.open_loose_project", "Abrir projeto avulso…").into());
    // aba Gerenciar (criar + editar skills) — chaves NOVAS via `tor`. Ver relatório.
    l.set_manage(tor("gui.manage", "Gerenciar").into());
    l.set_create_skill(tor("gui.create_skill", "Criar skill").into());
    l.set_edit_skill(tor("gui.edit_skill", "Editar skill").into());
    l.set_skill_slug(tor("gui.skill_slug", "Slug").into());
    l.set_skill_name(tor("gui.skill_name", "Nome").into());
    l.set_skill_desc(tor("gui.skill_desc", "Descrição").into());
    l.set_create(tor("gui.create", "Criar").into());
    l.set_save(tor("gui.save", "Salvar").into());
    l.set_saved(tor("gui.saved", "Salvo").into());
    l.set_slug_invalid(tor("gui.slug_invalid", "slug inválido — use só [a-z0-9-], começando por letra/dígito").into());
    l.set_skill_exists(tor("gui.skill_exists", "essa skill já existe").into());
    l.set_pick_skill(tor("gui.pick_skill", "Escolha uma skill…").into());
    l.set_pick_file(tor("gui.pick_file", "Arquivos").into());
    l.set_skill_created(tor("gui.skill_created", "Skill criada em").into());
    l.set_no_installed_skills(tor("gui.no_installed_skills", "Nenhuma skill instalada para editar").into());
    l.set_edit_now(tor("gui.edit_now", "Editar agora").into());
    l.set_pick_file_hint(tor("gui.pick_file_hint", "Selecione um arquivo na barra lateral para editar").into());
    // Tela SSH — chaves NOVAS via `tor`.
    l.set_ssh_title(tor("gui.ssh_title", "Chaves SSH").into());
    l.set_ssh_generate(tor("gui.ssh_generate", "Gerar chave").into());
    l.set_ssh_name(tor("gui.ssh_name", "Nome").into());
    l.set_ssh_kind(tor("gui.ssh_kind", "Tipo").into());
    l.set_ssh_comment(tor("gui.ssh_comment", "Comentário").into());
    l.set_ssh_passphrase(tor("gui.ssh_passphrase", "Passphrase (opcional)").into());
    l.set_ssh_copy_pub(tor("gui.ssh_copy_pub", "Copiar pública").into());
    l.set_ssh_copied(tor("gui.ssh_copied", "copiado").into());
    l.set_ssh_remove(tor("gui.ssh_remove", "Remover").into());
    l.set_ssh_empty(tor("gui.ssh_empty", "Nenhuma chave em ~/.ssh — gere uma acima.").into());
    l.set_ssh_priv_note(tor("gui.ssh_priv_note", "A chave privada nunca é exposta — só a pública sai.").into());
    l.set_ssh_keys_title(tor("gui.ssh_keys_title", "Suas chaves").into());
    // SSH — entropia (do lib, por tipo) + prova + Bitwarden. Chaves NOVAS via `tor`.
    l.set_ssh_entropy_ed25519(sshkeys::entropy_note(sshkeys::KeyKind::Ed25519).into());
    l.set_ssh_entropy_rsa(sshkeys::entropy_note(sshkeys::KeyKind::Rsa4096).into());
    l.set_ssh_kind_hint(tor(
        "gui.ssh_kind_hint",
        "ed25519 é o default forte da casa; use RSA só para hosts legados — e nunca abaixo de 4096 bits.",
    ).into());
    l.set_ssh_proof_label(tor("gui.ssh_proof_label", "Prova da chave (bits · fingerprint · tipo)").into());
    l.set_ssh_export_bw(tor("gui.ssh_export_bw", "Exportar → Bitwarden").into());
    l.set_ssh_bw_note(tor(
        "gui.ssh_bw_note",
        "Exportar → Bitwarden salva a chave no seu cofre (se destravado) ou gera um arquivo de import 600. \
         A chave PRIVADA nunca aparece nesta tela.",
    ).into());
    // Tela Configurações — chaves NOVAS via `tor`.
    l.set_cfg_title(tor("gui.cfg_title", "Configurações").into());
    l.set_cfg_language(tor("gui.cfg_language", "Idioma").into());
    l.set_cfg_theme(tor("gui.cfg_theme", "Tema").into());
    l.set_cfg_autostart(tor("gui.cfg_autostart", "Autostart do agente").into());
    l.set_cfg_autostart_desc(tor("gui.cfg_autostart_desc", "Inicia o agente de atualização junto com a sua sessão.").into());
    l.set_cfg_hooks(tor("gui.cfg_hooks", "Hooks do overdev").into());
    l.set_cfg_hooks_desc(tor("gui.cfg_hooks_desc", "Registra os hooks (Stop/PreToolUse) do overdev no Claude Code.").into());
    l.set_cfg_dirs(tor("gui.cfg_dirs", "Diretórios de dev e projetos fixados").into());
    l.set_cfg_dirs_desc(tor("gui.cfg_dirs_desc", "Onde o schematize procura os seus projetos.").into());
    l.set_cfg_manage(tor("gui.cfg_manage", "Gerenciar…").into());
    l.set_cfg_on(tor("gui.cfg_on", "ligado").into());
    l.set_cfg_off(tor("gui.cfg_off", "desligado").into());
    // Overdev — aditivos.
    l.set_od_history(tor("gui.od_history", "Histórico (cópia de segurança)").into());
    l.set_od_history_note(tor("gui.od_history_note", "O agente pode editar/apagar os arquivos do .overdev/ — este é o backup versionado deles.").into());
    l.set_od_view(tor("gui.od_view", "Ver").into());
    l.set_od_restore(tor("gui.od_restore", "Restaurar").into());
    l.set_od_snap_empty(tor("gui.od_snap_empty", "Sem snapshots ainda.").into());
    l.set_od_commits(tor("gui.od_commits", "Commits e push").into());
    l.set_od_commits_empty(tor("gui.od_commits_empty", "Sem commits (ou não é um repositório git).").into());
    l.set_od_close(tor("gui.od_close", "Fechar").into());
    // Paginação.
    l.set_pg_prev(tor("gui.pg_prev", "‹ Anterior").into());
    l.set_pg_next(tor("gui.pg_next", "Próximo ›").into());
    l.set_pg_of(tor("gui.pg_of", "de").into());
    // Versão do app + self-update (Configurações). Chaves NOVAS via `tor`.
    l.set_app_version_title(tor("gui.app_version_title", "Versão do app").into());
    l.set_app_check_update(tor("gui.app_check_update", "Verificar atualização").into());
    l.set_app_checking(tor("gui.app_checking", "Verificando…").into());
    l.set_app_up_to_date(tor("gui.app_up_to_date", "Você está atualizado").into());
    l.set_app_update_btn(tor("gui.app_update_btn", "Atualizar app").into());
    l.set_app_updating(tor("gui.app_updating", "Atualizando…").into());
    l.set_app_restart_hint(tor("gui.app_restart_hint", "Atualização concluída — reinicie o app.").into());
    l.set_app_restart(tor("gui.app_restart", "Reiniciar").into());
    // Sininho de notificações.
    l.set_notif_title(tor("gui.notif_title", "Notificações").into());
    l.set_notif_empty(tor("gui.notif_empty", "Sem notificações").into());
    l.set_notif_global(tor("gui.notif_global", "Globais").into());
    l.set_notif_personal(tor("gui.notif_personal", "Pessoais").into());
    l.set_notif_loading(tor("gui.notif_loading", "Carregando…").into());
    l.set_notif_do_update(tor("gui.notif_do_update", "Atualizar").into());
    l.set_notif_open(tor("gui.notif_open", "Abrir").into());
    l.set_notif_go_installed(tor("gui.notif_go_installed", "Ver instaladas").into());
    // Fork + comparar.
    l.set_fork_badge(tor("gui.fork_badge", "fork").into());
    l.set_fork_will(tor(
        "gui.fork_will",
        "Esta é uma skill OFICIAL. Ao editá-la, ela será forkada: uma cópia editável fica ativa e a versão oficial é preservada para comparar depois.",
    ).into());
    l.set_fork_active(tor(
        "gui.fork_active",
        "Fork ativo — a versão oficial está preservada para você comparar.",
    ).into());
    l.set_compare_official(tor("gui.compare_official", "Comparar com oficial").into());
    l.set_compare_note(tor(
        "gui.compare_note",
        "Comparar NÃO sobrescreve nada — apenas mostra as diferenças entre o seu fork e a versão oficial nova.",
    ).into());
    l.set_compare_files(tor("gui.compare_files", "Arquivos").into());
    l.set_compare_loading(tor("gui.compare_loading", "Comparando…").into());
    // Conta (login via device flow) — chaves NOVAS via `tor`.
    l.set_acc_section(tor("gui.acc_section", "Conta").into());
    l.set_acc_login(tor("gui.acc_login", "Entrar na plataforma").into());
    l.set_acc_logout(tor("gui.acc_logout", "Sair").into());
    l.set_acc_connected_as(tor("gui.acc_connected_as", "Conectado como").into());
    l.set_acc_logged_out_hint(tor(
        "gui.acc_logged_out_hint",
        "Entre na plataforma para receber notificações e sincronizar suas skills.",
    ).into());
    l.set_acc_modal_title(tor("gui.acc_modal_title", "Entrar na plataforma").into());
    l.set_acc_code_label(tor(
        "gui.acc_code_label",
        "Abra o endereço abaixo no navegador e digite este código:",
    ).into());
    l.set_acc_open_browser(tor("gui.acc_open_browser", "Abrir no navegador").into());
    l.set_acc_verification_at(tor("gui.acc_verification_at", "Acesse:").into());
    l.set_acc_waiting(tor("gui.acc_waiting", "Aguardando confirmação…").into());
    l.set_acc_cancel(tor("gui.acc_cancel", "Cancelar").into());
    l.set_acc_indicator_tip(tor("gui.acc_indicator_tip", "Conectado — abrir Conta").into());
}

// ---------------------------------------------------------------------------
// aba Gerenciar — lista os SLUGS das skills instaladas escaneando o diretório
// de skills (`~/.claude/skills/schematize-<slug>/` com SKILL.md). Cobre tanto as
// skills do catálogo quanto as criadas pelo usuário (que não estão no catálogo).
// Retorna Vec<String> (Send) — seguro pra rodar em thread e postar via event loop.
// ---------------------------------------------------------------------------
fn installed_skill_slugs() -> Vec<String> {
    let dir = util::skills_dir();
    let mut out: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue };
            if let Some(slug) = name.strip_prefix("schematize-") {
                if p.join("SKILL.md").is_file() {
                    out.push(slug.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Monta um `ModelRc<SharedString>` a partir de uma lista de Strings (roda na UI).
fn strings_model(v: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(
        v.into_iter().map(SharedString::from).collect::<Vec<SharedString>>(),
    )))
}

// ---------------------------------------------------------------------------
// Logo da janela — MESMA marca do egui (`schematize::appicon::rgba(256)`),
// convertida num `slint::Image` pra alimentar a propriedade `icon` do Window.
// ---------------------------------------------------------------------------
fn make_app_icon() -> slint::Image {
    let (rgba, w, h) = schematize::appicon::rgba(256);
    // clone_from_slice reinterpreta os bytes RGBA (u8) como pixels Rgba8.
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, w, h);
    slint::Image::from_rgba8(buf)
}

// ---------------------------------------------------------------------------
// Relança o app numa janela NOVA e encerra este processo. CONSERTO do bug do
// "reiniciar" (pós self-update) que só fechava e não reabria: fazemos um spawn
// DESACOPLADO do binário atual (nova sessão de processos via `process_group(0)`
// + stdio em /dev/null) ANTES do `exit(0)`, então a janela nova sobe sozinha e
// sobrevive à saída deste. Chamado pelo callback `restart` do Slint.
// ---------------------------------------------------------------------------
fn restart_app() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(&exe);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.process_group(0); // grupo próprio → não morre com o processo atual
        let _ = cmd.spawn(); // best-effort: se falhar, ainda saímos limpo
    }
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Abre um caminho no gerenciador de arquivos do sistema (xdg-open <path>).
// ---------------------------------------------------------------------------
fn open_path_in_files(root: &Path) {
    util::open_url(&root.to_string_lossy());
}

// ---------------------------------------------------------------------------
// Abre o projeto no VSCode: `code <root>` se o binário existe; senão cai no
// esquema `vscode://file/<root>` (best-effort via xdg-open).
// ---------------------------------------------------------------------------
fn open_in_vscode(root: &Path) {
    let root_s = root.to_string_lossy().into_owned();
    if which_bin("code")
        && std::process::Command::new("code")
            .arg(&root_s)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    {
        return;
    }
    util::open_url(&format!("vscode://file/{root_s}"));
}

// ---------------------------------------------------------------------------
// Estado derivado (missing/outdated/current/loading) + rótulo traduzido.
// ---------------------------------------------------------------------------
fn compute_state(installed: &Option<String>, latest: &Option<String>) -> (String, String) {
    match (installed, latest) {
        // Não instalada — mesmo com latest desconhecido, dá pra instalar.
        (None, _) => ("missing".into(), t("common.not_installed")),
        // Instalada, mas ainda resolvendo o latest (rede) → spinner.
        (Some(_), None) => ("loading".into(), "…".into()),
        (Some(i), Some(l)) if i == l => ("current".into(), t("common.current")),
        // Desatualizada: "UPDATE (X→Y)".
        (Some(i), Some(l)) => ("outdated".into(), format!("{} ({}→{})", t("common.update"), i, l)),
    }
}

// ---------------------------------------------------------------------------
// Montagem inicial do modelo (cabeçalhos de categoria + skills). Retorna as
// linhas E o Item alinhado a cada linha (None nos cabeçalhos), pra as ações.
// ---------------------------------------------------------------------------
/// Categoria normalizada de um item (vazio → "language").
fn category_of(it: &Item) -> &str {
    if it.category.is_empty() { "language" } else { it.category.as_str() }
}

/// Cabeçalho de categoria de UMA página (page 0 = Instaladas, 1 = Marketplace).
/// `count` é preenchido/atualizado por `recompute_headers` (esconde vazios).
fn header_row(label: &str, cat: &str, page: i32) -> SkillRow {
    SkillRow {
        is_header: true,
        header_label: label.into(),
        page,
        count: 0,
        category: cat.into(),
        slug: SharedString::new(),
        author: SharedString::new(),
        author_url: SharedString::new(),
        installed: SharedString::new(),
        latest: SharedString::new(),
        state: SharedString::new(),
        state_label: SharedString::new(),
        verified: false,
        selected: false,
        busy: false,
        op_label: SharedString::new(),
        op_error: false,
        disp: -1,
        forked: false,
        rating: SharedString::new(),
    }
}

/// Linha inicial de uma skill: instalada lida do disco (rápido), latest ainda
/// "…" (resolvido depois, assíncrono). Estado derivado do que já se sabe.
/// `forked` = a skill oficial virou fork editável (marca [fork] + habilita Comparar).
fn skill_row(it: &Item, forked: bool) -> SkillRow {
    let author = it.sponsor.as_ref().map(|s| s.name.clone()).unwrap_or_default();
    let author_url = it.sponsor.as_ref().map(|s| s.url.clone()).unwrap_or_default();
    let installed = skills::installed_version(it);
    let latest: Option<String> = None; // resolvido assíncrono após subir a janela
    let (state, state_label) = compute_state(&installed, &latest);
    SkillRow {
        is_header: false,
        header_label: SharedString::new(),
        page: 0,
        count: 0,
        category: category_of(it).into(),
        slug: it.slug.clone().into(),
        author: author.into(),
        author_url: author_url.into(),
        installed: installed.unwrap_or_else(|| "—".into()).into(),
        latest: "…".into(),
        state: state.into(),
        state_label: state_label.into(),
        verified: it.verified,
        selected: false,
        busy: false,
        op_label: SharedString::new(),
        op_error: false,
        disp: -1,
        forked,
        rating: SharedString::new(),
    }
}

/// Conjunto dos slugs atualmente FORKADOS (lido do estado uma vez).
fn forked_slugs() -> HashSet<String> {
    skills::load_state()
        .skills
        .iter()
        .filter(|(_, e)| e.forked)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Ordena os itens em grupos (base, language, external). Por categoria emite
/// DOIS cabeçalhos (Instaladas page=0 e Marketplace page=1) seguidos das skills;
/// a página ativa mostra o cabeçalho certo e as skills cujo estado casa (o Slint
/// filtra por `state`). Devolve o Item por linha (None nos cabeçalhos).
fn build_rows(items: &[Item]) -> (Vec<SkillRow>, Vec<Option<Item>>) {
    let groups = [
        ("base", t("gui.cat_base")),
        ("language", t("gui.cat_language")),
        ("external", t("gui.cat_external")),
    ];
    let forked = forked_slugs();
    let mut rows = Vec::new();
    let mut row_items: Vec<Option<Item>> = Vec::new();
    for (cat, label) in groups {
        let group: Vec<&Item> = items.iter().filter(|it| category_of(it) == cat).collect();
        if group.is_empty() {
            continue;
        }
        // page 0 = Instaladas, page 1 = Marketplace — só um aparece por vez.
        rows.push(header_row(&label, cat, 0));
        row_items.push(None);
        rows.push(header_row(&label, cat, 1));
        row_items.push(None);
        for it in group {
            rows.push(skill_row(it, forked.contains(it.slug.as_str())));
            row_items.push(Some(it.clone()));
        }
    }
    (rows, row_items)
}

/// Atualiza o marcador `forked` da linha de uma skill (por slug) no modelo do app —
/// chamado após uma edição que forka uma skill oficial, pra o badge [fork] e o botão
/// Comparar aparecerem sem recarregar a lista inteira. Opera sobre `app.get_rows()`
/// (roda no event loop; nada de Rc cruzando thread).
fn mark_row_forked(app: &AppWindow, slug: &str, forked: bool) {
    let rows = app.get_rows();
    for i in 0..rows.row_count() {
        if let Some(mut r) = rows.row_data(i) {
            if !r.is_header && r.slug == slug && r.forked != forked {
                r.forked = forked;
                rows.set_row_data(i, r);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reconta os cabeçalhos: cada cabeçalho (page, categoria) ganha o nº de skills
// que estão AGORA na sua página (Instaladas = state != "missing"; Marketplace =
// state == "missing"). count==0 → o Slint esconde o cabeçalho. Roda no event
// loop (só usa o modelo do app; nada de dados !Send).
// ---------------------------------------------------------------------------
fn recompute_headers(app: &AppWindow) {
    let rows = app.get_rows();
    let n = rows.row_count();
    for i in 0..n {
        let Some(mut h) = rows.row_data(i) else { continue };
        if !h.is_header {
            continue;
        }
        let mut count = 0;
        for j in 0..n {
            if let Some(r) = rows.row_data(j) {
                if r.is_header || r.category != h.category {
                    continue;
                }
                let missing = r.state == "missing";
                // page 0 = Instaladas (não-missing); page 1 = Marketplace (missing).
                if (h.page == 1 && missing) || (h.page == 0 && !missing) {
                    count += 1;
                }
            }
        }
        if h.count != count {
            h.count = count;
            rows.set_row_data(i, h);
        }
    }
    recompute_pagination(app);
}

// ---------------------------------------------------------------------------
// Paginação do Mercado: numera (disp) as skills VISÍVEIS na página-tab ativa em
// ordem sequencial; -1 nas que não pertencem à página. O Slint mostra só as
// cujo `disp` cai na janela `[mkt-page*20, +20)`. Total → controla o Pager.
// ---------------------------------------------------------------------------
fn recompute_pagination(app: &AppWindow) {
    let tab = app.get_active_tab();
    let rows = app.get_rows();
    let n = rows.row_count();
    let mut idx = 0i32;
    for i in 0..n {
        let Some(mut r) = rows.row_data(i) else { continue };
        if r.is_header {
            continue;
        }
        let missing = r.state == "missing";
        let on_page = (tab == 0 && !missing) || (tab == 1 && missing);
        let new_disp = if on_page {
            let d = idx;
            idx += 1;
            d
        } else {
            -1
        };
        if r.disp != new_disp {
            r.disp = new_disp;
            rows.set_row_data(i, r);
        }
    }
    app.set_mkt_total(idx);
}

// ---------------------------------------------------------------------------
// Status global (contagem de pendências) — mesma regra do egui.
// ---------------------------------------------------------------------------
fn update_status(app: &AppWindow) {
    let rows = app.get_rows();
    let mut pending = 0usize;
    for i in 0..rows.row_count() {
        if let Some(r) = rows.row_data(i) {
            if !r.is_header && (r.state == "missing" || r.state == "outdated") {
                pending += 1;
            }
        }
    }
    let status = if pending == 0 {
        t("gui.all_uptodate")
    } else {
        tf("gui.n_pending", &[("n", &pending.to_string())])
    };
    app.set_status(status.into());
}

// ---------------------------------------------------------------------------
// thread→UI: posta a atualização de versões (installed + latest) de uma linha.
// ---------------------------------------------------------------------------
fn post_versions(weak: Weak<AppWindow>, idx: usize, installed: Option<String>, latest: Option<String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                let (state, label) = compute_state(&installed, &latest);
                r.installed = installed.clone().unwrap_or_else(|| "—".into()).into();
                r.latest = latest.clone().unwrap_or_else(|| "?".into()).into();
                r.state = state.into();
                r.state_label = label.into();
                rows.set_row_data(idx, r);
            }
            update_status(&app);
            recompute_headers(&app);
        }
    });
}

/// thread→UI: marca uma linha como ocupada (operação em andamento) com rótulo.
fn post_row_busy(weak: Weak<AppWindow>, idx: usize, label: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                r.busy = true;
                r.op_label = label.into();
                r.op_error = false;
                rows.set_row_data(idx, r);
            }
        }
    });
}

/// thread→UI: resultado de uma operação numa linha. Instalar → instalada=latest
/// (o release baixado É o latest) e estado "current"; remover → não instalada.
/// Erro → mantém e mostra o rótulo em warn.
fn post_row_result(weak: Weak<AppWindow>, idx: usize, install: bool, res: Result<String, String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                r.busy = false;
                match res {
                    Ok(ver) => {
                        r.op_label = SharedString::new();
                        r.op_error = false;
                        if install {
                            r.installed = ver.clone().into();
                            r.latest = ver.into();
                            r.state = "current".into();
                            r.state_label = t("common.current").into();
                        } else {
                            r.installed = "—".into();
                            r.state = "missing".into();
                            r.state_label = t("common.not_installed").into();
                        }
                    }
                    Err(e) => {
                        r.op_label = tf("err.prefix", &[("e", &e)]).into();
                        r.op_error = true;
                    }
                }
                rows.set_row_data(idx, r);
            }
            update_status(&app);
            recompute_headers(&app);
        }
    });
}

/// thread→UI: fim do lote — solta o `busy` global e mostra o toast final.
fn post_batch_done(weak: Weak<AppWindow>, ok: usize, err: usize) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            app.set_busy(false);
            let toast = tf("gui.batch_done", &[("ok", &ok.to_string()), ("err", &err.to_string())]);
            app.set_status(toast.into());
            recompute_headers(&app);
        }
    });
}

// ---------------------------------------------------------------------------
// Resolução assíncrona do latest de UMA skill (rede). Detached: reusa install/
// check. Re-lê a instalada (barato) pra refletir mudanças de disco.
// ---------------------------------------------------------------------------
fn spawn_resolve(weak: Weak<AppWindow>, idx: usize, item: Item) {
    std::thread::spawn(move || {
        let installed = skills::installed_version(&item);
        let latest = skills::resolve_latest(&item).ok();
        post_versions(weak, idx, installed, latest);
    });
}

/// Dispara a resolução do latest de todas as skills em paralelo (uma thread por
/// skill; são poucas). Antes, zera a coluna latest de volta pra "…".
fn kick_resolve_all(weak: &Weak<AppWindow>, row_items: &Rc<Vec<Option<Item>>>) {
    if let Some(app) = weak.upgrade() {
        let rows = app.get_rows();
        for (idx, maybe) in row_items.iter().enumerate() {
            if let Some(it) = maybe {
                if let Some(mut r) = rows.row_data(idx) {
                    r.latest = "…".into();
                    if r.installed != "—" {
                        r.state = "loading".into();
                        r.state_label = "…".into();
                    }
                    rows.set_row_data(idx, r);
                }
                spawn_resolve(weak.clone(), idx, it.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Notas do marketplace: busca TODAS as médias numa thread (1 request) e preenche
// a coluna `rating` de cada linha de skill por slug. Sem nota (count 0 / None) →
// `format_rating` devolve "" e o badge some. Falha de rede = HashMap vazio → só
// não mostra nota (a UI nunca trava; nada de bloqueio no event loop).
// ---------------------------------------------------------------------------
fn kick_market_ratings(weak: Weak<AppWindow>) {
    std::thread::spawn(move || {
        let ratings = market::market_ratings_all(); // HashMap<String,(f32,u32)> (Send)
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                let rows = app.get_rows();
                for i in 0..rows.row_count() {
                    if let Some(mut r) = rows.row_data(i) {
                        if r.is_header {
                            continue;
                        }
                        let rating = market::format_rating(ratings.get(r.slug.as_str()).copied());
                        if r.rating.as_str() != rating {
                            r.rating = rating.into();
                            rows.set_row_data(i, r);
                        }
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Ações em massa/paralelo (espelha o run_batch do egui). ops = (idx, install?, Item).
// ---------------------------------------------------------------------------
fn run_batch(weak: Weak<AppWindow>, ops: Vec<(usize, bool, Item)>) {
    if ops.is_empty() {
        return;
    }
    if let Some(app) = weak.upgrade() {
        if app.get_busy() {
            return; // já tem lote rodando
        }
        app.set_busy(true);
    }
    std::thread::spawn(move || {
        let ok = AtomicUsize::new(0);
        let err = AtomicUsize::new(0);
        // marca cada linha como ocupada antes de começar
        for (idx, install, _) in &ops {
            let label = if *install { t("gui.installing") } else { t("gui.removing") };
            post_row_busy(weak.clone(), *idx, label);
        }
        // executa TODAS em paralelo; o lib serializa o state.json (STATE_LOCK).
        std::thread::scope(|sc| {
            for (idx, install, item) in &ops {
                let weak = weak.clone();
                let ok = &ok;
                let err = &err;
                sc.spawn(move || {
                    let res: Result<String, String> = if *install {
                        skills::install(item)
                    } else {
                        skills::remove(item).map(|_| String::new())
                    };
                    if res.is_ok() {
                        ok.fetch_add(1, Ordering::SeqCst);
                    } else {
                        err.fetch_add(1, Ordering::SeqCst);
                    }
                    post_row_result(weak, *idx, *install, res);
                });
            }
        });
        post_batch_done(weak, ok.load(Ordering::SeqCst), err.load(Ordering::SeqCst));
    });
}

/// Coleta ops (idx, install?, Item) das linhas que casam com o predicado.
/// `install=true` → instalar/atualizar; `install=false` → remover.
fn collect_ops(
    app: &AppWindow,
    row_items: &Rc<Vec<Option<Item>>>,
    install: bool,
    pred: impl Fn(&SkillRow) -> bool,
) -> Vec<(usize, bool, Item)> {
    let rows = app.get_rows();
    let mut ops = Vec::new();
    for (idx, maybe) in row_items.iter().enumerate() {
        if let Some(it) = maybe {
            if let Some(r) = rows.row_data(idx) {
                if !r.is_header && pred(&r) {
                    ops.push((idx, install, it.clone()));
                }
            }
        }
    }
    ops
}

/// NÃO instalada (pertence ao Marketplace).
fn is_missing(r: &SkillRow) -> bool {
    r.state == "missing"
}
/// Instalada (pertence a Instaladas) — qualquer estado que não seja "missing".
fn is_installed(r: &SkillRow) -> bool {
    r.state != "missing"
}
/// Instalada E desatualizada (installed Some E latest > installed). É o ÚNICO
/// alvo de "Atualizar tudo"/"Atualizar selecionadas": jamais instala nova.
fn is_outdated(r: &SkillRow) -> bool {
    r.state == "outdated"
}

// ===========================================================================
// ENVIRONMENTS — gestão dos runtimes de linguagem (aba 2).
// A GUI só MONTA o comando e ABRE UM TERMINAL rodando `schematize env …`; o plano
// exato + consentimento (e o sudo) acontecem no terminal (honesto). NUNCA executa
// o instalador de environment de dentro do processo da GUI.
// ===========================================================================

/// Rótulo de status de um environment — mesmas chaves i18n que o `list()` do CLI usa.
fn env_status_label(le: &environments::LangEnv) -> String {
    if let Some(m) = le.installed {
        tf("env.installed_via", &[("method", m.slug())])
    } else if le.runtime_present {
        t("env.installed")
    } else {
        t("env.not_installed")
    }
}

/// Constrói uma linha da aba Environments a partir do status do lib. O
/// `section_title` fica vazio aqui; quem monta a lista (build_env_rows_from) o
/// preenche na PRIMEIRA linha de cada seção (linguagens × ferramentas).
fn env_row(le: &environments::LangEnv) -> EnvRow {
    let methods: Vec<SharedString> = le.methods_available.iter().map(|m| m.slug().into()).collect();
    let method_sel = methods.first().cloned().unwrap_or_default();
    EnvRow {
        lang: le.lang.into(),
        display: le.display.into(),
        category: le.category.into(),
        install_hint: le.install_hint.as_str().into(),
        section_title: SharedString::new(),
        methods: ModelRc::from(Rc::new(VecModel::from(methods))),
        method_sel,
        installed: le.is_installed(),
        status_label: env_status_label(le).into(),
        op_label: SharedString::new(),
    }
}

/// Título traduzido da seção de uma categoria ("language" | "tool").
fn env_section_title(category: &str) -> String {
    match category {
        "tool" => tor("gui.env_tools_title", "Ferramentas de dev"),
        _ => tor("gui.env_langs_title", "Linguagens"),
    }
}

/// Monta as linhas a partir de um status já sondado, marcando o `section_title`
/// na primeira linha de cada categoria (o `status()` do lib já lista linguagens
/// primeiro e ferramentas depois). Assim a UI renderiza os dois blocos separados.
fn build_env_rows_from(status: &[environments::LangEnv]) -> Vec<EnvRow> {
    let mut last_cat = String::new();
    status
        .iter()
        .map(|le| {
            let mut row = env_row(le);
            if le.category != last_cat {
                last_cat = le.category.to_string();
                row.section_title = env_section_title(le.category).into();
            }
            row
        })
        .collect()
}

/// Constrói o modelo inteiro da aba Environments a partir de `environments::status()`.
fn build_env_rows() -> Vec<EnvRow> {
    build_env_rows_from(&environments::status())
}

// ---------------------------------------------------------------------------
// SSH — modelo da tela de chaves a partir de `sshkeys::list()` (só metadados
// PÚBLICOS; a privada nunca é lida/exposta). Igual ao padrão dos demais modelos.
// ---------------------------------------------------------------------------
fn build_ssh_rows() -> Vec<SshRow> {
    sshkeys::list()
        .into_iter()
        .map(|k| SshRow {
            name: k.name.into(),
            kind: k.kind.into(),
            comment: k.comment.into(),
            fingerprint: k.fingerprint.into(),
            public_path: k.public_path.into(),
            op_label: SharedString::new(),
            op_error: false,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Idiomas p/ o seletor de Configurações (código + nome nativo + marca do atual).
// ---------------------------------------------------------------------------
fn build_lang_items(current: &str) -> Vec<LangItem> {
    i18n::LANGS
        .iter()
        .map(|(code, name, _)| LangItem {
            code: (*code).into(),
            name: (*name).into(),
            current: *code == current,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Formatação p/ o histórico do DB do overdev. Sem crate de data: converte o
// epoch (UTC) via o algoritmo civil de Howard Hinnant.
// ---------------------------------------------------------------------------
fn fmt_ts(ts: i64) -> String {
    if ts <= 0 {
        return "—".into();
    }
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (h, mi) = (secs / 3600, (secs % 3600) / 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Tamanho legível (B / KB / MB).
fn fmt_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Linha de upstream (branch → remote · ↑ahead ↓behind); vazia se sem tracking.
fn fmt_upstream(up: Option<githist::Upstream>) -> String {
    match up {
        Some(u) => {
            let remote = u.remote.unwrap_or_else(|| "—".into());
            format!("{} → {} · ↑{} ↓{}", u.branch, remote, u.ahead, u.behind)
        }
        None => String::new(),
    }
}

/// Tamanho da página das listas paginadas (mercado, histórico do DB, commits).
const PAGE: usize = 20;

/// Uma página do histórico do DB (metadados → SnapRow).
fn snap_rows_page(all: &[overdevdb::SnapshotMeta], page: i32) -> Vec<SnapRow> {
    let start = (page.max(0) as usize) * PAGE;
    all.iter()
        .skip(start)
        .take(PAGE)
        .map(|m| SnapRow {
            id: m.id as i32,
            file: m.file.clone().into(),
            date: fmt_ts(m.ts).into(),
            size: fmt_size(m.size).into(),
            hash: m.hash.chars().take(8).collect::<String>().into(),
        })
        .collect()
}

/// Uma página do histórico de commits (Commit → CommitRow).
fn commit_rows_page(all: &[githist::Commit], page: i32) -> Vec<CommitRow> {
    let start = (page.max(0) as usize) * PAGE;
    all.iter()
        .skip(start)
        .take(PAGE)
        .map(|c| CommitRow {
            short: c.short.clone().into(),
            date: c.date.clone().into(),
            author: c.author.clone().into(),
            subject: c.subject.clone().into(),
            pushed: c.pushed,
        })
        .collect()
}

/// Localiza o binário `schematize` (CLI) pra montar o comando do terminal:
/// primeiro um irmão do executável atual; senão o do PATH.
fn schematize_bin() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("schematize");
            if cand.is_file() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    "schematize".into()
}

/// Um binário existe no PATH?
fn which_bin(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Abre um terminal gráfico rodando `inner` (bash -c). Mesmo padrão do gui.rs (egui):
/// cobre konsole/gnome-terminal/xfce4-terminal/x-terminal-emulator/kitty/alacritty/tilix/xterm.
fn launch_terminal(inner: &str) -> bool {
    let cands: &[(&str, &[&str])] = &[
        ("x-terminal-emulator", &["-e"]),
        ("konsole", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("xfce4-terminal", &["-x"]),
        ("tilix", &["-e"]),
        ("kitty", &[]),
        ("alacritty", &["-e"]),
        ("xterm", &["-e"]),
    ];
    for (term, pre) in cands {
        if which_bin(term)
            && std::process::Command::new(term)
                .args(*pre)
                .arg("bash")
                .arg("-c")
                .arg(inner)
                .spawn()
                .is_ok()
        {
            return true;
        }
    }
    false
}

/// Monta o comando do terminal p/ `schematize env <action> <lang> --method <m>`.
/// SEM `--yes`: o CLI mostra o plano e PEDE confirmação ali dentro (consentimento honesto).
fn env_terminal_inner(bin: &str, action: &str, lang: &str, method: &str) -> String {
    // Ferramentas não têm método (o CLI ignora `--method` pra elas) → omite o flag
    // quando `method` vem vazio, pra não passar um `--method ` sem valor.
    let (tag, method_arg) = if method.is_empty() {
        (String::new(), String::new())
    } else {
        (format!(" ({method})"), format!(" --method {method}"))
    };
    format!(
        "echo '── schematize env {action} {lang}{tag} ──'; echo; \
         {bin} env {action} {lang}{method_arg}; \
         echo; read -n1 -s -r -p '…'",
        action = action,
        lang = lang,
        tag = tag,
        method_arg = method_arg,
        bin = bin
    )
}

/// Dispara o terminal p/ uma ação de environment e devolve o rótulo transitório a exibir
/// na linha (terminal aberto, ou instrução manual quando nenhum terminal foi encontrado).
fn run_env_action(action: &str, lang: &str, method: &str) -> String {
    let bin = schematize_bin();
    let inner = env_terminal_inner(&bin, action, lang, method);
    if launch_terminal(&inner) {
        t("gui.env_terminal_opened")
    } else {
        let method_arg = if method.is_empty() { String::new() } else { format!(" --method {method}") };
        let cmd = format!("{bin} env {action} {lang}{method_arg}");
        tf("gui.env_no_terminal", &[("cmd", &cmd)])
    }
}

/// Uma skill (por slug) está instalada AGORA? (lê o modelo de linhas de skills.)
fn slug_installed(model: &VecModel<SkillRow>, slug: &str) -> bool {
    for i in 0..model.row_count() {
        if let Some(r) = model.row_data(i) {
            if !r.is_header && r.slug == slug {
                return r.state != "missing";
            }
        }
    }
    false
}

/// Índice da linha de uma skill (por slug) no vetor de itens alinhado ao modelo.
fn row_idx_of_slug(row_items: &[Option<Item>], slug: &str) -> Option<usize> {
    row_items
        .iter()
        .position(|m| m.as_ref().map(|it| it.slug == slug).unwrap_or(false))
}

/// Estado do modal de instalação do Marketplace, guardado no lado Rust (o Slint
/// carrega só o visual). Preenchido ao abrir; lido no confirmar.
#[derive(Default, Clone)]
struct ModalState {
    skill_idx: usize,   // linha da skill sendo instalada
    rec_slug: String,   // slug da recomendada a oferecer ("" = nenhuma)
    env_lang: String,   // linguagem do environment a oferecer ("" = nenhum)
}

// ===========================================================================
// OVERDEV — seletor de projeto (porte do project_bar do egui) + view nativa
// (porte do overdev_view). Lê `schematize::projects::scan()` + `config` p/ o
// seletor e `schematize::panel::load_overdev()` p/ o estado. Persiste a escolha
// via `config::add_recent_project`.
// ===========================================================================

/// Basename de um caminho como String (fallback: o caminho inteiro).
fn basename_of(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// Cabeçalho de grupo do seletor (Detectados / Recentes).
fn proj_header(label: &str) -> ProjItem {
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
fn build_proj_items(projects: &[projects::Project], recent: &[String]) -> Vec<ProjItem> {
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

/// Caminho do CHECKLIST de `<root>/.overdev`.
fn checklist_path(root: &Path) -> PathBuf {
    root.join(".overdev").join("CHECKLIST.md")
}

/// Caminho de um arquivo do editor (`PLAN.md`/`CHECKLIST.md`) sob `<root>/.overdev`.
/// Sanitiza `target` a um basename simples pra a GUI nunca escrever fora do `.overdev`.
fn overdev_file_path(root: &Path, target: &str) -> PathBuf {
    let name = Path::new(target).file_name().and_then(|s| s.to_str()).unwrap_or("PLAN.md");
    root.join(".overdev").join(name)
}

/// Parseia o CHECKLIST 2-níveis de `<root>` em `OverItem`s (kind + origem + índice).
/// Casa `- [H ...]` ANTES de `- [ ]`/`- [x]` (senão o humano cai no ramo de máquina).
/// `hindex` numera 1-based só os HUMANOS ABERTOS (- [H ]) — é o arg de `od-mark-human`.
fn parse_checklist_items(root: &Path) -> Vec<OverItem> {
    let cl = std::fs::read_to_string(checklist_path(root)).unwrap_or_default();
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
fn mark_human_done_at(root: &Path, index: i32) -> Result<(), String> {
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
fn refresh_proj_models(
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
fn load_overdev_into(app: &AppWindow, items: &VecModel<OverItem>, proj: Option<&Path>) {
    let Some(p) = proj else {
        app.set_od_has_project(false);
        app.set_od_has_overdev(false);
        app.set_od_current(SharedString::new());
        app.set_od_editor_content(SharedString::new());
        app.set_od_editor_status(SharedString::new());
        app.set_od_notes(SharedString::new());
        items.set_vec(Vec::new());
        return;
    };
    app.set_od_has_project(true);
    app.set_od_current(basename_of(p).into());
    let ov = panel::load_overdev(p);
    // Checklist 2-níveis parseado direto (o panel::load_overdev do lib ignora os
    // marcadores humanos `- [H ]`/`- [H x]`; aqui a GUI precisa deles).
    let its = parse_checklist_items(p);
    // Sem run: objetivo vazio E sem itens (mesma regra do egui).
    let has = !(ov.objetivo.trim().is_empty() && its.is_empty());
    app.set_od_has_overdev(has);
    if !has {
        items.set_vec(Vec::new());
        app.set_od_editor_content(SharedString::new());
        app.set_od_editor_status(SharedString::new());
        app.set_od_notes(SharedString::new());
        return;
    }
    // Contagem 4-categorias derivada do MESMO parse 2-níveis (máquina-abertos/feitos/
    // on-hold/humanos-abertos). `done` = feitos totais (máquina `- [x]` + humano
    // `- [H x]`), como o `Counts::done()` do engine do lib.
    let (mut done, mut open, mut hold, mut human) = (0i32, 0i32, 0i32, 0i32);
    for it in &its {
        match it.kind.as_str() {
            "done" | "hdone" => done += 1,
            "open" => open += 1,
            "hold" => hold += 1,
            "hopen" => human += 1,
            _ => {}
        }
    }
    app.set_od_objetivo(ov.objetivo.clone().into());
    app.set_od_mode(ov.mode.clone().into());
    app.set_od_done(done);
    app.set_od_open(open);
    app.set_od_hold(hold);
    app.set_od_human_open(human);
    app.set_od_decisoes(ov.decisoes.clone().into());
    app.set_od_plano(ov.plano.clone().into());
    app.set_od_perguntas(ov.perguntas.clone().into());
    items.set_vec(its);
    // Editor (arquivo atualmente escolhido) + notas do humano.
    load_editor_content(app, p);
    app.set_od_notes(overdev::read_notes(p).into());
}

/// Carrega no editor o conteúdo do arquivo escolhido (`od-editor-target`) de `<root>/.overdev`.
/// Limpa o feedback de status. Arquivo ausente → editor vazio (o Salvar cria).
fn load_editor_content(app: &AppWindow, root: &Path) {
    let target = app.get_od_editor_target().to_string();
    let content = std::fs::read_to_string(overdev_file_path(root, &target)).unwrap_or_default();
    app.set_od_editor_content(content.into());
    app.set_od_editor_status(SharedString::new());
    app.set_od_editor_error(false);
}

/// Escolhe um projeto: canoniza, persiste como recente e carrega o overdev.
fn select_project(
    app: &AppWindow,
    items: &VecModel<OverItem>,
    proj_model: &VecModel<ProjItem>,
    dev_model: &VecModel<SharedString>,
    pin_model: &VecModel<SharedString>,
    cur: &RefCell<Option<PathBuf>>,
    path: PathBuf,
) {
    let abs = std::fs::canonicalize(&path).unwrap_or(path);
    config::add_recent_project(&abs.to_string_lossy());
    *cur.borrow_mut() = Some(abs.clone());
    load_overdev_into(app, items, Some(&abs));
    // reflete o novo recente no seletor.
    refresh_proj_models(proj_model, dev_model, pin_model);
}

// ===========================================================================
// FASE 4 — overdev em TERMINAL EXTERNO + MONITOR leve. DECISÃO do dono: o `claude`
// NÃO roda mais acoplado (PTY) dentro do app — carregava o LOAD dele aqui (RAM,
// inchaço tipo VSCode) e nem submetia na TUI. Agora o botão só chama
// `agentrun::launch_in_terminal` (processo PRÓPRIO num terminal do sistema,
// DESACOPLADO) e o app apenas MONITORA o `.overdev/` a cada ~3s (só LÊ arquivos,
// não segura processo). O "não pare" é imposto pelo Stop hook do overdev — nada
// de auto-continue/nudge aqui. Só `String`/`PathBuf`/`Weak<AppWindow>` +
// `Arc<AtomicBool>` cruzam a fronteira de thread; a UI é tocada por `post_*`.
// ===========================================================================

/// Intervalo do monitor leve do `.overdev/` (só relê arquivos de progresso).
const OD_MONITOR_EVERY: Duration = Duration::from_secs(3);

/// Teto de itens abertos listados no monitor (o `claude` roda fora; isto é só espelho).
const OD_MONITOR_ITEMS: usize = 10;

/// Intervalo MÍNIMO entre leituras de `usage::agent_usage` dentro do monitor.
/// CUIDADO PERF: `agent_usage` parseia os `.jsonl` do Claude (100MB+) — jamais a
/// cada ciclo. Relemos os tokens no máx. a cada 30s (e sempre em thread própria).
const OD_USAGE_EVERY: Duration = Duration::from_secs(30);

/// Agrupa milhares com `.` (separador pt-BR): 1234567 → "1.234.567". PURO.
fn sep_thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push('.');
        }
        out.push(*b as char);
    }
    out
}

/// Converte um epoch (segundos) pra `HH:MM:SS` na hora LOCAL via `chrono::Local`.
/// Fallback improvável (timestamp fora de faixa) → string vazia.
fn fmt_ts_local(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Materializa as conclusões (`overdev::completions`) em linhas prontas pra UI:
/// `HH:MM:SS  <texto>` (hora local). Mantém a ordem do lib (ts asc → recentes embaixo).
fn fmt_completions(cs: Vec<overdev::Completion>) -> Vec<String> {
    cs.into_iter()
        .map(|c| {
            let hhmmss = fmt_ts_local(c.ts);
            if hhmmss.is_empty() {
                c.text
            } else {
                format!("{hhmmss}  {}", c.text)
            }
        })
        .collect()
}

/// Monta a linha de tokens do painel do monitor a partir de `usage::Usage`.
/// "Tokens: <total> (in <in> / out <out> · cache-read <cr>) · Modelo: <main>".
fn fmt_usage(u: &usage::Usage) -> String {
    let model = u.main_model().unwrap_or("—");
    format!(
        "{}: {} (in {} / out {} · cache-read {}) · {}: {}",
        tor("gui.od_tokens", "Tokens"),
        sep_thousands(u.total),
        sep_thousands(u.input),
        sep_thousands(u.output),
        sep_thousands(u.cache_read),
        tor("gui.od_model", "Modelo"),
        model,
    )
}

/// thread→UI: espelha o log de conclusões (linhas já formatadas `HH:MM:SS texto`).
fn post_completions(weak: &Weak<AppWindow>, lines: Vec<String>) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            let rows: Vec<SharedString> = lines.into_iter().map(SharedString::from).collect();
            app.set_od_completions(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

/// thread→UI: escreve a linha de tokens/modelo já formatada.
fn post_usage(weak: &Weak<AppWindow>, line: String) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.set_od_usage_line(line.into());
        }
    });
}

/// Lê `usage::agent_usage` (PESADO: parseia .jsonl de 100MB+) numa thread PRÓPRIA e
/// posta a linha formatada. Nunca no event loop, nunca no ritmo de 3s do monitor.
fn spawn_usage(weak: Weak<AppWindow>, project: PathBuf) {
    std::thread::spawn(move || {
        let u = usage::agent_usage(&project);
        post_usage(&weak, fmt_usage(&u));
    });
}

/// thread→UI: espelha o snapshot do `.overdev/` (estado + contadores + iterações +
/// lista de itens abertos). Cria um `VecModel` novo pra a lista (roda na UI thread).
fn post_monitor(weak: &Weak<AppWindow>, prog: overdev::Progress, items: Vec<String>) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.set_od_run_done(prog.done as i32);
            app.set_od_run_open(prog.open as i32);
            app.set_od_mon_human(prog.human as i32);
            app.set_od_mon_hold(prog.hold as i32);
            app.set_od_mon_iter(prog.iterations as i32);
            app.set_od_mon_max(prog.max_iters as i32);
            app.set_od_mon_mode(prog.mode.into());
            let rows: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
            app.set_od_mon_items(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

/// thread→UI: FIM do monitor — larga o flag de "monitorando", fixa o modo final e
/// re-sonda o projeto (`od-reload`) pra o checklist/contagem refletirem o disco.
fn post_monitor_end(weak: &Weak<AppWindow>, mode: String) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.set_od_session_running(false);
            app.set_od_mon_mode(mode.into());
            app.invoke_od_reload();
        }
    });
}

/// MONITOR leve: a cada ~3s lê `overdev::progress_at` + `open_items_at` + o log de
/// `overdev::completions` (tudo BARATO) e espelha na UI; os tokens (`agent_usage`,
/// PESADO) só no arranque e a cada ~30s, sempre em thread própria. NÃO segura o
/// processo do `claude` (ele roda no terminal externo). Para quando o botão Parar
/// levanta a `stop`, ou quando o run some/termina (`mode == "stopped"` /
/// `Progress::finished()`), MAS só depois de ter visto o run ficar `active` uma vez —
/// assim um `state.json` velho ("stopped") não encerra o monitor antes de o overdev
/// sequer arrancar no terminal.
///
/// `attach`: quando `true` (botão "Reload / Acompanhar", anexando a um run que já
/// roda POR FORA), começamos com `seen_active = true` — assim um run já em
/// andamento (mode "active") é seguido de imediato e um run já FINALIZADO
/// ("stopped") posta o snapshot final uma vez e encerra, em vez de exigir que o
/// monitor testemunhe a transição pra active (que já aconteceu antes de anexarmos).
fn run_monitor(weak: Weak<AppWindow>, project: PathBuf, stop: Arc<AtomicBool>, attach: bool) {
    std::thread::spawn(move || {
        let mut seen_active = attach;
        // Arranque: tokens uma vez (thread própria) — o resto é relido a cada ciclo.
        spawn_usage(weak.clone(), project.clone());
        let mut last_usage = Instant::now();
        loop {
            if stop.load(Ordering::SeqCst) {
                post_monitor_end(&weak, "stopped".into());
                return;
            }
            let prog = overdev::progress_at(&project);
            let items = overdev::open_items_at(&project, OD_MONITOR_ITEMS);
            post_completions(&weak, fmt_completions(overdev::completions(&project)));
            // Tokens: no MÁX. a cada 30s, sempre fora do event loop.
            if last_usage.elapsed() >= OD_USAGE_EVERY {
                spawn_usage(weak.clone(), project.clone());
                last_usage = Instant::now();
            }
            let mode = prog.mode.clone();
            let finished = prog.finished();
            if mode == "active" {
                seen_active = true;
            }
            post_monitor(&weak, prog, items);
            if seen_active && (mode == "stopped" || finished) {
                post_monitor_end(&weak, if mode.is_empty() { "stopped".into() } else { mode });
                return;
            }
            // Dorme em fatias curtas pra responder rápido ao botão Parar.
            let mut slept = Duration::ZERO;
            while slept < OD_MONITOR_EVERY {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
                slept += Duration::from_millis(200);
            }
        }
    });
}

// ===========================================================================
// GRAFO — porte do graph_view + step_graph do egui (schematize_cli_rs::gui).
// A física é force-directed em Rust (repulsão O(n²) + molas nas arestas +
// gravidade/centralização + damping), rodada num `slint::Timer` a ~60fps que
// RELAXA (para) quando a energia (alpha) cai. As posições (mundo) vivem aqui; a
// cada tick empurramos os `VecModel` de nós/arestas que o `.slint` renderiza. A
// transformação (scale/ox/oy) também é dona daqui (o Slint só a lê pra desenhar).
// Interação (pan/zoom/arrasto/clique) chega crua do Slint; o hit-test é aqui.
// ===========================================================================

/// Raio de um nó EM PIXELS de tela (constante ao zoom) — idêntico ao egui.
fn nsize(deg: f32) -> f32 {
    3.0 + (deg.sqrt() * 1.7).min(9.0)
}

/// Rótulo curto do nó (id truncado, seguro a UTF-8) — o egui truncava em 33+"…".
fn trunc_label(id: &str) -> String {
    if id.chars().count() > 34 {
        let s: String = id.chars().take(33).collect();
        format!("{s}…")
    } else {
        id.to_string()
    }
}

/// Nó com estado de simulação + flags de realce (recomputadas em `refresh_flags`).
struct GNode {
    id: String,
    loc: Option<String>,
    label: String,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    deg: f32,
    selected: bool,
    hot: bool, // vizinho do selecionado OU casa com a busca
    dim: bool, // apagado (fora do foco/da busca)
}

/// Estado inteiro do grafo (dono no Rust). Um por app; compartilhado via Rc<RefCell>.
#[derive(Default)]
struct GraphState {
    nodes: Vec<GNode>,
    edges: Vec<(usize, usize)>,
    project: Option<PathBuf>,
    sel: Option<usize>,
    search: String,
    scale: f32,
    ox: f32,
    oy: f32,
    alpha: f32,
    drag_node: Option<usize>,
    drag_off: (f32, f32),
    last_ptr: (f32, f32),
    moved: bool,
    canvas_w: f32,
    canvas_h: f32,
    fit_pending: bool,
}

impl GraphState {
    /// Um passo da física (idêntico ao `step_graph` do egui). No-op se relaxado.
    fn step(&mut self) {
        if self.alpha < 0.02 {
            return;
        }
        let n = self.nodes.len();
        const REP: f32 = 1400.0;
        const SPR: f32 = 0.02;
        const LEN: f32 = 70.0;
        const G: f32 = 0.015;
        for i in 0..n {
            let (xi, yi) = (self.nodes[i].x, self.nodes[i].y);
            let (mut fx, mut fy) = (0.0f32, 0.0f32);
            for j in 0..n {
                if i == j {
                    continue;
                }
                let dx = xi - self.nodes[j].x;
                let dy = yi - self.nodes[j].y;
                let d2 = dx * dx + dy * dy + 0.01;
                let d = d2.sqrt();
                let f = REP / d2;
                fx += f * dx / d;
                fy += f * dy / d;
            }
            fx -= xi * G;
            fy -= yi * G;
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
    fn to_world(&self, mx: f32, my: f32) -> (f32, f32) {
        let cx = self.canvas_w / 2.0;
        let cy = self.canvas_h / 2.0;
        ((mx - cx - self.ox) / self.scale, (my - cy - self.oy) / self.scale)
    }

    /// Nó sob o ponto de mundo (raio de tela convertido pra mundo) — como o egui.
    fn hit(&self, wx: f32, wy: f32) -> Option<usize> {
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
    fn fit(&mut self) {
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
    fn refresh_flags(&mut self) {
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

/// Constrói uma linha do modelo de nós a partir do estado de simulação.
fn graph_node_row(n: &GNode) -> GraphNode {
    GraphNode {
        id: n.id.clone().into(),
        label: n.label.clone().into(),
        x: n.x,
        y: n.y,
        r: nsize(n.deg),
        selected: n.selected,
        hot: n.hot,
        dim: n.dim,
        has_loc: n.loc.is_some(),
    }
}

/// Constrói uma linha do modelo de arestas (pontas em mundo + realce).
fn graph_edge_row(st: &GraphState, a: usize, b: usize) -> GraphEdge {
    let on = st.sel == Some(a) || st.sel == Some(b);
    GraphEdge {
        x1: st.nodes[a].x,
        y1: st.nodes[a].y,
        x2: st.nodes[b].x,
        y2: st.nodes[b].y,
        on,
    }
}

/// Empurra TUDO pro Slint: transformação (props), seleção (info) e os dois
/// VecModel (nós/arestas). Atualiza in-place quando o tamanho casa (sem realloc
/// no loop da física); senão troca o vec inteiro (carga/relayout).
fn graph_sync(app: &AppWindow, st: &GraphState, nodes: &VecModel<GraphNode>, edges: &VecModel<GraphEdge>) {
    app.set_g_scale(st.scale);
    app.set_g_ox(st.ox);
    app.set_g_oy(st.oy);
    app.set_g_has_graph(!st.nodes.is_empty());
    app.set_g_node_count(st.nodes.len() as i32);
    match st.sel {
        Some(i) => {
            app.set_g_has_sel(true);
            app.set_g_sel_id(st.nodes[i].id.clone().into());
            app.set_g_sel_loc(st.nodes[i].loc.clone().unwrap_or_default().into());
        }
        None => {
            app.set_g_has_sel(false);
            app.set_g_sel_id(SharedString::new());
            app.set_g_sel_loc(SharedString::new());
        }
    }
    if nodes.row_count() == st.nodes.len() {
        for (i, n) in st.nodes.iter().enumerate() {
            nodes.set_row_data(i, graph_node_row(n));
        }
    } else {
        nodes.set_vec(st.nodes.iter().map(graph_node_row).collect::<Vec<_>>());
    }
    if edges.row_count() == st.edges.len() {
        for (i, &(a, b)) in st.edges.iter().enumerate() {
            edges.set_row_data(i, graph_edge_row(st, a, b));
        }
    } else {
        edges.set_vec(st.edges.iter().map(|&(a, b)| graph_edge_row(st, a, b)).collect::<Vec<_>>());
    }
}

/// Carrega o grafo do `proj` (ou limpa se None) no estado. Espelha o
/// `reload_project` do egui: posições em espiral inicial, graus, e fit pendente.
fn load_graph_into(st: &mut GraphState, proj: Option<&Path>) {
    st.nodes.clear();
    st.edges.clear();
    st.sel = None;
    st.search.clear();
    st.drag_node = None;
    st.moved = false;
    st.scale = 1.0;
    st.ox = 0.0;
    st.oy = 0.0;
    st.alpha = 1.0;
    st.project = proj.map(|p| p.to_path_buf());
    let Some(p) = proj else {
        return;
    };
    let (nodes, edges, _idx) = panel::load_graph(p);
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.clone(), i);
        let a = i as f32 * 2.399_963; // ângulo áureo → espiral inicial (evita sobreposição)
        let r = 40.0 + 9.0 * (i as f32).sqrt();
        st.nodes.push(GNode {
            id: n.id.clone(),
            loc: n.loc.clone(),
            label: trunc_label(&n.id),
            x: a.cos() * r,
            y: a.sin() * r,
            vx: 0.0,
            vy: 0.0,
            deg: 0.0,
            selected: false,
            hot: false,
            dim: false,
        });
    }
    for e in &edges {
        if let (Some(&a), Some(&b)) = (id_to_idx.get(&e.from), id_to_idx.get(&e.to)) {
            st.edges.push((a, b));
            st.nodes[a].deg += 1.0;
            st.nodes[b].deg += 1.0;
        }
    }
    st.fit_pending = true;
    st.refresh_flags();
}

/// (Re)liga o Timer da física se estiver parado. O tick roda um passo, sincroniza,
/// e PARA (relaxa) quando alpha < 0.02 — não queima CPU parado. Reinicia via arrasto/carga.
fn graph_kick(
    timer: &Rc<slint::Timer>,
    weak: Weak<AppWindow>,
    state: Rc<RefCell<GraphState>>,
    nodes: Rc<VecModel<GraphNode>>,
    edges: Rc<VecModel<GraphEdge>>,
) {
    if timer.running() {
        return;
    }
    let timer2 = timer.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(app) = weak.upgrade() else {
            timer2.stop();
            return;
        };
        let mut st = state.borrow_mut();
        st.step();
        graph_sync(&app, &st, &nodes, &edges);
        if st.alpha < 0.02 {
            timer2.stop();
        }
    });
}

/// Carrega o grafo do projeto no estado, sincroniza e (re)liga a física.
fn graph_load_and_kick(
    proj: Option<&Path>,
    timer: &Rc<slint::Timer>,
    weak: &Weak<AppWindow>,
    state: &Rc<RefCell<GraphState>>,
    nodes: &Rc<VecModel<GraphNode>>,
    edges: &Rc<VecModel<GraphEdge>>,
) {
    load_graph_into(&mut state.borrow_mut(), proj);
    if let Some(app) = weak.upgrade() {
        graph_sync(&app, &state.borrow(), nodes, edges);
    }
    graph_kick(timer, weak.clone(), state.clone(), nodes.clone(), edges.clone());
}

/// (Re)carrega o histórico do DB do overdev + os commits do projeto `proj` nos
/// modelos, reseta a paginação e escreve a linha de upstream. None → limpa tudo.
/// Síncrono (sqlite/git locais e rápidos — mesma escolha do env status).
fn refresh_od_history(
    app: &AppWindow,
    snaps_all: &RefCell<Vec<overdevdb::SnapshotMeta>>,
    snaps_model: &VecModel<SnapRow>,
    commits_all: &RefCell<Vec<githist::Commit>>,
    commits_model: &VecModel<CommitRow>,
    proj: Option<&Path>,
) {
    match proj {
        Some(p) => {
            let snaps = overdevdb::history(p, 50).unwrap_or_default();
            let commits = githist::commits(p, 50);
            app.set_od_upstream_line(fmt_upstream(githist::upstream(p)).into());
            app.set_od_snap_total(snaps.len() as i32);
            app.set_od_commit_total(commits.len() as i32);
            app.set_od_snap_page(0);
            app.set_od_commit_page(0);
            snaps_model.set_vec(snap_rows_page(&snaps, 0));
            commits_model.set_vec(commit_rows_page(&commits, 0));
            *snaps_all.borrow_mut() = snaps;
            *commits_all.borrow_mut() = commits;
        }
        None => {
            app.set_od_upstream_line(SharedString::new());
            app.set_od_snap_total(0);
            app.set_od_commit_total(0);
            app.set_od_snap_page(0);
            app.set_od_commit_page(0);
            snaps_model.set_vec(Vec::new());
            commits_model.set_vec(Vec::new());
            snaps_all.borrow_mut().clear();
            commits_all.borrow_mut().clear();
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    detect_display_env();

    let items = registry::catalog();
    eprintln!("[catalog] {} skills (via schematize::registry::catalog)", items.len());
    let (rows, row_items) = build_rows(&items);
    let row_items = Rc::new(row_items);
    let model = Rc::new(VecModel::from(rows));

    let app = AppWindow::new()?;
    install_i18n(&app);
    // Logo da janela (título/taskbar) — mesma marca do egui.
    app.set_app_icon(make_app_icon());
    // Versão do app (Configurações) — ex.: "schematize v0.25.1".
    app.set_app_version(format!("schematize v{}", upgrade::app_version()).into());
    app.set_rows(ModelRc::from(model.clone()));
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
    app.set_env_rows(ModelRc::from(env_model.clone()));
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

    // ---- relançar o app (janela nova) — conserto do restart pós self-update ----
    // O callback existe e está ligado ao helper CORRETO (spawn desacoplado antes
    // de sair). Hoje o self-update NÃO está fiado nesta GUI Slint (mora no módulo
    // egui do lib, fora do alcance daqui), então nada dispara `restart()` ainda;
    // quando o self-update for portado pra cá, é só invocar `root.restart()`.
    app.on_restart(move || restart_app());

    // ---- toggle de seleção de uma linha ----
    {
        let model = model.clone();
        app.on_toggle(move |idx| {
            let i = idx as usize;
            if let Some(mut row) = model.row_data(i) {
                row.selected = !row.selected;
                model.set_row_data(i, row);
            }
        });
    }

    // ---- selecionar todas (da PÁGINA ativa) ----
    // Instaladas (tab 0) → todas as instaladas; Marketplace (tab 1) → todas as
    // não-instaladas. Não toca em linhas da outra página.
    {
        let weak = app.as_weak();
        let model = model.clone();
        app.on_select_all(move || {
            let tab = weak.upgrade().map(|a| a.get_active_tab()).unwrap_or(0);
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if r.is_header {
                        continue;
                    }
                    let on_page = if tab == 1 { is_missing(&r) } else { is_installed(&r) };
                    if on_page && !r.selected {
                        r.selected = true;
                        model.set_row_data(i, r);
                    }
                }
            }
        });
    }
    // ---- selecionar pendentes (só Instaladas): instaladas-DESATUALIZADAS ----
    {
        let model = model.clone();
        app.on_select_pending(move || {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if !r.is_header {
                        r.selected = is_outdated(&r); // nunca marca uma não-instalada
                        model.set_row_data(i, r);
                    }
                }
            }
        });
    }
    {
        let model = model.clone();
        app.on_select_none(move || {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if !r.is_header && r.selected {
                        r.selected = false;
                        model.set_row_data(i, r);
                    }
                }
            }
        });
    }

    // ---- abrir o site do autor (sponsor.url) ----
    {
        let model = model.clone();
        app.on_open_author(move |idx| {
            if let Some(r) = model.row_data(idx as usize) {
                if !r.author_url.is_empty() {
                    util::open_url(&r.author_url);
                }
            }
        });
    }

    // ---- Marketplace: ação por-linha INSTALAR ----
    // Skill de linguagem (ou skill com recommends) → abre o MODAL: oferece instalar
    // a recomendada (base) junto E, opcionalmente, o environment da linguagem (via
    // terminal). Skill sem nada a oferecer → instala direto (um clique).
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        let model = model.clone();
        let env_langs = env_langs.clone();
        let env_methods = env_methods.clone();
        let modal = modal.clone();
        app.on_row_install(move |idx| {
            let i = idx as usize;
            let Some(Some(it)) = row_items.get(i) else {
                return;
            };
            // recomendada a oferecer: 1ª recomendada que NÃO está instalada.
            let rec_slug = it
                .recommends
                .iter()
                .find(|s| !slug_installed(&model, s.as_str()))
                .cloned()
                .unwrap_or_default();
            // environment a oferecer: se o slug da skill é uma das 7 linguagens.
            let env_lang = if env_langs.contains(it.slug.as_str()) {
                it.slug.clone()
            } else {
                String::new()
            };
            // Nada a oferecer → instala direto, sem modal.
            if rec_slug.is_empty() && env_lang.is_empty() {
                run_batch(weak.clone(), vec![(i, true, it.clone())]);
                return;
            }
            let Some(app) = weak.upgrade() else { return };
            *modal.borrow_mut() = ModalState {
                skill_idx: i,
                rec_slug: rec_slug.clone(),
                env_lang: env_lang.clone(),
            };
            app.set_mp_title(tf("gui.mp_install_title", &[("slug", &it.slug)]).into());
            app.set_mp_idx(i as i32);
            // dependência opcional (base recomendada) — NUNCA marcada por padrão.
            let rec_show = !rec_slug.is_empty();
            app.set_mp_rec_show(rec_show);
            app.set_mp_rec_check(false);
            if rec_show {
                app.set_mp_rec_label(tf("gui.mp_with_recommended", &[("slug", &rec_slug)]).into());
            }
            // environment opcional — NUNCA marcado por padrão.
            let env_show = !env_lang.is_empty();
            app.set_mp_env_show(env_show);
            app.set_mp_env_check(false);
            if env_show {
                app.set_mp_env_label(tf("gui.mp_with_env", &[("lang", &env_lang)]).into());
                let methods: Vec<SharedString> = env_methods
                    .get(&env_lang)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|m| m.into())
                    .collect();
                let sel = methods.first().cloned().unwrap_or_default();
                app.set_mp_methods(ModelRc::from(Rc::new(VecModel::from(methods))));
                app.set_mp_method_sel(sel);
            }
            app.set_mp_open(true);
        });
    }

    // ---- Instaladas: ação por-linha ATUALIZAR ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_row_update(move |idx| {
            let i = idx as usize;
            if let Some(Some(it)) = row_items.get(i) {
                run_batch(weak.clone(), vec![(i, true, it.clone())]);
            }
        });
    }

    // ---- Instaladas: ação por-linha DESINSTALAR ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_row_remove(move |idx| {
            let i = idx as usize;
            if let Some(Some(it)) = row_items.get(i) {
                run_batch(weak.clone(), vec![(i, false, it.clone())]);
            }
        });
    }

    // ---- Marketplace: INSTALAR selecionadas (só as não-instaladas) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_install_selected(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, true, |r| r.selected && is_missing(r));
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- Instaladas: ATUALIZAR selecionadas (só instaladas-desatualizadas) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_update_selected(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, true, |r| r.selected && is_outdated(r));
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- Instaladas: DESINSTALAR selecionadas (só instaladas) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_remove_selected(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, false, |r| r.selected && is_installed(r));
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- Instaladas: ATUALIZAR TUDO ----
    // GARANTIA: só instaladas-DESATUALIZADAS (is_outdated ⟺ installed Some E
    // latest > installed). JAMAIS instala uma skill não instalada.
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_update_all(move || {
            if let Some(app) = weak.upgrade() {
                let ops = collect_ops(&app, &row_items, true, is_outdated);
                run_batch(weak.clone(), ops);
            }
        });
    }

    // ---- rechecar versões (re-resolve latest) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_check(move || {
            kick_resolve_all(&weak, &row_items);
            kick_market_ratings(weak.clone());
        });
    }

    // ==================== aba Environments ====================

    // escolher o método (chip) de uma linha de environment.
    {
        let env_model = env_model.clone();
        app.on_env_pick_method(move |idx, method| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                r.method_sel = method;
                env_model.set_row_data(i, r);
            }
        });
    }
    // instalar o environment da linha → abre TERMINAL com `schematize env install`.
    {
        let env_model = env_model.clone();
        app.on_env_install(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                // Linguagem exige método escolhido; ferramenta ("tool") não tem seletor.
                if r.category != "tool" && r.method_sel.is_empty() {
                    return;
                }
                let label = run_env_action("install", &r.lang.to_string(), &r.method_sel.to_string());
                r.op_label = label.into();
                env_model.set_row_data(i, r);
            }
        });
    }
    // desinstalar o environment da linha → abre TERMINAL com `schematize env remove`.
    {
        let env_model = env_model.clone();
        app.on_env_remove(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                // Linguagem exige método; ferramenta não (o CLI ignora `--method`).
                if r.category != "tool" && r.method_sel.is_empty() {
                    return;
                }
                let label = run_env_action("remove", &r.lang.to_string(), &r.method_sel.to_string());
                r.op_label = label.into();
                env_model.set_row_data(i, r);
            }
        });
    }
    // recarregar o status (re-sonda a máquina). Síncrono (local/rápido; evita !Send).
    {
        let env_model = env_model.clone();
        app.on_env_refresh(move || {
            env_model.set_vec(build_env_rows());
        });
    }

    // ==================== modal de instalação (Marketplace) ====================

    {
        let weak = app.as_weak();
        app.on_mp_toggle_rec(move || {
            if let Some(a) = weak.upgrade() {
                a.set_mp_rec_check(!a.get_mp_rec_check());
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_mp_toggle_env(move || {
            if let Some(a) = weak.upgrade() {
                a.set_mp_env_check(!a.get_mp_env_check());
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_mp_pick_method(move |m| {
            if let Some(a) = weak.upgrade() {
                a.set_mp_method_sel(m);
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_mp_cancel(move || {
            if let Some(a) = weak.upgrade() {
                a.set_mp_open(false);
            }
        });
    }
    // confirmar: instala a skill in-process (+ a base marcada, no MESMO lote paralelo)
    // e, se marcado, dispara o environment num TERMINAL (fora do processo).
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        let modal = modal.clone();
        let env_model = env_model.clone();
        app.on_mp_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let st = modal.borrow().clone();
            // lote in-process: a skill + (recomendada SÓ se o usuário marcou).
            let mut ops: Vec<(usize, bool, Item)> = Vec::new();
            if let Some(Some(it)) = row_items.get(st.skill_idx) {
                ops.push((st.skill_idx, true, it.clone()));
            }
            if app.get_mp_rec_check() && !st.rec_slug.is_empty() {
                if let Some(ridx) = row_idx_of_slug(&row_items, &st.rec_slug) {
                    if let Some(Some(rit)) = row_items.get(ridx) {
                        ops.push((ridx, true, rit.clone()));
                    }
                }
            }
            // environment opcional → terminal (só se marcado + método escolhido).
            let do_env = app.get_mp_env_check() && !st.env_lang.is_empty();
            let env_method = app.get_mp_method_sel().to_string();
            app.set_mp_open(false);
            run_batch(weak.clone(), ops);
            if do_env && !env_method.is_empty() {
                let label = run_env_action("install", &st.env_lang, &env_method);
                app.set_status(SharedString::from(label.clone()));
                // reflete a msg no card correspondente da aba Environments.
                for i in 0..env_model.row_count() {
                    if let Some(mut r) = env_model.row_data(i) {
                        if r.lang == st.env_lang {
                            r.op_label = label.clone().into();
                            env_model.set_row_data(i, r);
                            break;
                        }
                    }
                }
            }
        });
    }

    // ==================== aba Gerenciar (criar + editar skills) ====================
    // Todas as chamadas ao `skilledit` (scaffold/list/read/write) rodam em thread e
    // devolvem à UI via `invoke_from_event_loop` (padrão thread→UI do Slint). O
    // estado do form/editor mora em propriedades do app (nada de Rc !Send cruzando
    // a fronteira da thread — os modelos são REMONTADOS no event loop).
    app.set_mg_skills(strings_model(installed_skill_slugs()));
    app.set_mg_files(strings_model(Vec::new()));

    // re-sondar as skills instaladas (dropdown do modo Editar).
    {
        let weak = app.as_weak();
        app.on_mg_refresh_skills(move || {
            let weak = weak.clone();
            std::thread::spawn(move || {
                let slugs = installed_skill_slugs();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_mg_skills(strings_model(slugs));
                    }
                });
            });
        });
    }

    // validar o slug a cada tecla (puro/rápido — sem IO). Atualiza slug + erro inline.
    {
        let weak = app.as_weak();
        app.on_mg_slug_edited(move |s| {
            if let Some(app) = weak.upgrade() {
                let slug = s.to_string();
                app.set_mg_slug(s);
                // vazio → sem erro (só desabilita o botão); inválido → mostra o hint.
                let err = if slug.is_empty() || skilledit::validate_slug(&slug).is_ok() {
                    String::new()
                } else {
                    tor("gui.slug_invalid", "slug inválido — use só [a-z0-9-], começando por letra/dígito")
                };
                app.set_mg_slug_error(err.into());
            }
        });
    }

    // criar a skill → skilledit::scaffold(slug, nome, desc). Sucesso mostra o caminho
    // e re-sonda o dropdown; erro (ex.: já existe) mostra a mensagem.
    {
        let weak = app.as_weak();
        app.on_mg_create(move || {
            let Some(app) = weak.upgrade() else { return };
            let slug = app.get_mg_slug().to_string();
            let name = app.get_mg_name().to_string();
            let desc = app.get_mg_desc().to_string();
            // trava dupla: valida antes de spawnar (feedback imediato).
            if skilledit::validate_slug(&slug).is_err() {
                app.set_mg_slug_error(
                    tor("gui.slug_invalid", "slug inválido — use só [a-z0-9-], começando por letra/dígito").into(),
                );
                return;
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skilledit::scaffold(&slug, &name, &desc);
                // criou → já re-sonda a lista (passa a incluir a nova skill).
                let slugs = if res.is_ok() { Some(installed_skill_slugs()) } else { None };
                let created = slug.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        match res {
                            Ok(path) => {
                                app.set_mg_create_error(false);
                                app.set_mg_create_result(
                                    format!(
                                        "{} {}",
                                        tor("gui.skill_created", "Skill criada em"),
                                        path.display()
                                    )
                                    .into(),
                                );
                                app.set_mg_created_slug(created.into());
                                if let Some(s) = slugs {
                                    app.set_mg_skills(strings_model(s));
                                }
                            }
                            Err(e) => {
                                app.set_mg_create_error(true);
                                // "já existe" ganha mensagem amigável; senão a msg do lib.
                                let msg = if e.contains("já existe") {
                                    tor("gui.skill_exists", "essa skill já existe")
                                } else {
                                    e
                                };
                                app.set_mg_create_result(msg.into());
                            }
                        }
                    }
                });
            });
        });
    }

    // pós-criar: pula pro modo Editar já com a skill recém-criada carregada.
    {
        let weak = app.as_weak();
        app.on_mg_edit_created(move || {
            let Some(app) = weak.upgrade() else { return };
            let slug = app.get_mg_created_slug().to_string();
            if slug.is_empty() {
                return;
            }
            app.set_mg_mode(1);
            app.invoke_mg_pick_skill(slug.into()); // reusa o pick p/ listar os arquivos
        });
    }

    // escolher uma skill → lista os arquivos editáveis (skilledit::list_files).
    {
        let weak = app.as_weak();
        app.on_mg_pick_skill(move |s| {
            let Some(app) = weak.upgrade() else { return };
            let slug = s.to_string();
            app.set_mg_sel_skill(s);
            // troca de skill zera a seleção de arquivo/editor/feedback.
            app.set_mg_sel_file(SharedString::new());
            app.set_mg_content(SharedString::new());
            app.set_mg_save_result(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                // lista os arquivos + status de FORK (oficial? já forkada?) da skill escolhida.
                let files = skilledit::list_files(&slug).unwrap_or_default();
                let official = skills::is_official(&slug);
                let forked = skills::load_state().skills.get(&slug).map(|e| e.forked).unwrap_or(false);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_mg_files(strings_model(files));
                        app.set_mg_sel_official(official);
                        app.set_mg_sel_forked(forked);
                    }
                });
            });
        });
    }

    // escolher um arquivo → carrega o conteúdo no editor (skilledit::read_file).
    {
        let weak = app.as_weak();
        app.on_mg_pick_file(move |f| {
            let Some(app) = weak.upgrade() else { return };
            let slug = app.get_mg_sel_skill().to_string();
            let rel = f.to_string();
            app.set_mg_sel_file(f);
            app.set_mg_save_result(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skilledit::read_file(&slug, &rel);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        match res {
                            Ok(content) => app.set_mg_content(content.into()),
                            Err(e) => {
                                app.set_mg_content(SharedString::new());
                                app.set_mg_save_error(true);
                                app.set_mg_save_result(tf("err.prefix", &[("e", &e)]).into());
                            }
                        }
                    }
                });
            });
        });
    }

    // salvar o editor → grava LOCAL em ~/.claude/skills (skilledit::write_file).
    // `write_file` já FORKA automaticamente uma skill oficial antes de gravar; após
    // salvar, relemos o estado de fork e refletimos no banner + no badge [fork] da lista.
    {
        let weak = app.as_weak();
        app.on_mg_save(move || {
            let Some(app) = weak.upgrade() else { return };
            let slug = app.get_mg_sel_skill().to_string();
            let rel = app.get_mg_sel_file().to_string();
            let content = app.get_mg_content().to_string();
            if slug.is_empty() || rel.is_empty() {
                return;
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skilledit::write_file(&slug, &rel, &content);
                // pós-gravação: a skill oficial pode ter virado fork agora.
                let forked = res.is_ok()
                    && skills::load_state().skills.get(&slug).map(|e| e.forked).unwrap_or(false);
                let slug2 = slug.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        match res {
                            Ok(()) => {
                                app.set_mg_save_error(false);
                                app.set_mg_save_result(tor("gui.saved", "Salvo").into());
                                app.set_mg_sel_forked(forked);
                                mark_row_forked(&app, &slug2, forked);
                            }
                            Err(e) => {
                                app.set_mg_save_error(true);
                                app.set_mg_save_result(tf("err.prefix", &[("e", &e)]).into());
                            }
                        }
                    }
                });
            });
        });
    }

    // ==================== aba Grafo ====================
    // Estado (dono da física + transformação), dois VecModel (nós/arestas) e o
    // Timer da física. O grafo COMPARTILHA o projeto com a aba Overdev — carregado
    // junto na seleção/restauração de projeto (mais abaixo).
    let graph_state = Rc::new(RefCell::new(GraphState { scale: 1.0, alpha: 1.0, ..Default::default() }));
    let graph_nodes = Rc::new(VecModel::<GraphNode>::from(Vec::new()));
    let graph_edges = Rc::new(VecModel::<GraphEdge>::from(Vec::new()));
    let graph_timer = Rc::new(slint::Timer::default());
    app.set_graph_nodes(ModelRc::from(graph_nodes.clone()));
    app.set_graph_edges(ModelRc::from(graph_edges.clone()));

    // ==================== aba Overdev ====================
    // Modelos: seletor de projeto (detectados + recentes), dev_dirs, e checklist.
    let od_proj_model = Rc::new(VecModel::<ProjItem>::from(Vec::new()));
    let od_dev_model = Rc::new(VecModel::<SharedString>::from(Vec::new()));
    let od_pin_model = Rc::new(VecModel::<SharedString>::from(Vec::new()));
    let od_items_model = Rc::new(VecModel::<OverItem>::from(Vec::new()));
    app.set_od_projects(ModelRc::from(od_proj_model.clone()));
    app.set_od_dev_dirs(ModelRc::from(od_dev_model.clone()));
    app.set_od_pinned(ModelRc::from(od_pin_model.clone()));
    app.set_od_items(ModelRc::from(od_items_model.clone()));
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
    app.set_od_snaps(ModelRc::from(od_snaps_model.clone()));
    app.set_od_commits(ModelRc::from(od_commits_model.clone()));
    // Restaura a última escolha (mais recente), senão empty-state.
    match config::recent_projects().into_iter().next() {
        Some(p) => {
            let abs = std::fs::canonicalize(&p).unwrap_or_else(|_| PathBuf::from(&p));
            *od_current.borrow_mut() = Some(abs.clone());
            load_overdev_into(&app, &od_items_model, Some(&abs));
            refresh_od_history(&app, &od_snaps_all, &od_snaps_model, &od_commits_all, &od_commits_model, Some(&abs));
            // grafo compartilha o projeto restaurado.
            graph_load_and_kick(Some(&abs), &graph_timer, &app.as_weak(), &graph_state, &graph_nodes, &graph_edges);
        }
        None => load_overdev_into(&app, &od_items_model, None),
    }

    // escolher um projeto do seletor.
    {
        let weak = app.as_weak();
        let items = od_items_model.clone();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_od_pick_project(move |path| {
            if path.is_empty() {
                return;
            }
            if let Some(app) = weak.upgrade() {
                select_project(&app, &items, &pm, &dm, &pnm, &cur, PathBuf::from(path.to_string()));
                let p = cur.borrow().clone();
                graph_load_and_kick(p.as_deref(), &gt, &weak, &gs, &gn, &ge);
                app.invoke_od_refresh_history();
            }
        });
    }
    // abrir uma pasta avulsa (picker NATIVO do sistema).
    {
        let weak = app.as_weak();
        let items = od_items_model.clone();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_od_open_folder(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(t("gui.open_folder")).pick_folder() {
                if let Some(app) = weak.upgrade() {
                    select_project(&app, &items, &pm, &dm, &pnm, &cur, dir);
                    let p = cur.borrow().clone();
                    graph_load_and_kick(p.as_deref(), &gt, &weak, &gs, &gn, &ge);
                    app.invoke_od_refresh_history();
                }
            }
        });
    }
    // abrir a pasta do projeto ATUAL no gerenciador de arquivos (xdg-open <root>).
    {
        let cur = od_current.clone();
        app.on_od_open_project_folder(move || {
            if let Some(p) = cur.borrow().as_ref() {
                open_path_in_files(p);
            }
        });
    }
    // abrir o projeto ATUAL no VSCode (`code <root>` / vscode://file/<root>).
    {
        let cur = od_current.clone();
        app.on_od_open_vscode(move || {
            if let Some(p) = cur.borrow().as_ref() {
                open_in_vscode(p);
            }
        });
    }
    // recarregar: re-sonda dev_dirs/projetos e recarrega o projeto atual.
    {
        let weak = app.as_weak();
        let items = od_items_model.clone();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_od_reload(move || {
            refresh_proj_models(&pm, &dm, &pnm);
            if let Some(app) = weak.upgrade() {
                let p = cur.borrow().clone();
                load_overdev_into(&app, &items, p.as_deref());
                graph_load_and_kick(p.as_deref(), &gt, &weak, &gs, &gn, &ge);
                app.invoke_od_refresh_history();
            }
        });
    }
    // cadastrar um diretório de desenvolvimento (picker nativo).
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.on_od_add_dev_dir(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(t("gui.add_dev_dir")).pick_folder() {
                let abs = std::fs::canonicalize(&dir).unwrap_or(dir);
                config::add_dev_dir(&abs.to_string_lossy());
                refresh_proj_models(&pm, &dm, &pnm);
            }
        });
    }
    // remover um diretório de desenvolvimento.
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.on_od_remove_dev_dir(move |path| {
            config::remove_dev_dir(&path.to_string());
            refresh_proj_models(&pm, &dm, &pnm);
        });
    }
    // FIXAR uma pasta como projeto (picker nativo → config::pin_project). Uma pasta
    // fixada vira UM projeto no seletor mesmo sem marcador git (workspace/monorepo).
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.on_od_pin_folder(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(tor("gui.pin_folder", "Fixar pasta…")).pick_folder() {
                let abs = std::fs::canonicalize(&dir).unwrap_or(dir);
                config::pin_project(&abs.to_string_lossy());
                refresh_proj_models(&pm, &dm, &pnm);
            }
        });
    }
    // DESAFIXAR uma pasta fixada (config::unpin_project).
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.on_od_unpin(move |path| {
            config::unpin_project(&path.to_string());
            refresh_proj_models(&pm, &dm, &pnm);
        });
    }
    // abrir o painel HTML do projeto atual no navegador.
    {
        let cur = od_current.clone();
        app.on_od_open_browser(move || {
            if let Some(p) = cur.borrow().as_ref() {
                let _ = panel::open_in_browser(p);
            }
        });
    }
    // ---- Fase 3: marcar item HUMANO aberto como feito (- [H ]→- [H x]) ----
    // Edita o CHECKLIST.md do projeto e recarrega a view (contagem + itens).
    {
        let weak = app.as_weak();
        let items = od_items_model.clone();
        let cur = od_current.clone();
        app.on_od_mark_human(move |idx| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                let _ = mark_human_done_at(&p, idx);
                load_overdev_into(&app, &items, Some(&p));
            }
        });
    }
    // ---- Fase 3: trocar o arquivo do editor (PLAN.md/CHECKLIST.md) ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_editor_pick(move |target| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                app.set_od_editor_target(target);
                load_editor_content(&app, &p);
            }
        });
    }
    // ---- Fase 3: salvar o arquivo do editor (regrava no .overdev/) ----
    // Reflete no checklist/itens se o arquivo salvo for o CHECKLIST.md.
    {
        let weak = app.as_weak();
        let items = od_items_model.clone();
        let cur = od_current.clone();
        app.on_od_editor_save(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                let target = app.get_od_editor_target().to_string();
                let content = app.get_od_editor_content().to_string();
                let path = overdev_file_path(&p, &target);
                match std::fs::write(&path, content) {
                    Ok(()) => {
                        app.set_od_editor_error(false);
                        app.set_od_editor_status(tor("gui.saved", "Salvo").into());
                        // salvar o CHECKLIST.md muda o estado 2-níveis: recarrega.
                        if target == "CHECKLIST.md" {
                            load_overdev_into(&app, &items, Some(&p));
                            app.set_od_editor_error(false);
                            app.set_od_editor_status(tor("gui.saved", "Salvo").into());
                        }
                    }
                    Err(e) => {
                        app.set_od_editor_error(true);
                        app.set_od_editor_status(e.to_string().into());
                    }
                }
            }
        });
    }
    // ---- Fase 3: adicionar ponto/nota por task (add_note kind="task") ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_add_note(move |texto| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if !texto.trim().is_empty() {
                    let _ = overdev::add_note(&p, "task", &texto);
                    app.set_od_notes(overdev::read_notes(&p).into());
                }
            }
        });
    }
    // ---- Fase 3: prompt de correção do overdev (add_note kind="correcao") ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_add_correction(move |texto| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if !texto.trim().is_empty() {
                    let _ = overdev::add_note(&p, "correcao", &texto);
                    app.set_od_notes(overdev::read_notes(&p).into());
                }
            }
        });
    }
    // ---- Fase 4: "Executar overdev" — passo 1: GUARDRAIL (mostra o comando) ----
    // Não dispara nada; só mostra o comando que abrirá no TERMINAL EXTERNO e abre o
    // mini-modal de confirmação. O disparo real (launch_in_terminal) é no `od-run-confirm`.
    {
        let weak = app.as_weak();
        app.on_od_run_request(move || {
            if let Some(app) = weak.upgrade() {
                app.set_od_agent_cmdline(
                    tor(
                        "gui.od_agent_cmdline",
                        "claude --dangerously-skip-permissions \"<prompt do overdev>\"  (terminal externo, processo próprio)",
                    )
                    .into(),
                );
                app.set_od_confirm_open(true);
            }
        });
    }
    // ---- Fase 4: guardrail — CANCELAR (fecha sem disparar) ----
    {
        let weak = app.as_weak();
        app.on_od_run_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.set_od_confirm_open(false);
            }
        });
    }
    // ---- Fase 4: guardrail — CONFIRMAR: abre o `claude` num TERMINAL EXTERNO ----
    // Chama `agentrun::launch_in_terminal` numa thread (processo próprio, RAM dele,
    // fora do app). Sucesso → mensagem "claude aberto no terminal <nome>…" + liga o
    // MONITOR leve. Erro (claude/terminal ausente) → mostra a msg e não monitora.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let stop = od_stop_flag.clone();
        app.on_od_run_confirm(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if app.get_od_session_running() {
                    return; // já monitorando — não dispara outro
                }
                app.set_od_confirm_open(false);
                app.set_od_run_status(SharedString::new());
                app.set_od_run_done(0);
                app.set_od_run_open(0);
                app.set_od_mon_human(0);
                app.set_od_mon_hold(0);
                app.set_od_mon_iter(0);
                app.set_od_mon_max(0);
                app.set_od_mon_mode(SharedString::new());
                app.set_od_mon_items(ModelRc::from(Rc::new(VecModel::<SharedString>::from(Vec::new()))));
                stop.store(false, Ordering::SeqCst);
                let objetivo = overdev::objetivo_at(&p).unwrap_or_default();
                let w = weak.clone();
                let stop2 = stop.clone();
                std::thread::spawn(move || match agentrun::launch_in_terminal(&p, &objetivo) {
                    Ok(term) => {
                        let wu = w.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = wu.upgrade() {
                                app.set_od_session_running(true);
                                let msg = format!(
                                    "{}{}{}",
                                    tor("gui.od_launched_pre", "claude aberto no terminal "),
                                    term,
                                    tor(
                                        "gui.od_launched_post",
                                        " — o overdev roda fora do app; acompanhe abaixo.",
                                    ),
                                );
                                app.set_od_run_status(msg.into());
                            }
                        });
                        run_monitor(w, p, stop2, false);
                    }
                    Err(e) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = w.upgrade() {
                                app.set_od_session_running(false);
                                app.set_od_run_status(e.into());
                            }
                        });
                    }
                });
            }
        });
    }
    // ---- Fase 4: "Parar" — levanta a flag; o MONITOR encerra (não mata o claude) ----
    {
        let weak = app.as_weak();
        let stop = od_stop_flag.clone();
        app.on_od_stop(move || {
            stop.store(true, Ordering::SeqCst);
            if let Some(app) = weak.upgrade() {
                app.set_od_run_status(tor("gui.od_stop", "Parar").into());
            }
        });
    }
    // ---- Reload / Acompanhar: ANEXA o monitor a um overdev que já roda POR FORA ----
    // (terminal/processo próprio). Sem depender de ter clicado "Executar overdev":
    // (re)liga a `run_monitor` no projeto atual lendo o `.overdev/` do disco e passa
    // a espelhar ao vivo. Sem `.overdev/` → avisa e não liga. Se um monitor já está
    // vivo, só reforça os tokens (o loop já reflete o resto). `attach=true` faz o
    // monitor seguir um run já em curso (ou postar 1x e encerrar se já finalizou).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let stop = od_stop_flag.clone();
        app.on_od_attach(move || {
            let root = cur.borrow().clone();
            let Some(app) = weak.upgrade() else { return };
            let Some(p) = root else {
                app.set_od_run_status(tor("gui.od_pick_first", "Selecione um projeto primeiro.").into());
                return;
            };
            if !p.join(".overdev").is_dir() {
                app.set_od_run_status(tor("gui.od_no_overdev_here", "nenhum overdev neste projeto").into());
                return;
            }
            // Já monitorando: não dispara outra thread (evita duplicata na mesma
            // `stop`); só reforça os tokens agora.
            if app.get_od_session_running() {
                spawn_usage(weak.clone(), p.clone());
                app.set_od_run_status(tor("gui.od_attached", "acompanhando o overdev deste projeto…").into());
                return;
            }
            // Zera o painel e liga o monitor anexado ao run externo.
            app.set_od_run_status(tor("gui.od_attached", "acompanhando o overdev deste projeto…").into());
            app.set_od_run_done(0);
            app.set_od_run_open(0);
            app.set_od_mon_human(0);
            app.set_od_mon_hold(0);
            app.set_od_mon_iter(0);
            app.set_od_mon_max(0);
            app.set_od_mon_mode(SharedString::new());
            app.set_od_mon_items(ModelRc::from(Rc::new(VecModel::<SharedString>::from(Vec::new()))));
            app.set_od_session_running(true);
            stop.store(false, Ordering::SeqCst);
            run_monitor(weak.clone(), p, stop.clone(), true);
        });
    }
    // ---- "Atualizar tokens": relê `agent_usage` (PESADO) sob demanda, em thread ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_refresh_tokens(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if p.join(".overdev").is_dir() {
                    spawn_usage(weak.clone(), p);
                } else {
                    app.set_od_run_status(tor("gui.od_no_overdev_here", "nenhum overdev neste projeto").into());
                }
            }
        });
    }

    // ==================== aba Grafo — interação ====================
    // Ponteiro/roda chegam crus do Slint (coords relativas ao canvas, em px). O
    // hit-test e a decisão pan-vs-arrasto acontecem AQUI (como no egui). Cada
    // handler sincroniza o modelo/props no fim.

    // canvas mudou de tamanho → guarda; se havia fit pendente (carga), enquadra.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_canvas_resized(move |w, h| {
            let mut st = gs.borrow_mut();
            st.canvas_w = w;
            st.canvas_h = h;
            if st.fit_pending && w > 1.0 && h > 1.0 {
                st.fit();
                st.fit_pending = false;
            }
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // mouse-down: hit-test → fixa o nó a arrastar (com offset de pega) ou nada.
    {
        let gs = graph_state.clone();
        app.on_graph_press(move |mx, my| {
            let mut st = gs.borrow_mut();
            let (wx, wy) = st.to_world(mx, my);
            st.last_ptr = (mx, my);
            st.moved = false;
            match st.hit(wx, wy) {
                Some(i) => {
                    st.drag_node = Some(i);
                    st.drag_off = (st.nodes[i].x - wx, st.nodes[i].y - wy);
                }
                None => st.drag_node = None,
            }
        });
    }
    // arrasto: nó fixo → move o nó (reaquece a física); senão → pan do fundo.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        let gt = graph_timer.clone();
        app.on_graph_move(move |mx, my| {
            let need_kick;
            {
                let mut st = gs.borrow_mut();
                let (dx, dy) = (mx - st.last_ptr.0, my - st.last_ptr.1);
                if dx.abs() + dy.abs() > 3.0 {
                    st.moved = true;
                }
                match st.drag_node {
                    Some(i) => {
                        let (wx, wy) = st.to_world(mx, my);
                        st.nodes[i].x = wx + st.drag_off.0;
                        st.nodes[i].y = wy + st.drag_off.1;
                        st.nodes[i].vx = 0.0;
                        st.nodes[i].vy = 0.0;
                        if st.alpha < 0.3 {
                            st.alpha = 0.3;
                        }
                        need_kick = true;
                    }
                    None => {
                        st.ox += dx;
                        st.oy += dy;
                        need_kick = false;
                    }
                }
                st.last_ptr = (mx, my);
                if let Some(app) = weak.upgrade() {
                    graph_sync(&app, &st, &gn, &ge);
                }
            }
            if need_kick {
                graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
            }
        });
    }
    // mouse-up: se não houve arrasto, é um CLIQUE → seleciona/deseleciona o nó.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_release(move || {
            let mut st = gs.borrow_mut();
            if !st.moved {
                let (wx, wy) = st.to_world(st.last_ptr.0, st.last_ptr.1);
                st.sel = st.hit(wx, wy);
                st.refresh_flags();
            }
            st.drag_node = None;
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // roda: zoom centrado no cursor (mesma matemática do egui).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_scroll(move |mx, my, dy| {
            let mut st = gs.borrow_mut();
            let cx = st.canvas_w / 2.0;
            let cy = st.canvas_h / 2.0;
            let m = (mx - cx - st.ox, my - cy - st.oy);
            let f = (dy * 0.0015).exp();
            st.ox -= m.0 * (f - 1.0);
            st.oy -= m.1 * (f - 1.0);
            st.scale = (st.scale * f).clamp(0.12, 6.0);
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // botão "ajustar": enquadra tudo no canvas.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_fit(move || {
            let mut st = gs.borrow_mut();
            st.fit();
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // busca: realça/apaga nós por nome (recomputa flags e sincroniza).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_search_changed(move |s| {
            let mut st = gs.borrow_mut();
            st.search = s.to_string();
            st.refresh_flags();
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    // clique em "abrir no editor" do nó selecionado → vscode://file/<abs>/…:<linha>.
    {
        let gs = graph_state.clone();
        app.on_graph_open_editor(move || {
            let st = gs.borrow();
            if let (Some(i), Some(proj)) = (st.sel, st.project.clone()) {
                if let Some(loc) = st.nodes[i].loc.clone() {
                    let abs = std::fs::canonicalize(&proj).unwrap_or(proj);
                    util::open_url(&format!("vscode://file/{}/{}", abs.to_string_lossy(), loc));
                }
            }
        });
    }
    // exportar vault Obsidian do índice (bônus — via panel::export_obsidian_at).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        app.on_graph_export(move || {
            let proj = gs.borrow().project.clone();
            if let Some(p) = proj {
                match panel::export_obsidian_at(&p, None) {
                    Ok(dir) => {
                        if let Some(app) = weak.upgrade() {
                            app.set_status(tf("gui.exported", &[("p", &dir.to_string_lossy())]).into());
                        }
                        util::open_url(&dir.to_string_lossy());
                    }
                    Err(e) => {
                        if let Some(app) = weak.upgrade() {
                            app.set_status(tf("err.prefix", &[("e", &e)]).into());
                        }
                    }
                }
            }
        });
    }
    // abrir o painel HTML (com o mesmo grafo) no navegador (bônus).
    {
        let gs = graph_state.clone();
        app.on_graph_open_browser(move || {
            let proj = gs.borrow().project.clone();
            if let Some(p) = proj {
                let _ = panel::open_in_browser(&p);
            }
        });
    }

    // ==================== Paginação do Mercado ====================
    // Recomputa os índices de exibição (disp) quando a sub-aba muda.
    {
        let weak = app.as_weak();
        app.on_mkt_recompute(move || {
            if let Some(app) = weak.upgrade() {
                recompute_pagination(&app);
            }
        });
    }

    // ==================== Tela SSH (chaves) ====================
    let ssh_model = Rc::new(VecModel::<SshRow>::from(build_ssh_rows()));
    app.set_ssh_rows(ModelRc::from(ssh_model.clone()));
    // re-sonda ~/.ssh.
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_refresh(move || {
            m.set_vec(build_ssh_rows());
            if let Some(app) = weak.upgrade() {
                app.set_ssh_gen_status(SharedString::new());
                app.set_ssh_gen_proof(SharedString::new());
                app.set_ssh_bw_result(SharedString::new());
            }
        });
    }
    // exportar uma chave p/ o Bitwarden (cofre destravado OU arquivo de import 600).
    // Roda em THREAD (bw/subprocesso pode bloquear); só o resultado (String, Send)
    // volta pela event loop. A chave PRIVADA nunca chega à UI (o lib a esconde).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_export_bw(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let i = idx as usize;
            let Some(mut r) = m.row_data(i) else { return };
            let name = r.name.to_string();
            // marca a linha como ocupada e limpa o banner anterior.
            r.op_label = tor("gui.ssh_bw_exporting", "exportando…").into();
            r.op_error = false;
            m.set_row_data(i, r);
            app.set_ssh_bw_result(SharedString::new());
            let weak2 = app.as_weak();
            std::thread::spawn(move || {
                let res = sshkeys::export_bitwarden(&name, None);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak2.upgrade() {
                        // solta o "ocupado" da linha (o modelo é o mesmo VecModel).
                        let rows = app.get_ssh_rows();
                        if let Some(mut r) = rows.row_data(i) {
                            r.op_label = SharedString::new();
                            r.op_error = false;
                            rows.set_row_data(i, r);
                        }
                        match res {
                            Ok(msg) => {
                                app.set_ssh_bw_result(msg.into());
                                app.set_ssh_bw_error(false);
                            }
                            Err(e) => {
                                app.set_ssh_bw_result(e.into());
                                app.set_ssh_bw_error(true);
                            }
                        }
                    }
                });
            });
        });
    }
    // gerar um par (ed25519/rsa). NUNCA sobrescreve (force=false).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_generate(move || {
            let Some(app) = weak.upgrade() else { return };
            let name = app.get_ssh_gen_name().to_string();
            let kind_s = app.get_ssh_gen_kind().to_string();
            let comment = app.get_ssh_gen_comment().to_string();
            let pass = app.get_ssh_gen_passphrase().to_string();
            if let Err(e) = sshkeys::valid_name(&name) {
                app.set_ssh_gen_error(true);
                app.set_ssh_gen_status(e.into());
                return;
            }
            let kind = match sshkeys::KeyKind::parse(&kind_s) {
                Ok(k) => k,
                Err(e) => {
                    app.set_ssh_gen_error(true);
                    app.set_ssh_gen_status(e.into());
                    return;
                }
            };
            let comment_opt = if comment.trim().is_empty() { None } else { Some(comment.as_str()) };
            let pass_opt = if pass.is_empty() { None } else { Some(pass.as_str()) };
            match sshkeys::generate(&name, kind, comment_opt, pass_opt, false) {
                Ok(info) => {
                    app.set_ssh_gen_error(false);
                    app.set_ssh_gen_status(format!("{} · {}", info.name, info.fingerprint).into());
                    // PROVA da chave: bits · fingerprint · tipo (ssh-keygen -l). Confere a força.
                    let proof = sshkeys::proof_line(&info.name).unwrap_or_default();
                    app.set_ssh_gen_proof(proof.into());
                    app.set_ssh_gen_name(SharedString::new());
                    app.set_ssh_gen_comment(SharedString::new());
                    app.set_ssh_gen_passphrase(SharedString::new());
                    m.set_vec(build_ssh_rows());
                }
                Err(e) => {
                    app.set_ssh_gen_error(true);
                    app.set_ssh_gen_status(e.into());
                    app.set_ssh_gen_proof(SharedString::new());
                }
            }
        });
    }
    // copiar a PÚBLICA (export_public + clipboard). NUNCA toca a privada.
    {
        let m = ssh_model.clone();
        app.on_ssh_copy(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = m.row_data(i) {
                let name = r.name.to_string();
                match sshkeys::export_public(&name) {
                    Ok(pubtext) => {
                        let ok = sshkeys::copy_to_clipboard(&pubtext);
                        r.op_label = if ok {
                            tor("gui.ssh_copied", "copiado").into()
                        } else {
                            tor("gui.ssh_copy_fail", "sem clipboard (instale wl-copy/xclip)").into()
                        };
                        r.op_error = !ok;
                    }
                    Err(e) => {
                        r.op_label = e.into();
                        r.op_error = true;
                    }
                }
                m.set_row_data(i, r);
            }
        });
    }
    // pedir remoção → abre o modal de confirmação.
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_remove_request(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            if let Some(r) = m.row_data(idx as usize) {
                let name = r.name.to_string();
                app.set_ssh_confirm_name(name.clone().into());
                app.set_ssh_confirm_msg(
                    format!(
                        "{} '{}'? {}",
                        tor("gui.ssh_remove_confirm", "Remover a chave"),
                        name,
                        tor("gui.ssh_remove_note", "Isto apaga o par (privada + pública).")
                    )
                    .into(),
                );
                app.set_ssh_confirm_open(true);
            }
        });
    }
    // confirmar remoção (remove o par).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_remove_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let name = app.get_ssh_confirm_name().to_string();
            app.set_ssh_confirm_open(false);
            if !name.is_empty() {
                match sshkeys::remove(&name) {
                    Ok(()) => m.set_vec(build_ssh_rows()),
                    Err(e) => {
                        app.set_ssh_gen_error(true);
                        app.set_ssh_gen_status(e.into());
                    }
                }
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_ssh_remove_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.set_ssh_confirm_open(false);
            }
        });
    }

    // ==================== Tela Configurações ====================
    let cur_lang = i18n::current_code();
    let cfg_lang_model = Rc::new(VecModel::<LangItem>::from(build_lang_items(&cur_lang)));
    app.set_cfg_langs(ModelRc::from(cfg_lang_model.clone()));
    app.set_cfg_lang_code(cur_lang.clone().into());
    app.set_cfg_lang_name(i18n::name_of(&cur_lang).unwrap_or("").into());
    app.set_cfg_autostart_on(autostart::is_active());
    app.set_cfg_hooks_on(settings::overdev_enabled());
    // trocar idioma AO VIVO: persiste + recarrega TODOS os rótulos estáticos (L).
    {
        let weak = app.as_weak();
        let lm = cfg_lang_model.clone();
        app.on_cfg_set_lang(move |code| {
            let Some(app) = weak.upgrade() else { return };
            let c = code.to_string();
            if i18n::set_lang(&c).is_ok() {
                install_i18n(&app);
                app.set_cfg_lang_code(c.clone().into());
                app.set_cfg_lang_name(i18n::name_of(&c).unwrap_or("").into());
                lm.set_vec(build_lang_items(&c));
            }
        });
    }
    // autostart do agente (systemd --user + XDG). exe = binário do CLI schematize.
    {
        let weak = app.as_weak();
        app.on_cfg_toggle_autostart(move || {
            let Some(app) = weak.upgrade() else { return };
            let on = app.get_cfg_autostart_on();
            let res = if on { autostart::disable() } else { autostart::enable(&schematize_bin()) };
            app.set_cfg_autostart_on(if res.is_ok() { !on } else { autostart::is_active() });
        });
    }
    // hooks do overdev no settings.json do Claude Code.
    {
        let weak = app.as_weak();
        app.on_cfg_toggle_hooks(move || {
            let Some(app) = weak.upgrade() else { return };
            let on = app.get_cfg_hooks_on();
            let res = if on { settings::disable() } else { settings::enable(&schematize_bin()) };
            app.set_cfg_hooks_on(if res.is_ok() { !on } else { settings::overdev_enabled() });
        });
    }
    // atalho: reusa o modal de diretórios de dev / projetos fixados.
    {
        let weak = app.as_weak();
        app.on_cfg_manage_dirs(move || {
            if let Some(app) = weak.upgrade() {
                app.set_dev_modal_open(true);
            }
        });
    }

    // ==================== Overdev — histórico DB + commits ====================
    // recarrega o histórico do projeto atual (chamado ao entrar na tela / reload).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let sa = od_snaps_all.clone();
        let sm = od_snaps_model.clone();
        let ca = od_commits_all.clone();
        let cm = od_commits_model.clone();
        app.on_od_refresh_history(move || {
            if let Some(app) = weak.upgrade() {
                let p = cur.borrow().clone();
                refresh_od_history(&app, &sa, &sm, &ca, &cm, p.as_deref());
            }
        });
    }
    // paginação do histórico do DB.
    {
        let weak = app.as_weak();
        let all = od_snaps_all.clone();
        let m = od_snaps_model.clone();
        app.on_od_snap_page_prev(move || {
            if let Some(app) = weak.upgrade() {
                let p = (app.get_od_snap_page() - 1).max(0);
                app.set_od_snap_page(p);
                m.set_vec(snap_rows_page(&all.borrow(), p));
            }
        });
    }
    {
        let weak = app.as_weak();
        let all = od_snaps_all.clone();
        let m = od_snaps_model.clone();
        app.on_od_snap_page_next(move || {
            if let Some(app) = weak.upgrade() {
                let p = app.get_od_snap_page() + 1;
                if (p as usize) * PAGE < all.borrow().len() {
                    app.set_od_snap_page(p);
                    m.set_vec(snap_rows_page(&all.borrow(), p));
                }
            }
        });
    }
    // Ver: conteúdo do snapshot num visor read-only.
    {
        let weak = app.as_weak();
        app.on_od_snap_view(move |id| {
            let Some(app) = weak.upgrade() else { return };
            match overdevdb::get(id as i64) {
                Ok(content) => {
                    app.set_od_snap_view_title(format!("snapshot #{id}").into());
                    app.set_od_snap_view_content(content.into());
                }
                Err(e) => {
                    app.set_od_snap_view_title(format!("snapshot #{id}").into());
                    app.set_od_snap_view_content(e.into());
                }
            }
            app.set_od_snap_view_open(true);
        });
    }
    // Restaurar: pede confirmação.
    {
        let weak = app.as_weak();
        app.on_od_snap_restore_request(move |id| {
            let Some(app) = weak.upgrade() else { return };
            app.set_od_snap_confirm_id(id);
            app.set_od_snap_confirm_msg(
                format!("{} #{id}?", tor("gui.od_restore_confirm", "Restaurar o snapshot")).into(),
            );
            app.set_od_snap_confirm_open(true);
        });
    }
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_snap_restore_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let id = app.get_od_snap_confirm_id();
            app.set_od_snap_confirm_open(false);
            let root = cur.borrow().clone();
            if let (Some(p), true) = (root, id >= 0) {
                match overdevdb::restore(id as i64, &p) {
                    Ok(dest) => app.set_od_run_status(
                        format!("{} {}", tor("gui.od_restored", "restaurado:"), dest.display()).into(),
                    ),
                    Err(e) => app.set_od_run_status(e.into()),
                }
                // recarrega overdev (checklist) + histórico refletindo o disco.
                app.invoke_od_reload();
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_od_snap_restore_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.set_od_snap_confirm_open(false);
            }
        });
    }
    // paginação dos commits.
    {
        let weak = app.as_weak();
        let all = od_commits_all.clone();
        let m = od_commits_model.clone();
        app.on_od_commit_page_prev(move || {
            if let Some(app) = weak.upgrade() {
                let p = (app.get_od_commit_page() - 1).max(0);
                app.set_od_commit_page(p);
                m.set_vec(commit_rows_page(&all.borrow(), p));
            }
        });
    }
    {
        let weak = app.as_weak();
        let all = od_commits_all.clone();
        let m = od_commits_model.clone();
        app.on_od_commit_page_next(move || {
            if let Some(app) = weak.upgrade() {
                let p = app.get_od_commit_page() + 1;
                if (p as usize) * PAGE < all.borrow().len() {
                    app.set_od_commit_page(p);
                    m.set_vec(commit_rows_page(&all.borrow(), p));
                }
            }
        });
    }

    // ==================== Versão do app + self-update ====================
    // "Verificar atualização" → app_update_available() em thread; se há versão nova,
    // acende o botão "Atualizar app" que roda selfupdate::run() em thread e, ao
    // concluir, sugere reiniciar (o restart já existe: relança a janela nova).
    {
        let weak = app.as_weak();
        app.on_app_check_update(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_app_checking() || app.get_app_updating() {
                return;
            }
            app.set_app_checking(true);
            app.set_app_update_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = upgrade::app_update_available();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_app_checking(false);
                        match res {
                            Some((_cur, new)) => {
                                app.set_app_has_update(true);
                                app.set_app_update_status(
                                    format!("{} v{new}", tor("gui.app_new_version", "Nova versão disponível:")).into(),
                                );
                            }
                            None => {
                                app.set_app_has_update(false);
                                app.set_app_update_status(tor("gui.app_up_to_date", "Você está atualizado").into());
                            }
                        }
                    }
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        app.on_app_do_update(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_app_updating() {
                return;
            }
            app.set_app_updating(true);
            app.set_app_update_status(tor("gui.app_updating", "Atualizando…").into());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = selfupdate::run();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_app_updating(false);
                        match res {
                            Ok(msg) => {
                                app.set_app_update_done(true);
                                app.set_app_has_update(false);
                                app.set_app_update_status(msg.into());
                            }
                            Err(e) => {
                                app.set_app_update_status(tf("err.prefix", &[("e", &e)]).into());
                            }
                        }
                    }
                });
            });
        });
    }

    // ==================== Sininho de notificações ====================
    // Os modelos (Global/Pessoal) são REMONTADOS no event loop a cada abertura (não
    // cruzam a fronteira da thread — padrão thread→UI do resto da GUI). A ação de
    // cada item viaja pelo próprio callback (kind, action), sem estado Rust extra.
    app.set_notif_global(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));
    app.set_notif_personal(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));

    // recompute só a contagem (badge) — barato de disparar, roda em thread.
    {
        let weak = app.as_weak();
        app.on_notif_refresh(move || {
            let weak = weak.clone();
            std::thread::spawn(move || {
                let n = notifications::count();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_notif_count(n as i32);
                    }
                });
            });
        });
    }
    // abrir o painel: mostra loading e colhe collect() em thread; ao voltar, monta
    // os dois modelos (Global/Pessoal) no event loop e atualiza o badge.
    {
        let weak = app.as_weak();
        app.on_notif_toggle(move || {
            let Some(app) = weak.upgrade() else { return };
            let open = !app.get_notif_open();
            app.set_notif_open(open);
            if !open {
                return;
            }
            app.set_notif_loading(true);
            let weak = weak.clone();
            std::thread::spawn(move || {
                let notifs = notifications::collect();
                // extrai o que cruza a fronteira da thread (tudo String/bool/Send).
                let rows: Vec<(bool, String, String, String, String, bool)> = notifs
                    .iter()
                    .map(|n| {
                        (
                            matches!(n.scope, notifications::NotifScope::Global),
                            n.title.clone(),
                            n.body.clone(),
                            n.kind.clone(),
                            n.action.clone().unwrap_or_default(),
                            n.action.is_some(),
                        )
                    })
                    .collect();
                let total = rows.len();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        let mut gv: Vec<NotifItem> = Vec::new();
                        let mut pv: Vec<NotifItem> = Vec::new();
                        for (idx, (global, title, body, kind, action, has_action)) in rows.into_iter().enumerate() {
                            let item = NotifItem {
                                idx: idx as i32,
                                scope: if global { "global".into() } else { "personal".into() },
                                title: title.into(),
                                body: body.into(),
                                kind: kind.into(),
                                action: action.into(),
                                has_action,
                            };
                            if global {
                                gv.push(item);
                            } else {
                                pv.push(item);
                            }
                        }
                        app.set_notif_global(ModelRc::from(Rc::new(VecModel::from(gv))));
                        app.set_notif_personal(ModelRc::from(Rc::new(VecModel::from(pv))));
                        app.set_notif_total(total as i32);
                        app.set_notif_count(total as i32);
                        app.set_notif_loading(false);
                    }
                });
            });
        });
    }
    // executar a ação de uma notificação — (kind, action) vêm do próprio item.
    {
        let weak = app.as_weak();
        app.on_notif_action(move |kind, action| {
            let Some(app) = weak.upgrade() else { return };
            match kind.as_str() {
                // nova versão do app → fecha o painel, vai pra Configurações e dispara o update.
                "app_update" => {
                    app.set_notif_open(false);
                    app.set_screen(5);
                    app.set_app_has_update(true);
                    app.invoke_app_do_update();
                }
                // post do blog → abre a URL no navegador.
                "news" => {
                    let url = action.to_string();
                    if !url.is_empty() {
                        util::open_url(&url);
                    }
                }
                // skill desatualizada → leva pra aba Instaladas do Mercado.
                "skill_outdated" => {
                    app.set_notif_open(false);
                    app.set_screen(1);
                    app.set_active_tab(0);
                    app.set_mkt_page(0);
                    recompute_pagination(&app);
                }
                _ => {}
            }
        });
    }
    // contagem inicial + refresh periódico (a cada 90s) do badge, em thread.
    app.invoke_notif_refresh();
    let notif_timer = Rc::new(slint::Timer::default());
    {
        let weak = app.as_weak();
        notif_timer.start(TimerMode::Repeated, Duration::from_secs(90), move || {
            if let Some(app) = weak.upgrade() {
                app.invoke_notif_refresh();
            }
        });
    }

    // ==================== Comparar fork vs oficial ====================
    // "Comparar com oficial" → compare_update(slug) em thread; abre o painel com
    // base→nova, arquivos (status) e o diff. NÃO sobrescreve nada.
    app.set_cmp_files(ModelRc::from(Rc::new(VecModel::<CmpFile>::from(Vec::new()))));
    {
        let weak = app.as_weak();
        app.on_cmp_request(move |slug| {
            let Some(app) = weak.upgrade() else { return };
            let slug = slug.to_string();
            if slug.is_empty() {
                return;
            }
            app.set_cmp_open(true);
            app.set_cmp_loading(true);
            app.set_cmp_error(SharedString::new());
            app.set_cmp_diff(SharedString::new());
            app.set_cmp_versions(SharedString::new());
            app.set_cmp_slug(slug.clone().into());
            app.set_cmp_title(format!("{} {slug}", tor("gui.compare_title", "Comparar:")).into());
            app.set_cmp_files(ModelRc::from(Rc::new(VecModel::<CmpFile>::from(Vec::new()))));
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skills::compare_update(&slug);
                // extrai os campos (String/bool) antes de cruzar pro event loop.
                let out: Result<(String, String, Vec<(String, String)>, String), String> =
                    res.map(|c| {
                        (
                            c.base_version,
                            c.new_version,
                            c.files.into_iter().map(|f| (f.path, f.status)).collect(),
                            c.diff_text,
                        )
                    });
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_cmp_loading(false);
                        match out {
                            Ok((base, new, files, diff)) => {
                                app.set_cmp_versions(format!("v{base} → v{new}").into());
                                app.set_cmp_diff(if diff.trim().is_empty() {
                                    tor("gui.compare_identical", "(sem diferenças de conteúdo)").into()
                                } else {
                                    diff.into()
                                });
                                app.set_cmp_files(ModelRc::from(Rc::new(VecModel::from(
                                    files
                                        .into_iter()
                                        .map(|(path, status)| CmpFile { path: path.into(), status: status.into() })
                                        .collect::<Vec<CmpFile>>(),
                                ))));
                            }
                            Err(e) => app.set_cmp_error(e.into()),
                        }
                    }
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        app.on_cmp_close(move || {
            if let Some(app) = weak.upgrade() {
                app.set_cmp_open(false);
            }
        });
    }

    // ==================== Conta (login via device flow) ====================
    // Estado da sessão + fluxo de login OAuth device flow. `device_start` e o loop
    // de `device_poll_once` são REDE — rodam numa thread (nunca bloqueiam o event
    // loop); a UI é tocada só via `invoke_from_event_loop`. O loop é CANCELÁVEL: o
    // flag corrente vive num `Rc<RefCell<Arc<AtomicBool>>>` (padrão do worker do
    // overdev). Cada login troca por um flag NOVO e levanta o antigo, encerrando
    // qualquer thread remanescente; Cancelar/Sair levantam o flag corrente.
    // Só dados `Send` (String/PathBuf/Arc) cruzam a fronteira.
    let acc_stop: Rc<RefCell<Arc<AtomicBool>>> = Rc::new(RefCell::new(Arc::new(AtomicBool::new(false))));
    // Estado inicial: reflete a sessão persistida em disco.
    app.set_acc_logged_in(account::is_logged_in());
    app.set_acc_sub(account::account_sub().unwrap_or_default().into());

    // iniciar o device flow.
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.on_acc_login(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_acc_polling() {
                return; // já há um login em andamento
            }
            // levanta o flag antigo (encerra thread remanescente) e cria um novo.
            acc_stop.borrow().store(true, Ordering::SeqCst);
            let stop = Arc::new(AtomicBool::new(false));
            *acc_stop.borrow_mut() = stop.clone();
            app.set_acc_polling(true);
            app.set_acc_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                match account::device_start() {
                    Err(e) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak.upgrade() {
                                app.set_acc_polling(false);
                                app.set_acc_status(
                                    format!("{} {e}", tor("gui.acc_start_error", "Falha ao iniciar o login:")).into(),
                                );
                            }
                        });
                    }
                    Ok(dl) => {
                        // Mostra o código + a URL e abre o modal.
                        let user_code = dl.user_code.clone();
                        let verification_uri = dl.verification_uri.clone();
                        let verification_complete = dl.verification_uri_complete.clone();
                        {
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(app) = weak.upgrade() {
                                    app.set_acc_user_code(user_code.into());
                                    app.set_acc_verification_uri(verification_uri.into());
                                    app.set_acc_verification_uri_complete(verification_complete.into());
                                    app.set_acc_status(SharedString::new());
                                    app.set_acc_modal_open(true);
                                }
                            });
                        }
                        // Loop de poll (respeita interval/expires_in; cancelável via `stop`).
                        let start = Instant::now();
                        let mut interval = dl.interval.max(1);
                        loop {
                            if stop.load(Ordering::SeqCst) {
                                return; // cancelado/substituído — a UI já foi tratada
                            }
                            if start.elapsed().as_secs() >= dl.expires_in {
                                let weak = weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        app.set_acc_modal_open(false);
                                        app.set_acc_polling(false);
                                        app.set_acc_status(
                                            tor("gui.acc_expired", "O código expirou. Tente novamente.").into(),
                                        );
                                    }
                                });
                                return;
                            }
                            // dorme `interval` em passos de 1s pra reagir rápido ao cancelamento.
                            let mut slept = 0u64;
                            while slept < interval {
                                if stop.load(Ordering::SeqCst) {
                                    return;
                                }
                                std::thread::sleep(Duration::from_secs(1));
                                slept += 1;
                            }
                            match account::device_poll_once(&dl.device_code) {
                                Ok(account::PollResult::Pending) => {}
                                Ok(account::PollResult::SlowDown) => interval += 5,
                                Ok(account::PollResult::Denied) => {
                                    let weak = weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = weak.upgrade() {
                                            app.set_acc_modal_open(false);
                                            app.set_acc_polling(false);
                                            app.set_acc_status(
                                                tor("gui.acc_denied", "Acesso negado. Tente novamente.").into(),
                                            );
                                        }
                                    });
                                    return;
                                }
                                Ok(account::PollResult::Expired) => {
                                    let weak = weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = weak.upgrade() {
                                            app.set_acc_modal_open(false);
                                            app.set_acc_polling(false);
                                            app.set_acc_status(
                                                tor("gui.acc_expired", "O código expirou. Tente novamente.").into(),
                                            );
                                        }
                                    });
                                    return;
                                }
                                Ok(account::PollResult::Ok(tokens)) => {
                                    let sub = tokens.sub.clone();
                                    let save_err = account::save_tokens(&tokens).err();
                                    let weak = weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = weak.upgrade() {
                                            app.set_acc_modal_open(false);
                                            app.set_acc_polling(false);
                                            match save_err {
                                                None => {
                                                    app.set_acc_logged_in(true);
                                                    app.set_acc_sub(sub.into());
                                                    app.set_acc_status(SharedString::new());
                                                    // recomputa o badge do sino (notificações do
                                                    // servidor aparecem quando logado).
                                                    app.invoke_notif_refresh();
                                                }
                                                Some(e) => app.set_acc_status(
                                                    format!("{} {e}", tor("gui.acc_save_error", "Falha ao salvar a sessão:")).into(),
                                                ),
                                            }
                                        }
                                    });
                                    return;
                                }
                                // erro de rede transitório: mantém o poll (não derruba o fluxo).
                                Err(_) => {}
                            }
                        }
                    }
                }
            });
        });
    }

    // abrir a verification_uri_complete no navegador.
    {
        let weak = app.as_weak();
        app.on_acc_open_verify(move || {
            if let Some(app) = weak.upgrade() {
                let url = app.get_acc_verification_uri_complete().to_string();
                if !url.is_empty() {
                    util::open_url(&url);
                }
            }
        });
    }

    // cancelar o login (para o loop de poll + fecha o modal).
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.on_acc_cancel_login(move || {
            acc_stop.borrow().store(true, Ordering::SeqCst);
            if let Some(app) = weak.upgrade() {
                app.set_acc_modal_open(false);
                app.set_acc_polling(false);
                app.set_acc_status(SharedString::new());
            }
        });
    }

    // sair (logout): encerra a sessão + atualiza a UI + recomputa o badge do sino.
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.on_acc_logout(move || {
            // por segurança, para qualquer poll em andamento.
            acc_stop.borrow().store(true, Ordering::SeqCst);
            account::logout();
            if let Some(app) = weak.upgrade() {
                app.set_acc_logged_in(false);
                app.set_acc_sub(SharedString::new());
                app.set_acc_polling(false);
                app.set_acc_modal_open(false);
                app.set_acc_status(SharedString::new());
                // notificações do servidor somem quando deslogado → recomputa o badge.
                app.invoke_notif_refresh();
            }
        });
    }

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
        let root = std::path::Path::new("/proj");
        assert_eq!(overdev_file_path(root, "PLAN.md"), root.join(".overdev").join("PLAN.md"));
        assert_eq!(overdev_file_path(root, "CHECKLIST.md"), root.join(".overdev").join("CHECKLIST.md"));
        // tentativa de path traversal é reduzida ao basename.
        assert_eq!(overdev_file_path(root, "../../etc/passwd"), root.join(".overdev").join("passwd"));
    }
}
