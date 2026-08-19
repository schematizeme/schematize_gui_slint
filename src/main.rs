//! schematize â aba Skills em Slint (1Âº incremento REAL da migraÃ§Ã£o eguiâSlint).
//!
//! O quÃª: a aba Skills como GESTOR de verdade. Reusa a LÃGICA do crate irmÃ£o
//! `schematize` (sem a GUI egui): catÃ¡logo (`registry::catalog`), versÃµes e
//! aÃ§Ãµes (`skills::installed_version` / `resolve_latest` / `install` / `remove`),
//! e i18n (`schematize::i18n`, 11 locales). O visual Ã© Slint (ver `ui/app.slint`).
//!
//! AssÃ­ncrono: `resolve_latest` e as aÃ§Ãµes (instalar/remover) sÃ£o REDE/IO â rodam
//! em threads e devolvem resultado Ã  UI via `slint::invoke_from_event_loop` +
//! `Weak<AppWindow>::upgrade` (o padrÃ£o do Slint pra threadâUI). O event loop
//! nunca bloqueia. As aÃ§Ãµes em massa disparam em PARALELO (thread::scope),
//! espelhando o `run_batch` do egui; o lib serializa o `state.json` (STATE_LOCK).
//!
//! Escopo deste incremento: SÃ a aba Skills funcional. Overdev/Grafo ficam como
//! placeholders "em breve" na barra de abas (prÃ³ximos incrementos).

use schematize::agentrun;
use schematize::i18n::{self, t, tf};
use schematize::registry::{self, Item};
use schematize::{
    account, autostart, config, database, debugreport, environments, githist, market, notifications,
    overdev, overdevdb, panel, projects, selfupdate, settings, skilledit, skills, sshkeys, upgrade,
    usage, util,
};
use slint::{Model, ModelRc, SharedString, TimerMode, VecModel, Weak};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::unix::process::CommandExt; // process_group: desacopla o restart
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

slint::include_modules!(); // gera AppWindow, SkillRow, Theme, L a partir de ui/app.slint

// ---------------------------------------------------------------------------
// DetecÃ§Ã£o de ambiente grÃ¡fico (Wayland vs X11 + desktop). SÃ³ loga â o backend
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
    eprintln!("[env] servidor grÃ¡fico : {server}");
    eprintln!("[env] XDG_SESSION_TYPE : {}", if session.is_empty() { "?".into() } else { session });
    eprintln!("[env] desktop          : {desktop}");
    eprintln!("[env] idioma i18n      : {}", i18n::current_code());
    eprintln!("[env] backend Slint    : winit (default) â cobre Wayland E X11; renderer femtovg (OpenGL/GLES)");
    if wayland.is_none() && x11.is_none() {
        eprintln!("[env] AVISO: sem display, a janela nÃ£o abre. Este incremento valida COMPILAÃÃO; a exibiÃ§Ã£o precisa de um servidor Wayland/X11.");
    }
}

/// Traduz uma chave; se ela AINDA nÃ£o existe no lib (o `t()` do lib devolve a
/// prÃ³pria chave quando nÃ£o acha), cai no `fallback` embutido. Usado sÃ³ para as
/// chaves NOVAS desta fase (Home/navegaÃ§Ã£o) â assim a UI jÃ¡ mostra texto decente
/// e, quando o lib ganhar essas chaves, passa a usar a traduÃ§Ã£o automaticamente.
/// As chaves novas estÃ£o listadas no relatÃ³rio de entrega.
fn tor(key: &str, fallback: &str) -> String {
    let v = t(key);
    if v == key {
        fallback.to_string()
    } else {
        v
    }
}

// ---------------------------------------------------------------------------
// i18n: injeta TODOS os rÃ³tulos estÃ¡ticos da UI no `global L` do .slint. Nada de
// texto hardcoded no Slint â as strings vÃªm de `schematize::i18n` (11 locales).
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
    // Tooltip do selo verificado (sÃ³ o check + hover; sem texto ao lado).
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
    // modal de instalaÃ§Ã£o do Marketplace
    l.set_mp_recommends_note(t("gui.mp_recommends_note").into());
    l.set_mp_env_note(t("gui.mp_env_note").into());
    l.set_mp_confirm(t("gui.mp_confirm").into());
    l.set_mp_cancel(t("gui.mp_cancel").into());
    // aba Overdev (seletor de projeto + view) â reusa as chaves do egui.
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
    // Projetos fixados (pins) â chaves NOVAS com fallback embutido via `tor`.
    l.set_pinned_projects(tor("gui.pinned_projects", "Projetos fixados").into());
    l.set_pin_folder(tor("gui.pin_folder", "Fixar pastaâ¦").into());
    l.set_unpin(tor("gui.unpin", "Desafixar").into());
    l.set_pin_hint(tor(
        "gui.pin_hint",
        "Uma pasta fixada vira UM projeto no selector â Ãºtil pra workspace de microserviÃ§os.",
    ).into());
    l.set_no_overdev(t("gui.no_overdev").into());
    l.set_od_decisions(t("gui.od_decisions").into());
    l.set_od_plan(t("gui.od_plan").into());
    l.set_od_questions(t("gui.od_questions").into());
    l.set_open_browser(t("gui.open_browser").into());
    // aba Overdev â Fase 3 (editor + tasks + checklist 2-nÃ­veis). Chaves NOVAS com
    // fallback embutido via `tor` atÃ© serem adicionadas ao lib (ver relatÃ³rio).
    l.set_od_human(tor("gui.od_human", "humano").into());
    l.set_od_machine(tor("gui.od_machine", "mÃ¡quina").into());
    l.set_od_mark_human(tor("gui.od_mark_human", "marcar como feito").into());
    l.set_od_editor(tor("gui.od_editor", "Editor").into());
    l.set_od_save_plan(tor("gui.od_save_plan", "Salvar").into());
    l.set_od_tasks(tor("gui.od_tasks", "Tarefas e notas").into());
    l.set_od_add_note(tor("gui.od_add_note", "Adicionar nota").into());
    l.set_od_note(tor("gui.od_note", "Nota para esta tarefaâ¦").into());
    l.set_od_correction(tor("gui.od_correction", "Prompt de correÃ§Ã£o do overdev").into());
    l.set_od_notes_title(tor("gui.od_notes", "Notas e correÃ§Ãµes").into());
    // aba Overdev â Fase 4 (terminal externo + monitor leve). Chaves NOVAS via `tor`.
    l.set_od_run(tor("gui.od_run", "Executar overdev").into());
    l.set_od_gen_archive(tor("gui.od_gen_archive", "Gerar afazeres do archive").into());
    l.set_od_stop(tor("gui.od_stop", "Parar").into());
    l.set_od_running(tor("gui.od_running", "monitorandoâ¦").into());
    l.set_od_mon_active(tor("gui.od_mon_active", "rodando").into());
    l.set_od_confirm_run(tor(
        "gui.od_confirm_run",
        "Isto abre o `claude` num TERMINAL EXTERNO (processo prÃ³prio, fora do app) e roda o overdev \
         neste projeto com acesso ao seu ambiente â ele pode editar arquivos. O app apenas MONITORA o \
         progresso. Confira o comando abaixo antes de confirmar.",
    ).into());
    l.set_od_run_done(tor("gui.od_done", "concluÃ­do").into());
    l.set_od_agent_cmd(tor("gui.od_agent_cmd", "Comando do agente").into());
    l.set_od_ext_terminal(tor(
        "gui.od_ext_terminal",
        "claude rodando em terminal externo (processo prÃ³prio) â o load dele fica fora do app.",
    ).into());
    l.set_od_mon_iters(tor("gui.od_mon_iters", "iteraÃ§Ãµes").into());
    l.set_od_mon_open_title(tor("gui.od_mon_open_title", "Itens abertos (mÃ¡quina)").into());
    // Reload/Acompanhar + log de conclusÃµes + tokens (anexar monitor a run externo).
    l.set_od_attach(tor("gui.od_attach", "Reload / Acompanhar").into());
    l.set_od_refresh_tokens(tor("gui.od_refresh_tokens", "Atualizar tokens").into());
    l.set_od_completions_title(tor("gui.od_completions_title", "ConclusÃµes").into());
    // aba Grafo â reusa as chaves do egui (todas jÃ¡ nos 11 locales do lib).
    l.set_g_search_hint(t("gui.search").into());
    l.set_g_nodes_suffix(t("gui.graph_nodes").into());
    l.set_g_fit(t("gui.fit").into());
    l.set_g_no_graph(t("gui.no_graph").into());
    l.set_g_export_obsidian(t("gui.export_obsidian").into());
    l.set_g_open_editor(t("gui.open_editor").into());
    l.set_g_no_loc(t("gui.no_loc").into());
    // BotÃµes/estados NOVOS da aba Grafo (reindexar + recarregar + nÃ³ sem descriÃ§Ã£o).
    // Chaves novas com fallback pt-BR embutido via `tor` atÃ© entrarem no lib.
    l.set_g_reindex(tor("gui.g_reindex", "Reindexar").into());
    l.set_g_reload(tor("gui.g_reload", "Recarregar").into());
    l.set_g_drill(tor("gui.g_drill", "Grafo do serviço").into());
    l.set_g_global(tor("gui.g_global", "← Grafo global").into());
    l.set_g_node_nodesc(tor("gui.g_node_nodesc", "(sem descriÃ§Ã£o no Ã­ndice â rode Reindexar)").into());
    // Home + navegaÃ§Ã£o (Fase 1) â chaves NOVAS, com fallback embutido via `tor`
    // atÃ© serem adicionadas ao lib. Ver lista no relatÃ³rio de entrega.
    l.set_home(tor("gui.home", "InÃ­cio").into());
    l.set_home_title(tor("gui.home_title", "O que vocÃª quer fazer?").into());
    l.set_home_market(tor("gui.home_market", "Mercado de Skills").into());
    l.set_home_overdev_desc(tor("gui.home_overdev_desc", "Acompanhe o desenvolvimento contÃ­nuo do projeto.").into());
    l.set_home_market_desc(tor("gui.home_market_desc", "Instale, atualize e descubra skills e environments.").into());
    l.set_home_graph_desc(tor("gui.home_graph_desc", "Explore o grafo de microfunÃ§Ãµes do projeto.").into());
    l.set_home_environments(tor("gui.home_environments", "Environments").into());
    l.set_home_environments_desc(tor("gui.home_environments_desc", "Gerencie os runtimes de linguagem.").into());
    l.set_home_ssh(tor("gui.home_ssh", "SSH").into());
    l.set_home_ssh_desc(tor("gui.home_ssh_desc", "Chaves e acesso remoto.").into());
    l.set_home_settings(tor("gui.home_settings", "ConfiguraÃ§Ãµes").into());
    l.set_home_settings_desc(tor("gui.home_settings_desc", "Idioma, tema e preferÃªncias.").into());
    l.set_open_vscode(tor("gui.open_vscode", "Abrir no VSCode").into());
    l.set_open_loose_project(tor("gui.open_loose_project", "Abrir projeto avulsoâ¦").into());
    // aba Gerenciar (criar + editar skills) â chaves NOVAS via `tor`. Ver relatÃ³rio.
    l.set_manage(tor("gui.manage", "Gerenciar").into());
    l.set_create_skill(tor("gui.create_skill", "Criar skill").into());
    l.set_edit_skill(tor("gui.edit_skill", "Editar skill").into());
    l.set_skill_slug(tor("gui.skill_slug", "Slug").into());
    l.set_skill_name(tor("gui.skill_name", "Nome").into());
    l.set_skill_desc(tor("gui.skill_desc", "DescriÃ§Ã£o").into());
    l.set_create(tor("gui.create", "Criar").into());
    l.set_save(tor("gui.save", "Salvar").into());
    l.set_saved(tor("gui.saved", "Salvo").into());
    l.set_slug_invalid(tor("gui.slug_invalid", "slug invÃ¡lido â use sÃ³ [a-z0-9-], comeÃ§ando por letra/dÃ­gito").into());
    l.set_skill_exists(tor("gui.skill_exists", "essa skill jÃ¡ existe").into());
    l.set_pick_skill(tor("gui.pick_skill", "Escolha uma skillâ¦").into());
    l.set_pick_file(tor("gui.pick_file", "Arquivos").into());
    l.set_skill_created(tor("gui.skill_created", "Skill criada em").into());
    l.set_no_installed_skills(tor("gui.no_installed_skills", "Nenhuma skill instalada para editar").into());
    l.set_edit_now(tor("gui.edit_now", "Editar agora").into());
    l.set_pick_file_hint(tor("gui.pick_file_hint", "Selecione um arquivo na barra lateral para editar").into());
    // Tela SSH â chaves NOVAS via `tor`.
    l.set_ssh_title(tor("gui.ssh_title", "Chaves SSH").into());
    l.set_ssh_generate(tor("gui.ssh_generate", "Gerar chave").into());
    l.set_ssh_name(tor("gui.ssh_name", "Nome").into());
    l.set_ssh_kind(tor("gui.ssh_kind", "Tipo").into());
    l.set_ssh_comment(tor("gui.ssh_comment", "ComentÃ¡rio").into());
    l.set_ssh_passphrase(tor("gui.ssh_passphrase", "Passphrase (opcional)").into());
    l.set_ssh_copy_pub(tor("gui.ssh_copy_pub", "Copiar pÃºblica").into());
    l.set_ssh_copied(tor("gui.ssh_copied", "copiado").into());
    l.set_ssh_remove(tor("gui.ssh_remove", "Remover").into());
    l.set_ssh_empty(tor("gui.ssh_empty", "Nenhuma chave em ~/.ssh â gere uma acima.").into());
    l.set_ssh_priv_note(tor("gui.ssh_priv_note", "A chave privada nunca Ã© exposta â sÃ³ a pÃºblica sai.").into());
    l.set_ssh_keys_title(tor("gui.ssh_keys_title", "Suas chaves").into());
    // SSH â entropia (do lib, por tipo) + prova + Bitwarden. Chaves NOVAS via `tor`.
    l.set_ssh_entropy_ed25519(sshkeys::entropy_note(sshkeys::KeyKind::Ed25519).into());
    l.set_ssh_entropy_rsa(sshkeys::entropy_note(sshkeys::KeyKind::Rsa4096).into());
    l.set_ssh_kind_hint(tor(
        "gui.ssh_kind_hint",
        "ed25519 Ã© o default forte da casa; use RSA sÃ³ para hosts legados â e nunca abaixo de 4096 bits.",
    ).into());
    l.set_ssh_proof_label(tor("gui.ssh_proof_label", "Prova da chave (bits Â· fingerprint Â· tipo)").into());
    l.set_ssh_export_bw(tor("gui.ssh_export_bw", "Exportar â Bitwarden").into());
    l.set_ssh_bw_note(tor(
        "gui.ssh_bw_note",
        "Exportar â Bitwarden salva a chave no seu cofre (se destravado) ou gera um arquivo de import 600. \
         A chave PRIVADA nunca aparece nesta tela.",
    ).into());
    // Tela ConfiguraÃ§Ãµes â chaves NOVAS via `tor`.
    l.set_cfg_title(tor("gui.cfg_title", "ConfiguraÃ§Ãµes").into());
    l.set_cfg_language(tor("gui.cfg_language", "Idioma").into());
    l.set_cfg_theme(tor("gui.cfg_theme", "Tema").into());
    l.set_cfg_autostart(tor("gui.cfg_autostart", "Autostart do agente").into());
    l.set_cfg_autostart_desc(tor("gui.cfg_autostart_desc", "Inicia o agente de atualizaÃ§Ã£o junto com a sua sessÃ£o.").into());
    l.set_cfg_hooks(tor("gui.cfg_hooks", "Hooks do overdev").into());
    l.set_cfg_hooks_desc(tor("gui.cfg_hooks_desc", "Registra os hooks (Stop/PreToolUse) do overdev no Claude Code.").into());
    l.set_cfg_dirs(tor("gui.cfg_dirs", "DiretÃ³rios de dev e projetos fixados").into());
    l.set_cfg_dirs_desc(tor("gui.cfg_dirs_desc", "Onde o schematize procura os seus projetos.").into());
    l.set_cfg_manage(tor("gui.cfg_manage", "Gerenciarâ¦").into());
    l.set_cfg_on(tor("gui.cfg_on", "ligado").into());
    l.set_cfg_off(tor("gui.cfg_off", "desligado").into());
    // DiagnÃ³stico (relatÃ³rio de debug) â chaves NOVAS via `tor`.
    l.set_cfg_debug_title(tor("gui.cfg_debug_title", "DiagnÃ³stico").into());
    l.set_cfg_debug_btn(tor("gui.cfg_debug_btn", "Gerar relatÃ³rio de debug").into());
    l.set_cfg_debug_generating(tor("gui.cfg_debug_generating", "Gerandoâ¦").into());
    l.set_cfg_debug_open(tor("gui.cfg_debug_open", "Abrir pasta").into());
    l.set_cfg_debug_note(tor(
        "gui.cfg_debug_note",
        "modo 600 Â· segredos redigidos Â· revise antes de compartilhar",
    ).into());
    l.set_cfg_debug_net(tor("gui.cfg_debug_net", "incluir diagnÃ³stico de rede (mais lento)").into());
    l.set_cfg_debug_saved(tor("gui.cfg_debug_saved", "RelatÃ³rio gravado em").into());
    // Overdev â aditivos.
    l.set_od_history(tor("gui.od_history", "HistÃ³rico (cÃ³pia de seguranÃ§a)").into());
    l.set_od_history_note(tor("gui.od_history_note", "O agente pode editar/apagar os arquivos do .overdev/ â este Ã© o backup versionado deles.").into());
    l.set_od_view(tor("gui.od_view", "Ver").into());
    l.set_od_restore(tor("gui.od_restore", "Restaurar").into());
    l.set_od_snap_empty(tor("gui.od_snap_empty", "Sem snapshots ainda.").into());
    l.set_od_commits(tor("gui.od_commits", "Commits e push").into());
    l.set_od_commits_empty(tor("gui.od_commits_empty", "Sem commits (ou nÃ£o Ã© um repositÃ³rio git).").into());
    l.set_od_close(tor("gui.od_close", "Fechar").into());
    // PaginaÃ§Ã£o.
    l.set_pg_prev(tor("gui.pg_prev", "â¹ Anterior").into());
    l.set_pg_next(tor("gui.pg_next", "PrÃ³ximo âº").into());
    l.set_pg_of(tor("gui.pg_of", "de").into());
    // VersÃ£o do app + self-update (ConfiguraÃ§Ãµes). Chaves NOVAS via `tor`.
    l.set_app_version_title(tor("gui.app_version_title", "VersÃ£o do app").into());
    l.set_app_check_update(tor("gui.app_check_update", "Verificar atualizaÃ§Ã£o").into());
    l.set_app_checking(tor("gui.app_checking", "Verificandoâ¦").into());
    l.set_app_up_to_date(tor("gui.app_up_to_date", "VocÃª estÃ¡ atualizado").into());
    l.set_app_update_btn(tor("gui.app_update_btn", "Atualizar app").into());
    l.set_app_updating(tor("gui.app_updating", "Atualizandoâ¦").into());
    l.set_app_restart_hint(tor("gui.app_restart_hint", "AtualizaÃ§Ã£o concluÃ­da â reinicie o app.").into());
    l.set_app_restart(tor("gui.app_restart", "Reiniciar").into());
    l.set_updater_missing_msg(tor("gui.updater_missing", "O gestor de atualizações (schematize-updater) não está instalado — ele cuida de instalar/atualizar o app.").into());
    l.set_updater_install_btn(tor("gui.updater_install", "Instalar gestor de atualizações").into());
    // Sininho de notificaÃ§Ãµes.
    l.set_notif_title(tor("gui.notif_title", "NotificaÃ§Ãµes").into());
    l.set_notif_empty(tor("gui.notif_empty", "Sem notificaÃ§Ãµes").into());
    l.set_notif_global(tor("gui.notif_global", "Globais").into());
    l.set_notif_personal(tor("gui.notif_personal", "Pessoais").into());
    l.set_notif_loading(tor("gui.notif_loading", "Carregandoâ¦").into());
    l.set_notif_do_update(tor("gui.notif_do_update", "Atualizar").into());
    l.set_notif_open(tor("gui.notif_open", "Abrir").into());
    l.set_notif_go_installed(tor("gui.notif_go_installed", "Ver instaladas").into());
    // Fork + comparar.
    l.set_fork_badge(tor("gui.fork_badge", "fork").into());
    l.set_fork_will(tor(
        "gui.fork_will",
        "Esta Ã© uma skill OFICIAL. Ao editÃ¡-la, ela serÃ¡ forkada: uma cÃ³pia editÃ¡vel fica ativa e a versÃ£o oficial Ã© preservada para comparar depois.",
    ).into());
    l.set_fork_active(tor(
        "gui.fork_active",
        "Fork ativo â a versÃ£o oficial estÃ¡ preservada para vocÃª comparar.",
    ).into());
    l.set_compare_official(tor("gui.compare_official", "Comparar com oficial").into());
    l.set_compare_note(tor(
        "gui.compare_note",
        "Comparar NÃO sobrescreve nada â apenas mostra as diferenÃ§as entre o seu fork e a versÃ£o oficial nova.",
    ).into());
    l.set_compare_files(tor("gui.compare_files", "Arquivos").into());
    l.set_compare_loading(tor("gui.compare_loading", "Comparandoâ¦").into());
    // Conta (login via device flow) â chaves NOVAS via `tor`.
    l.set_acc_section(tor("gui.acc_section", "Conta").into());
    l.set_acc_login(tor("gui.acc_login", "Entrar na plataforma").into());
    l.set_acc_logout(tor("gui.acc_logout", "Sair").into());
    l.set_acc_connected_as(tor("gui.acc_connected_as", "Conectado como").into());
    l.set_acc_logged_out_hint(tor(
        "gui.acc_logged_out_hint",
        "Entre na plataforma para receber notificaÃ§Ãµes e sincronizar suas skills.",
    ).into());
    l.set_acc_modal_title(tor("gui.acc_modal_title", "Entrar na plataforma").into());
    l.set_acc_code_label(tor(
        "gui.acc_code_label",
        "Abra o endereÃ§o abaixo no navegador e digite este cÃ³digo:",
    ).into());
    l.set_acc_open_browser(tor("gui.acc_open_browser", "Abrir no navegador").into());
    l.set_acc_verification_at(tor("gui.acc_verification_at", "Acesse:").into());
    l.set_acc_waiting(tor("gui.acc_waiting", "Aguardando confirmaÃ§Ã£oâ¦").into());
    l.set_acc_cancel(tor("gui.acc_cancel", "Cancelar").into());
    l.set_acc_indicator_tip(tor("gui.acc_indicator_tip", "Conectado â abrir Conta").into());
    // Database builder (tela 6) â chaves NOVAS via `tor`.
    l.set_home_database(tor("gui.home_database", "Banco de dados").into());
    l.set_home_database_desc(tor("gui.home_database_desc", "Leia, modele e gere o schema do seu banco.").into());
    l.set_db_title(tor("gui.db_title", "Database builder").into());
    l.set_db_sub_connect(tor("gui.db_sub_connect", "Conectar").into());
    l.set_db_sub_schema(tor("gui.db_sub_schema", "Schema").into());
    l.set_db_sub_generate(tor("gui.db_sub_generate", "Gerar").into());
    l.set_db_sub_graph(tor("gui.db_sub_graph", "Grafo").into());
    l.set_db_sqlite_label(tor("gui.db_sqlite_label", "Arquivo SQLite").into());
    l.set_db_pg_label(tor("gui.db_pg_label", "Connection string Postgres").into());
    l.set_db_pick_file(tor("gui.db_pick_file", "Escolherâ¦").into());
    l.set_db_introspect(tor("gui.db_introspect", "Introspectar").into());
    l.set_db_load_json(tor("gui.db_load_json", "Carregar schema.json").into());
    l.set_db_save_json(tor("gui.db_save_json", "Salvar schema.json").into());
    l.set_db_no_schema(tor("gui.db_no_schema", "Nenhum schema carregado â introspecte um banco, carregue um schema.json ou adicione uma tabela.").into());
    l.set_db_tables_title(tor("gui.db_tables_title", "Tabelas").into());
    l.set_db_cols_label(tor("gui.db_cols_label", "Colunas").into());
    l.set_db_fks_label(tor("gui.db_fks_label", "Chaves estrangeiras").into());
    l.set_db_indexes_label(tor("gui.db_indexes_label", "Ãndices").into());
    l.set_db_editor_title(tor("gui.db_editor_title", "Editar schema").into());
    l.set_db_add_table(tor("gui.db_add_table", "Adicionar tabela").into());
    l.set_db_table_name(tor("gui.db_table_name", "Nome da tabela").into());
    l.set_db_select_table(tor("gui.db_select_table", "Tabela alvo").into());
    l.set_db_add_column(tor("gui.db_add_column", "Adicionar coluna").into());
    l.set_db_col_name(tor("gui.db_col_name", "Nome da coluna").into());
    l.set_db_col_type(tor("gui.db_col_type", "Tipo").into());
    l.set_db_pk(tor("gui.db_pk", "PK").into());
    l.set_db_unique(tor("gui.db_unique", "UNIQUE").into());
    l.set_db_nullable(tor("gui.db_nullable", "NULL").into());
    l.set_db_add_fk(tor("gui.db_add_fk", "Adicionar FK").into());
    l.set_db_fk_col(tor("gui.db_fk_col", "Coluna").into());
    l.set_db_fk_reftable(tor("gui.db_fk_reftable", "Tabela ref.").into());
    l.set_db_fk_refcol(tor("gui.db_fk_refcol", "Coluna ref.").into());
    l.set_db_gen_sql(tor("gui.db_gen_sql", "Gerar SQL").into());
    l.set_db_gen_migration(tor("gui.db_gen_migration", "Gerar migration").into());
    l.set_db_gen_save(tor("gui.db_gen_save", "Salvar em arquivoâ¦").into());
    l.set_db_ai_title(tor("gui.db_ai_title", "Gerar por descriÃ§Ã£o (IA)").into());
    l.set_db_ai_hint(tor("gui.db_ai_hint", "Descreva o domÃ­nio do sistemaâ¦").into());
    l.set_db_ai_generate(tor("gui.db_ai_generate", "Gerar com IA").into());
    l.set_db_ai_note(tor(
        "gui.db_ai_note",
        "Segue a skill schematize-database num terminal externo e emite schema.json + schema.sql + migration no <projeto>_archive/database/. Roda no terminal; carregue o schema.json quando terminar.",
    ).into());
    l.set_db_ai_no_project(tor("gui.db_ai_no_project", "Selecione um projeto na tela Overdev/Grafo primeiro.").into());
    l.set_db_view_graph(tor("gui.db_view_graph", "Ver grafo").into());
    l.set_db_node_cols(tor("gui.db_node_cols", "(sem colunas)").into());
}

// ---------------------------------------------------------------------------
// aba Gerenciar â lista os SLUGS das skills instaladas escaneando o diretÃ³rio
// de skills (`~/.claude/skills/schematize-<slug>/` com SKILL.md). Cobre tanto as
// skills do catÃ¡logo quanto as criadas pelo usuÃ¡rio (que nÃ£o estÃ£o no catÃ¡logo).
// Retorna Vec<String> (Send) â seguro pra rodar em thread e postar via event loop.
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
// Logo da janela â MESMA marca do egui (`schematize::appicon::rgba(256)`),
// convertida num `slint::Image` pra alimentar a propriedade `icon` do Window.
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// RelanÃ§a o app numa janela NOVA e encerra este processo. CONSERTO do bug do
// "reiniciar" (pÃ³s self-update) que sÃ³ fechava e nÃ£o reabria: fazemos um spawn
// DESACOPLADO do binÃ¡rio atual (nova sessÃ£o de processos via `process_group(0)`
// + stdio em /dev/null) ANTES do `exit(0)`, entÃ£o a janela nova sobe sozinha e
// sobrevive Ã  saÃ­da deste. Chamado pelo callback `restart` do Slint.
// ---------------------------------------------------------------------------
fn restart_app() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(&exe);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.process_group(0); // grupo prÃ³prio â nÃ£o morre com o processo atual
        let _ = cmd.spawn(); // best-effort: se falhar, ainda saÃ­mos limpo
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
// Abre o projeto no VSCode: `code <root>` se o binÃ¡rio existe; senÃ£o cai no
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
// Estado derivado (missing/outdated/current/loading) + rÃ³tulo traduzido.
// ---------------------------------------------------------------------------
fn compute_state(installed: &Option<String>, latest: &Option<String>) -> (String, String) {
    match (installed, latest) {
        // NÃ£o instalada â mesmo com latest desconhecido, dÃ¡ pra instalar.
        (None, _) => ("missing".into(), t("common.not_installed")),
        // Instalada, mas ainda resolvendo o latest (rede) â spinner.
        (Some(_), None) => ("loading".into(), "â¦".into()),
        (Some(i), Some(l)) if i == l => ("current".into(), t("common.current")),
        // Desatualizada: "UPDATE (XâY)".
        (Some(i), Some(l)) => ("outdated".into(), format!("{} ({}â{})", t("common.update"), i, l)),
    }
}

// ---------------------------------------------------------------------------
// Montagem inicial do modelo (cabeÃ§alhos de categoria + skills). Retorna as
// linhas E o Item alinhado a cada linha (None nos cabeÃ§alhos), pra as aÃ§Ãµes.
// ---------------------------------------------------------------------------
/// Categoria normalizada de um item (vazio â "language").
fn category_of(it: &Item) -> &str {
    if it.category.is_empty() { "language" } else { it.category.as_str() }
}

/// CabeÃ§alho de categoria de UMA pÃ¡gina (page 0 = Instaladas, 1 = Marketplace).
/// `count` Ã© preenchido/atualizado por `recompute_headers` (esconde vazios).
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

/// Linha inicial de uma skill: instalada lida do disco (rÃ¡pido), latest ainda
/// "â¦" (resolvido depois, assÃ­ncrono). Estado derivado do que jÃ¡ se sabe.
/// `forked` = a skill oficial virou fork editÃ¡vel (marca [fork] + habilita Comparar).
fn skill_row(it: &Item, forked: bool) -> SkillRow {
    let author = it.sponsor.as_ref().map(|s| s.name.clone()).unwrap_or_default();
    let author_url = it.sponsor.as_ref().map(|s| s.url.clone()).unwrap_or_default();
    let installed = skills::installed_version(it);
    let latest: Option<String> = None; // resolvido assÃ­ncrono apÃ³s subir a janela
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
        installed: installed.unwrap_or_else(|| "â".into()).into(),
        latest: "â¦".into(),
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
/// DOIS cabeÃ§alhos (Instaladas page=0 e Marketplace page=1) seguidos das skills;
/// a pÃ¡gina ativa mostra o cabeÃ§alho certo e as skills cujo estado casa (o Slint
/// filtra por `state`). Devolve o Item por linha (None nos cabeÃ§alhos).
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
        // page 0 = Instaladas, page 1 = Marketplace â sÃ³ um aparece por vez.
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

/// Atualiza o marcador `forked` da linha de uma skill (por slug) no modelo do app â
/// chamado apÃ³s uma ediÃ§Ã£o que forka uma skill oficial, pra o badge [fork] e o botÃ£o
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
// Reconta os cabeÃ§alhos: cada cabeÃ§alho (page, categoria) ganha o nÂº de skills
// que estÃ£o AGORA na sua pÃ¡gina (Instaladas = state != "missing"; Marketplace =
// state == "missing"). count==0 â o Slint esconde o cabeÃ§alho. Roda no event
// loop (sÃ³ usa o modelo do app; nada de dados !Send).
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
                // page 0 = Instaladas (nÃ£o-missing); page 1 = Marketplace (missing).
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
// PaginaÃ§Ã£o do Mercado: numera (disp) as skills VISÃVEIS na pÃ¡gina-tab ativa em
// ordem sequencial; -1 nas que nÃ£o pertencem Ã  pÃ¡gina. O Slint mostra sÃ³ as
// cujo `disp` cai na janela `[mkt-page*20, +20)`. Total â controla o Pager.
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
// Status global (contagem de pendÃªncias) â mesma regra do egui.
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
// threadâUI: posta a atualizaÃ§Ã£o de versÃµes (installed + latest) de uma linha.
// ---------------------------------------------------------------------------
fn post_versions(weak: Weak<AppWindow>, idx: usize, installed: Option<String>, latest: Option<String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            let rows = app.get_rows();
            if let Some(mut r) = rows.row_data(idx) {
                let (state, label) = compute_state(&installed, &latest);
                r.installed = installed.clone().unwrap_or_else(|| "â".into()).into();
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

/// threadâUI: marca uma linha como ocupada (operaÃ§Ã£o em andamento) com rÃ³tulo.
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

/// threadâUI: resultado de uma operaÃ§Ã£o numa linha. Instalar â instalada=latest
/// (o release baixado Ã o latest) e estado "current"; remover â nÃ£o instalada.
/// Erro â mantÃ©m e mostra o rÃ³tulo em warn.
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
                            r.installed = "â".into();
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

/// threadâUI: fim do lote â solta o `busy` global e mostra o toast final.
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
// ResoluÃ§Ã£o assÃ­ncrona do latest de UMA skill (rede). Detached: reusa install/
// check. Re-lÃª a instalada (barato) pra refletir mudanÃ§as de disco.
// ---------------------------------------------------------------------------
fn spawn_resolve(weak: Weak<AppWindow>, idx: usize, item: Item) {
    std::thread::spawn(move || {
        let installed = skills::installed_version(&item);
        let latest = skills::resolve_latest(&item).ok();
        post_versions(weak, idx, installed, latest);
    });
}

/// Dispara a resoluÃ§Ã£o do latest de todas as skills em paralelo (uma thread por
/// skill; sÃ£o poucas). Antes, zera a coluna latest de volta pra "â¦".
fn kick_resolve_all(weak: &Weak<AppWindow>, row_items: &Rc<Vec<Option<Item>>>) {
    if let Some(app) = weak.upgrade() {
        let rows = app.get_rows();
        for (idx, maybe) in row_items.iter().enumerate() {
            if let Some(it) = maybe {
                if let Some(mut r) = rows.row_data(idx) {
                    r.latest = "â¦".into();
                    if r.installed != "â" {
                        r.state = "loading".into();
                        r.state_label = "â¦".into();
                    }
                    rows.set_row_data(idx, r);
                }
                spawn_resolve(weak.clone(), idx, it.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Notas do marketplace: busca TODAS as mÃ©dias numa thread (1 request) e preenche
// a coluna `rating` de cada linha de skill por slug. Sem nota (count 0 / None) â
// `format_rating` devolve "" e o badge some. Falha de rede = HashMap vazio â sÃ³
// nÃ£o mostra nota (a UI nunca trava; nada de bloqueio no event loop).
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
// AÃ§Ãµes em massa/paralelo (espelha o run_batch do egui). ops = (idx, install?, Item).
// ---------------------------------------------------------------------------
fn run_batch(weak: Weak<AppWindow>, ops: Vec<(usize, bool, Item)>) {
    if ops.is_empty() {
        return;
    }
    if let Some(app) = weak.upgrade() {
        if app.get_busy() {
            return; // jÃ¡ tem lote rodando
        }
        app.set_busy(true);
    }
    std::thread::spawn(move || {
        let ok = AtomicUsize::new(0);
        let err = AtomicUsize::new(0);
        // marca cada linha como ocupada antes de comeÃ§ar
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
/// `install=true` â instalar/atualizar; `install=false` â remover.
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

/// NÃO instalada (pertence ao Marketplace).
fn is_missing(r: &SkillRow) -> bool {
    r.state == "missing"
}
/// Instalada (pertence a Instaladas) â qualquer estado que nÃ£o seja "missing".
fn is_installed(r: &SkillRow) -> bool {
    r.state != "missing"
}
/// Instalada E desatualizada (installed Some E latest > installed). Ã o ÃNICO
/// alvo de "Atualizar tudo"/"Atualizar selecionadas": jamais instala nova.
fn is_outdated(r: &SkillRow) -> bool {
    r.state == "outdated"
}

// ===========================================================================
// ENVIRONMENTS â gestÃ£o dos runtimes de linguagem (aba 2).
// A GUI sÃ³ MONTA o comando e ABRE UM TERMINAL rodando `schematize env â¦`; o plano
// exato + consentimento (e o sudo) acontecem no terminal (honesto). NUNCA executa
// o instalador de environment de dentro do processo da GUI.
// ===========================================================================

/// RÃ³tulo de status de um environment â mesmas chaves i18n que o `list()` do CLI usa.
fn env_status_label(le: &environments::LangEnv) -> String {
    if let Some(m) = le.installed {
        tf("env.installed_via", &[("method", m.slug())])
    } else if le.runtime_present {
        t("env.installed")
    } else {
        t("env.not_installed")
    }
}

/// ConstrÃ³i uma linha da aba Environments a partir do status do lib. O
/// `section_title` fica vazio aqui; quem monta a lista (build_env_rows_from) o
/// preenche na PRIMEIRA linha de cada seÃ§Ã£o (linguagens Ã ferramentas).
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

/// TÃ­tulo traduzido da seÃ§Ã£o de uma categoria ("language" | "tool").
fn env_section_title(category: &str) -> String {
    match category {
        "tool" => tor("gui.env_tools_title", "Ferramentas de dev"),
        _ => tor("gui.env_langs_title", "Linguagens"),
    }
}

/// Monta as linhas a partir de um status jÃ¡ sondado, marcando o `section_title`
/// na primeira linha de cada categoria (o `status()` do lib jÃ¡ lista linguagens
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

/// ConstrÃ³i o modelo inteiro da aba Environments a partir de `environments::status()`.
fn build_env_rows() -> Vec<EnvRow> {
    build_env_rows_from(&environments::status())
}

// ---------------------------------------------------------------------------
// SSH â modelo da tela de chaves a partir de `sshkeys::list()` (sÃ³ metadados
// PÃBLICOS; a privada nunca Ã© lida/exposta). Igual ao padrÃ£o dos demais modelos.
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
// Idiomas p/ o seletor de ConfiguraÃ§Ãµes (cÃ³digo + nome nativo + marca do atual).
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
// FormataÃ§Ã£o p/ o histÃ³rico do DB do overdev. Sem crate de data: converte o
// epoch (UTC) via o algoritmo civil de Howard Hinnant.
// ---------------------------------------------------------------------------
fn fmt_ts(ts: i64) -> String {
    if ts <= 0 {
        return "â".into();
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

/// Tamanho legÃ­vel (B / KB / MB).
fn fmt_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Linha de upstream (branch â remote Â· âahead âbehind); vazia se sem tracking.
fn fmt_upstream(up: Option<githist::Upstream>) -> String {
    match up {
        Some(u) => {
            let remote = u.remote.unwrap_or_else(|| "â".into());
            format!("{} â {} Â· â{} â{}", u.branch, remote, u.ahead, u.behind)
        }
        None => String::new(),
    }
}

/// Tamanho da pÃ¡gina das listas paginadas (mercado, histÃ³rico do DB, commits).
const PAGE: usize = 20;

/// Uma pÃ¡gina do histÃ³rico do DB (metadados â SnapRow).
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

/// Uma pÃ¡gina do histÃ³rico de commits (Commit â CommitRow).
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

/// Localiza o binÃ¡rio `schematize` (CLI) pra montar o comando do terminal:
/// primeiro um irmÃ£o do executÃ¡vel atual; senÃ£o o do PATH.
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

/// Um binÃ¡rio existe no PATH?
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

/// Abre um terminal grÃ¡fico rodando `inner` (bash -c). Mesmo padrÃ£o do gui.rs (egui):
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
/// SEM `--yes`: o CLI mostra o plano e PEDE confirmaÃ§Ã£o ali dentro (consentimento honesto).
fn env_terminal_inner(bin: &str, action: &str, lang: &str, method: &str) -> String {
    // Ferramentas nÃ£o tÃªm mÃ©todo (o CLI ignora `--method` pra elas) â omite o flag
    // quando `method` vem vazio, pra nÃ£o passar um `--method ` sem valor.
    let (tag, method_arg) = if method.is_empty() {
        (String::new(), String::new())
    } else {
        (format!(" ({method})"), format!(" --method {method}"))
    };
    format!(
        "echo 'ââ schematize env {action} {lang}{tag} ââ'; echo; \
         {bin} env {action} {lang}{method_arg}; \
         echo; read -n1 -s -r -p 'â¦'",
        action = action,
        lang = lang,
        tag = tag,
        method_arg = method_arg,
        bin = bin
    )
}

/// Dispara o terminal p/ uma aÃ§Ã£o de environment e devolve o rÃ³tulo transitÃ³rio a exibir
/// na linha (terminal aberto, ou instruÃ§Ã£o manual quando nenhum terminal foi encontrado).
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

/// Uma skill (por slug) estÃ¡ instalada AGORA? (lÃª o modelo de linhas de skills.)
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

/// Ãndice da linha de uma skill (por slug) no vetor de itens alinhado ao modelo.
fn row_idx_of_slug(row_items: &[Option<Item>], slug: &str) -> Option<usize> {
    row_items
        .iter()
        .position(|m| m.as_ref().map(|it| it.slug == slug).unwrap_or(false))
}

/// Estado do modal de instalaÃ§Ã£o do Marketplace, guardado no lado Rust (o Slint
/// carrega sÃ³ o visual). Preenchido ao abrir; lido no confirmar.
#[derive(Default, Clone)]
struct ModalState {
    skill_idx: usize,   // linha da skill sendo instalada
    rec_slug: String,   // slug da recomendada a oferecer ("" = nenhuma)
    env_lang: String,   // linguagem do environment a oferecer ("" = nenhum)
}

// ===========================================================================
// OVERDEV â seletor de projeto (porte do project_bar do egui) + view nativa
// (porte do overdev_view). LÃª `schematize::projects::scan()` + `config` p/ o
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

/// CabeÃ§alho de grupo do seletor (Detectados / Recentes).
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
/// (os que nÃ£o estÃ£o jÃ¡ entre os detectados). Espelha o combo do project_bar egui.
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

/// Dir de overdev do projeto pela regra "ler ambos" do lib: `.schematize/overdev` (novo, vivo) se
/// existir; senão `.overdev` legado; senão o novo default. Era hardcoded `.overdev` — por isso a GUI
/// mostrava 0/0/0 e editor vazio em projetos já migrados pro `.schematize/` (gerava no novo, lia no
/// antigo). Usa o MESMO resolvedor do CLI (`panel::load_overdev`), então GUI e CLI concordam.
fn overdev_dir(root: &Path) -> PathBuf {
    schematize::paths::overdev_dir_at(root)
}

/// Caminho do CHECKLIST do overdev do projeto (dir resolvido por `overdev_dir`).
fn checklist_path(root: &Path) -> PathBuf {
    overdev_dir(root).join("CHECKLIST.md")
}

/// Caminho de um arquivo do editor (`PLAN.md`/`CHECKLIST.md`) no dir de overdev resolvido.
/// Sanitiza `target` a um basename simples pra a GUI nunca escrever fora do dir de overdev.
fn overdev_file_path(root: &Path, target: &str) -> PathBuf {
    let name = Path::new(target).file_name().and_then(|s| s.to_str()).unwrap_or("PLAN.md");
    overdev_dir(root).join(name)
}

/// Parseia o CHECKLIST 2-nÃ­veis de `<root>` em `OverItem`s (kind + origem + Ã­ndice).
/// Casa `- [H ...]` ANTES de `- [ ]`/`- [x]` (senÃ£o o humano cai no ramo de mÃ¡quina).
/// `hindex` numera 1-based sÃ³ os HUMANOS ABERTOS (- [H ]) â Ã© o arg de `od-mark-human`.
fn parse_checklist_items(root: &Path) -> Vec<OverItem> {
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

/// Fecha o `index`-Ã©simo (1-based) item HUMANO ABERTO de `<root>`: `- [H ]`â`- [H x]`.
/// Path-aware (o `overdev::human_done` do lib opera no cwd, nÃ£o serve Ã  GUI que
/// monitora outro projeto) â replica a regra do lib editando o arquivo direto.
fn mark_human_done_at(root: &Path, index: i32) -> Result<(), String> {
    if index < 1 {
        return Err("Ã­ndice humano invÃ¡lido".into());
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
        return Err(format!("nÃ£o hÃ¡ {index}Âº item humano aberto"));
    }
    std::fs::write(&path, out.join("\n")).map_err(|e| e.to_string())
}

/// Re-sonda dev_dirs + pins + projetos e reconstrÃ³i os modelos do seletor, da lista
/// de dev_dirs e da lista de pastas FIXADAS. O scan agora inclui os pins (pastas
/// fixadas pelo usuÃ¡rio) â elas aparecem no seletor mesmo sem marcador git.
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
/// Espelha o overdev_view do egui: objetivo, mode, progresso, checklist e seÃ§Ãµes.
/// Ações de skills instaladas (gui.json) → linhas do modelo Slint. Cada uma vira um botão na aba do
/// projeto; Q.A./Pentest aparecem quando as skills schematize-engineering/pentest estão instaladas.
fn skill_action_rows() -> Vec<SkillAction> {
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

/// Ícone da janela desenhado em código (`schematize::appicon::rgba`) — resiliente: não depende de
/// arquivo (não some nem quebra o build), e sai nítido em qualquer tamanho (antialiasing no lib).
fn make_app_icon() -> slint::Image {
    let (rgba, w, h) = schematize::appicon::rgba(256);
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, w, h);
    slint::Image::from_rgba8(buf)
}

/// Recomputa o orçamento do governador de concorrência e joga nos props da GUI (linha "máquina:
/// teto/livre/load/rodando" + clampa o K do split ao teto). Persiste ~/.schematize/agents.json.
fn apply_agent_budget(app: &AppWindow) {
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
    apply_agent_budget(app); // linha do governador (teto/livre/load) na aba Overdev
    app.set_od_current(basename_of(p).into());
    let ov = panel::load_overdev(p);
    // Checklist 2-nÃ­veis parseado direto (o panel::load_overdev do lib ignora os
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
    // Contagem 4-categorias derivada do MESMO parse 2-nÃ­veis (mÃ¡quina-abertos/feitos/
    // on-hold/humanos-abertos). `done` = feitos totais (mÃ¡quina `- [x]` + humano
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

/// Carrega no editor o conteÃºdo do arquivo escolhido (`od-editor-target`) de `<root>/.overdev`.
/// Limpa o feedback de status. Arquivo ausente â editor vazio (o Salvar cria).
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
// FASE 4 â overdev em TERMINAL EXTERNO + MONITOR leve. DECISÃO do dono: o `claude`
// NÃO roda mais acoplado (PTY) dentro do app â carregava o LOAD dele aqui (RAM,
// inchaÃ§o tipo VSCode) e nem submetia na TUI. Agora o botÃ£o sÃ³ chama
// `agentrun::launch_in_terminal` (processo PRÃPRIO num terminal do sistema,
// DESACOPLADO) e o app apenas MONITORA o `.overdev/` a cada ~3s (sÃ³ LÃ arquivos,
// nÃ£o segura processo). O "nÃ£o pare" Ã© imposto pelo Stop hook do overdev â nada
// de auto-continue/nudge aqui. SÃ³ `String`/`PathBuf`/`Weak<AppWindow>` +
// `Arc<AtomicBool>` cruzam a fronteira de thread; a UI Ã© tocada por `post_*`.
// ===========================================================================

/// Intervalo do monitor leve do `.overdev/` (sÃ³ relÃª arquivos de progresso).
const OD_MONITOR_EVERY: Duration = Duration::from_secs(3);

/// Teto de itens abertos listados no monitor (o `claude` roda fora; isto Ã© sÃ³ espelho).
const OD_MONITOR_ITEMS: usize = 10;

/// Intervalo MÃNIMO entre leituras de `usage::agent_usage` dentro do monitor.
/// CUIDADO PERF: `agent_usage` parseia os `.jsonl` do Claude (100MB+) â jamais a
/// cada ciclo. Relemos os tokens no mÃ¡x. a cada 30s (e sempre em thread prÃ³pria).
const OD_USAGE_EVERY: Duration = Duration::from_secs(30);

/// Agrupa milhares com `.` (separador pt-BR): 1234567 â "1.234.567". PURO.
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
/// Fallback improvÃ¡vel (timestamp fora de faixa) â string vazia.
fn fmt_ts_local(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Materializa as conclusÃµes (`overdev::completions`) em linhas prontas pra UI:
/// `HH:MM:SS  <texto>` (hora local). MantÃ©m a ordem do lib (ts asc â recentes embaixo).
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
/// "Tokens: <total> (in <in> / out <out> Â· cache-read <cr>) Â· Modelo: <main>".
fn fmt_usage(u: &usage::Usage) -> String {
    let model = u.main_model().unwrap_or("â");
    format!(
        "{}: {} (in {} / out {} Â· cache-read {}) Â· {}: {}",
        tor("gui.od_tokens", "Tokens"),
        sep_thousands(u.total),
        sep_thousands(u.input),
        sep_thousands(u.output),
        sep_thousands(u.cache_read),
        tor("gui.od_model", "Modelo"),
        model,
    )
}

/// threadâUI: espelha o log de conclusÃµes (linhas jÃ¡ formatadas `HH:MM:SS texto`).
fn post_completions(weak: &Weak<AppWindow>, lines: Vec<String>) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            let rows: Vec<SharedString> = lines.into_iter().map(SharedString::from).collect();
            app.set_od_completions(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

/// threadâUI: escreve a linha de tokens/modelo jÃ¡ formatada.
fn post_usage(weak: &Weak<AppWindow>, line: String) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.set_od_usage_line(line.into());
        }
    });
}

/// LÃª `usage::agent_usage` (PESADO: parseia .jsonl de 100MB+) numa thread PRÃPRIA e
/// posta a linha formatada. Nunca no event loop, nunca no ritmo de 3s do monitor.
fn spawn_usage(weak: Weak<AppWindow>, project: PathBuf) {
    std::thread::spawn(move || {
        let u = usage::agent_usage(&project);
        post_usage(&weak, fmt_usage(&u));
    });
}

/// threadâUI: espelha o snapshot do `.overdev/` (estado + contadores + iteraÃ§Ãµes +
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

/// threadâUI: FIM do monitor â larga o flag de "monitorando", fixa o modo final e
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

/// MONITOR leve: a cada ~3s lÃª `overdev::progress_at` + `open_items_at` + o log de
/// `overdev::completions` (tudo BARATO) e espelha na UI; os tokens (`agent_usage`,
/// PESADO) sÃ³ no arranque e a cada ~30s, sempre em thread prÃ³pria. NÃO segura o
/// processo do `claude` (ele roda no terminal externo). Para quando o botÃ£o Parar
/// levanta a `stop`, ou quando o run some/termina (`mode == "stopped"` /
/// `Progress::finished()`), MAS sÃ³ depois de ter visto o run ficar `active` uma vez â
/// assim um `state.json` velho ("stopped") nÃ£o encerra o monitor antes de o overdev
/// sequer arrancar no terminal.
///
/// `attach`: quando `true` (botÃ£o "Reload / Acompanhar", anexando a um run que jÃ¡
/// roda POR FORA), comeÃ§amos com `seen_active = true` â assim um run jÃ¡ em
/// andamento (mode "active") Ã© seguido de imediato e um run jÃ¡ FINALIZADO
/// ("stopped") posta o snapshot final uma vez e encerra, em vez de exigir que o
/// monitor testemunhe a transiÃ§Ã£o pra active (que jÃ¡ aconteceu antes de anexarmos).
fn run_monitor(weak: Weak<AppWindow>, project: PathBuf, stop: Arc<AtomicBool>, attach: bool) {
    std::thread::spawn(move || {
        let mut seen_active = attach;
        // Arranque: tokens uma vez (thread prÃ³pria) â o resto Ã© relido a cada ciclo.
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
            // Tokens: no MÃX. a cada 30s, sempre fora do event loop.
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
            // Dorme em fatias curtas pra responder rÃ¡pido ao botÃ£o Parar.
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
// GRAFO â porte do graph_view + step_graph do egui (schematize_cli_rs::gui).
// A fÃ­sica Ã© force-directed em Rust (repulsÃ£o O(nÂ²) + molas nas arestas +
// gravidade/centralizaÃ§Ã£o + damping), rodada num `slint::Timer` a ~60fps que
// RELAXA (para) quando a energia (alpha) cai. As posiÃ§Ãµes (mundo) vivem aqui; a
// cada tick empurramos os `VecModel` de nÃ³s/arestas que o `.slint` renderiza. A
// transformaÃ§Ã£o (scale/ox/oy) tambÃ©m Ã© dona daqui (o Slint sÃ³ a lÃª pra desenhar).
// InteraÃ§Ã£o (pan/zoom/arrasto/clique) chega crua do Slint; o hit-test Ã© aqui.
// ===========================================================================

/// Raio de um nÃ³ EM PIXELS de tela (constante ao zoom) â idÃªntico ao egui.
fn nsize(deg: f32) -> f32 {
    3.0 + (deg.sqrt() * 1.7).min(9.0)
}

/// RÃ³tulo curto do nÃ³ (id truncado, seguro a UTF-8) â o egui truncava em 33+"â¦".
fn trunc_label(id: &str) -> String {
    if id.chars().count() > 34 {
        let s: String = id.chars().take(33).collect();
        format!("{s}â¦")
    } else {
        id.to_string()
    }
}

/// NÃ³ com estado de simulaÃ§Ã£o + flags de realce (recomputadas em `refresh_flags`).
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
    // Drill-down: `Some(servico)` = vendo o grafo DETALHADO daquele microserviço
    // (`.schematize/grafos/<servico>.md`); `None` = a visão GLOBAL da aplicação.
    service: Option<String>,
    // DescriÃ§Ã£o por nÃ³ (nome -> "O quÃª"), vinda do Ã­ndice/MAPA (Â§39). Carregada
    // junto do grafo; consultada ao selecionar um nÃ³ pra mostrar no bloco lateral.
    descs: HashMap<String, String>,
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
    // `true` quando o grafo global foi AGREGADO por microserviço (índice flat > cap, sem
    // GRAFO_GLOBAL.md): cada nó é um serviço ("<serviço> · N funções") e o drill abre o detalhe.
    aggregated: bool,
}

impl GraphState {
    /// Um passo da fÃ­sica (idÃªntico ao `step_graph` do egui). No-op se relaxado.
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

    /// Tela (px relativo ao canvas) â mundo, desfazendo o centro + pan + zoom.
    fn to_world(&self, mx: f32, my: f32) -> (f32, f32) {
        let cx = self.canvas_w / 2.0;
        let cy = self.canvas_h / 2.0;
        ((mx - cx - self.ox) / self.scale, (my - cy - self.oy) / self.scale)
    }

    /// NÃ³ sob o ponto de mundo (raio de tela convertido pra mundo) â como o egui.
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

    /// Enquadra todos os nÃ³s no canvas atual (idÃªntico ao `fit` do egui).
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

    /// Recomputa selected/hot/dim (sÃ³ muda em seleÃ§Ã£o/busca â nÃ£o a cada tick).
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

/// ConstrÃ³i uma linha do modelo de nÃ³s a partir do estado de simulaÃ§Ã£o.
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

/// ConstrÃ³i uma linha do modelo de arestas (pontas em mundo + realce).
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

/// Empurra TUDO pro Slint: transformaÃ§Ã£o (props), seleÃ§Ã£o (info) e os dois
/// VecModel (nÃ³s/arestas). Atualiza in-place quando o tamanho casa (sem realloc
/// no loop da fÃ­sica); senÃ£o troca o vec inteiro (carga/relayout).
fn graph_sync(app: &AppWindow, st: &GraphState, nodes: &VecModel<GraphNode>, edges: &VecModel<GraphEdge>) {
    app.set_g_scale(st.scale);
    app.set_g_ox(st.ox);
    app.set_g_oy(st.oy);
    app.set_g_has_graph(!st.nodes.is_empty());
    app.set_g_node_count(st.nodes.len() as i32);
    // Drill-down: `service` Some = vendo o grafo detalhado daquele microserviço.
    app.set_g_in_service(st.service.is_some());
    app.set_g_service_name(st.service.clone().unwrap_or_default().into());
    match st.sel {
        Some(i) => {
            app.set_g_has_sel(true);
            app.set_g_sel_id(st.nodes[i].id.clone().into());
            app.set_g_sel_loc(st.nodes[i].loc.clone().unwrap_or_default().into());
            // descriÃ§Ã£o do nÃ³ selecionado (por nome). "" â o Slint mostra a dica de reindexar.
            let desc = st.descs.get(&st.nodes[i].id).cloned().unwrap_or_default();
            app.set_g_sel_desc(desc.into());
        }
        None => {
            app.set_g_has_sel(false);
            app.set_g_sel_id(SharedString::new());
            app.set_g_sel_loc(SharedString::new());
            app.set_g_sel_desc(SharedString::new());
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
/// `reload_project` do egui: posiÃ§Ãµes em espiral inicial, graus, e fit pendente.
fn load_graph_into(st: &mut GraphState, proj: Option<&Path>) {
    st.nodes.clear();
    st.edges.clear();
    st.descs.clear();
    st.sel = None;
    st.search.clear();
    st.drag_node = None;
    st.moved = false;
    st.scale = 1.0;
    st.ox = 0.0;
    st.oy = 0.0;
    st.alpha = 1.0;
    st.service = None; // visão GLOBAL (drill-down sai)
    st.project = proj.map(|p| p.to_path_buf());
    let Some(p) = proj else {
        return;
    };
    // Grafo global PAGINADO: GRAFO_GLOBAL.md autorado; senão, se o flat passar do cap, AGREGA por
    // microserviço (1 nó por serviço) — o "1600 nós ilegíveis" vira mapa mental; o drill ("Grafo do
    // serviço") abre o detalhe via `load_service_graph` (fallback pelo flat).
    let (nodes, edges, _idx, aggregated) = panel::load_graph_global(p);
    st.aggregated = aggregated;
    // descriÃ§Ãµes dos nÃ³s (nome -> "O quÃª") do Ã­ndice/MAPA, guardadas p/ o bloco lateral.
    st.descs = panel::node_descriptions(p);
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.clone(), i);
        let a = i as f32 * 2.399_963; // Ã¢ngulo Ã¡ureo â espiral inicial (evita sobreposiÃ§Ã£o)
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

/// Drill-down: carrega o grafo DETALHADO de UM microserviço (`.schematize/grafos/<servico>.md`)
/// no estado. Devolve `true` se achou/carregou (nós > 0); `false` se não há grafo detalhado desse
/// serviço — nesse caso o chamador mantém a visão atual e avisa (não zera a tela à toa).
fn load_service_into(st: &mut GraphState, proj: Option<&Path>, servico: &str) -> bool {
    let Some(p) = proj else {
        return false;
    };
    let (nodes, edges) = panel::load_service_graph(p, servico);
    if nodes.is_empty() {
        return false;
    }
    st.nodes.clear();
    st.edges.clear();
    st.descs.clear();
    st.sel = None;
    st.search.clear();
    st.drag_node = None;
    st.moved = false;
    st.scale = 1.0;
    st.ox = 0.0;
    st.oy = 0.0;
    st.alpha = 1.0;
    st.service = Some(servico.to_string());
    st.project = Some(p.to_path_buf());
    st.descs = panel::node_descriptions(p);
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.clone(), i);
        let a = i as f32 * 2.399_963;
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
    true
}

/// (Re)liga o Timer da fÃ­sica se estiver parado. O tick roda um passo, sincroniza,
/// e PARA (relaxa) quando alpha < 0.02 â nÃ£o queima CPU parado. Reinicia via arrasto/carga.
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

/// Carrega o grafo do projeto no estado, sincroniza e (re)liga a fÃ­sica.
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

// ===========================================================================
// DATABASE BUILDER (tela 6) â modelo de tabelas + grafo do schema.
// O Schema canÃ´nico vive num `Arc<Mutex<database::Schema>>` (Send+Sync: cruza pra
// a thread da introspecÃ§Ã£o Postgres E Ã© lido pelos callbacks na UI thread). A UI
// reflete via models Slint REMONTADOS no event loop. O grafo do schema reusa a
// MESMA engine forÃ§a-dirigida (GraphState) num estado DEDICADO â nÃ£o colide com o
// grafo do Ã­ndice Â§39 da aba Grafo.
// ===========================================================================

/// ConstrÃ³i as linhas do modelo de tabelas (colunas/FKs/Ã­ndices aninhados) a partir
/// de um `database::Schema`. Roda no event loop (cria VecModels novos por tabela).
fn build_db_table_rows(schema: &database::Schema) -> Vec<DbTableRow> {
    schema
        .tables
        .iter()
        .map(|t| {
            let cols: Vec<DbColumn> = t
                .columns
                .iter()
                .map(|c| DbColumn {
                    name: c.name.clone().into(),
                    ty: c.ty.clone().into(),
                    nullable: c.nullable,
                    pk: c.pk,
                    unique: c.unique,
                })
                .collect();
            let fks: Vec<DbFkRow> = t
                .fks
                .iter()
                .map(|f| DbFkRow {
                    column: f.column.clone().into(),
                    ref_table: f.ref_table.clone().into(),
                    ref_column: f.ref_column.clone().into(),
                })
                .collect();
            let idxs: Vec<DbIndexRow> = t
                .indexes
                .iter()
                .map(|ix| DbIndexRow {
                    name: ix.name.clone().into(),
                    columns_text: ix.columns.join(", ").into(),
                    unique: ix.unique,
                })
                .collect();
            DbTableRow {
                name: t.name.clone().into(),
                columns: ModelRc::from(Rc::new(VecModel::from(cols))),
                fks: ModelRc::from(Rc::new(VecModel::from(fks))),
                indexes: ModelRc::from(Rc::new(VecModel::from(idxs))),
            }
        })
        .collect()
}

/// Reflete o `database::Schema` na UI: modelo de tabelas + nomes (dropdown) + flag
/// has-schema. MantÃ©m a tabela alvo do editor se ainda existir; senÃ£o pega a 1Âª.
fn db_rebuild(app: &AppWindow, schema: &database::Schema) {
    app.set_db_tables(ModelRc::from(Rc::new(VecModel::from(build_db_table_rows(schema)))));
    let names: Vec<String> = schema.tables.iter().map(|t| t.name.clone()).collect();
    app.set_db_table_names(strings_model(names.clone()));
    app.set_db_has_schema(!schema.tables.is_empty());
    let sel = app.get_db_sel_table().to_string();
    if !names.iter().any(|n| n == &sel) {
        app.set_db_sel_table(names.first().cloned().unwrap_or_default().into());
    }
}

/// Carrega um grafo jÃ¡ pronto (nÃ³s/arestas/descriÃ§Ãµes vindos de `database::to_graph`
/// + `table_descriptions`) no estado â sem projeto no disco. Espelha o arranjo em
/// espiral + graus + fit pendente do `load_graph_into`.
fn load_db_graph_into(
    st: &mut GraphState,
    nodes: Vec<panel::Node>,
    edges: Vec<panel::Edge>,
    descs: HashMap<String, String>,
) {
    st.nodes.clear();
    st.edges.clear();
    st.descs = descs;
    st.sel = None;
    st.search.clear();
    st.drag_node = None;
    st.moved = false;
    st.scale = 1.0;
    st.ox = 0.0;
    st.oy = 0.0;
    st.alpha = 1.0;
    st.project = None;
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.clone(), i);
        let a = i as f32 * 2.399_963;
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

/// Como `graph_sync`, mas escreve as propriedades `db-g-*` do grafo do SCHEMA (sem
/// arquivo:linha â tabela nÃ£o tem local no cÃ³digo; sÃ³ nome + descriÃ§Ã£o das colunas).
fn db_graph_sync(app: &AppWindow, st: &GraphState, nodes: &VecModel<GraphNode>, edges: &VecModel<GraphEdge>) {
    app.set_db_g_scale(st.scale);
    app.set_db_g_ox(st.ox);
    app.set_db_g_oy(st.oy);
    app.set_db_g_has_graph(!st.nodes.is_empty());
    app.set_db_g_node_count(st.nodes.len() as i32);
    match st.sel {
        Some(i) => {
            app.set_db_g_has_sel(true);
            app.set_db_g_sel_id(st.nodes[i].id.clone().into());
            let desc = st.descs.get(&st.nodes[i].id).cloned().unwrap_or_default();
            app.set_db_g_sel_desc(desc.into());
        }
        None => {
            app.set_db_g_has_sel(false);
            app.set_db_g_sel_id(SharedString::new());
            app.set_db_g_sel_desc(SharedString::new());
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

/// (Re)liga o Timer da fÃ­sica do grafo do SCHEMA (relaxa quando alpha < 0.02).
fn db_graph_kick(
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
        db_graph_sync(&app, &st, &nodes, &edges);
        if st.alpha < 0.02 {
            timer2.stop();
        }
    });
}

/// Monta o prompt em LINGUAGEM NATURAL do "gerar por descriÃ§Ã£o (IA)": pede pra seguir
/// a disciplina de modelagem da casa (schematize-database) e emitir schema.json +
/// schema.sql + migration em `<projeto>_archive/database/`, a partir da descriÃ§Ã£o do
/// usuÃ¡rio. NÃO usa o slash `/database-design` (nÃ£o roda como arg do claude).
fn db_ai_prompt(project_basename: &str, desc: &str) -> String {
    format!(
        "Modele o banco de dados deste projeto a partir da descriÃ§Ã£o de domÃ­nio no fim desta mensagem, \
         usando a DISCIPLINA DE MODELAGEM DA CASA (a skill schematize-database): normalizaÃ§Ã£o 1FNâ3FN, \
         PK surrogate ULID/UUIDv7 interna + a chave natural como UNIQUE (identidade â  email), tipos corretos \
         por coluna (dinheiro em inteiro/numeric, tempo em timestamptz UTC, enum como domÃ­nio), constraints \
         conscientes (NOT NULL/default, UNIQUE, CHECK, FOREIGN KEY com ON DELETE consciente), Ã­ndices \
         conscientes (sem redundÃ¢ncia; PII nunca vira chave de Ã­ndice) e o piso de privacidade (coluna PII \
         marcada, base legal + retenÃ§Ã£o â LGPD). \
         EMITA os artefatos na pasta `{proj}_archive/database/` (crie se nÃ£o existir): \
         (1) `schema.json` no formato do database builder â um objeto JSON com `tables` (array), cada tabela \
         com `name`, `columns` (cada uma {{name, ty, nullable, pk, unique}}), `fks` (cada uma \
         {{column, ref_table, ref_column}}) e `indexes` (cada um {{name, columns[], unique}}); \
         (2) `schema.sql` com os CREATE TABLE; (3) `migration.sql` no estilo expand-contract reversÃ­vel. \
         NÃ£o pare enquanto os trÃªs arquivos nÃ£o estiverem gravados e consistentes.\n\n\
         DescriÃ§Ã£o do domÃ­nio:\n{desc}",
        proj = project_basename,
        desc = desc,
    )
}

/// (Re)carrega o histÃ³rico do DB do overdev + os commits do projeto `proj` nos
/// modelos, reseta a paginaÃ§Ã£o e escreve a linha de upstream. None â limpa tudo.
/// SÃ­ncrono (sqlite/git locais e rÃ¡pidos â mesma escolha do env status).
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
    // Logo da janela (tÃ­tulo/taskbar) â mesma marca do egui.
    // Ícone da janela DESENHADO em código (resiliente — sem depender de arquivo).
    app.set_app_icon(make_app_icon());
    // Ações declaradas por skills instaladas (gui.json) → botões (Q.A., Pentest, …) na aba do projeto.
    app.set_skill_actions(ModelRc::from(Rc::new(VecModel::from(skill_action_rows()))));
    // VersÃ£o do app (ConfiguraÃ§Ãµes) â ex.: "schematize v0.25.1".
    app.set_app_version(format!("schematize v{}", upgrade::app_version()).into());
    app.set_rows(ModelRc::from(model.clone()));
    update_status(&app);
    recompute_headers(&app); // esconde cabeÃ§alhos de pÃ¡gina sem itens

    // PÃ¡gina inicial: Instaladas (0). Se NADA estiver instalado, abre no
    // Marketplace (1) â senÃ£o o usuÃ¡rio cai numa lista vazia.
    if !model.iter().any(|r| !r.is_header && is_installed(&r)) {
        app.set_active_tab(1);
    }
    // Recomputa a paginaÃ§Ã£o para a aba inicial efetiva (o handler `changed active-tab`
    // ainda nÃ£o estÃ¡ ligado neste ponto â recomputa explicitamente).
    recompute_pagination(&app);

    // Resolve o latest de todas as skills assim que a janela sobe (nÃ£o bloqueia).
    kick_resolve_all(&app.as_weak(), &row_items);
    // Busca as notas do marketplace (1 request, thread) e preenche os badges por slug.
    kick_market_ratings(app.as_weak());

    // ---- aba Environments: modelo + Ã­ndices auxiliares p/ o modal ----
    // Sonda a mÃ¡quina UMA vez (local, rÃ¡pido pra command -v). O refresh re-sonda.
    let env_status = environments::status();
    let env_model = Rc::new(VecModel::from(build_env_rows_from(&env_status)));
    app.set_env_rows(ModelRc::from(env_model.clone()));
    // lang â mÃ©todos disponÃ­veis (slugs), pra o modal montar os chips sem re-sondar.
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
    // conjunto das 7 linguagens que TÃM environment (pra decidir a oferta no modal).
    // SÃ³ categoria "language" â ferramentas (claude/code/codex) nÃ£o entram na oferta.
    let env_langs: Rc<std::collections::HashSet<String>> = Rc::new(
        env_status
            .iter()
            .filter(|le| le.category == "language")
            .map(|le| le.lang.to_string())
            .collect(),
    );
    // estado do modal de instalaÃ§Ã£o (lado Rust).
    let modal = Rc::new(RefCell::new(ModalState::default()));

    // ---- relanÃ§ar o app (janela nova) â conserto do restart pÃ³s self-update ----
    // O callback existe e estÃ¡ ligado ao helper CORRETO (spawn desacoplado antes
    // de sair). Hoje o self-update NÃO estÃ¡ fiado nesta GUI Slint (mora no mÃ³dulo
    // egui do lib, fora do alcance daqui), entÃ£o nada dispara `restart()` ainda;
    // quando o self-update for portado pra cÃ¡, Ã© sÃ³ invocar `root.restart()`.
    app.on_restart(move || restart_app());

    // ---- toggle de seleÃ§Ã£o de uma linha ----
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

    // ---- selecionar todas (da PÃGINA ativa) ----
    // Instaladas (tab 0) â todas as instaladas; Marketplace (tab 1) â todas as
    // nÃ£o-instaladas. NÃ£o toca em linhas da outra pÃ¡gina.
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
    // ---- selecionar pendentes (sÃ³ Instaladas): instaladas-DESATUALIZADAS ----
    {
        let model = model.clone();
        app.on_select_pending(move || {
            for i in 0..model.row_count() {
                if let Some(mut r) = model.row_data(i) {
                    if !r.is_header {
                        r.selected = is_outdated(&r); // nunca marca uma nÃ£o-instalada
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

    // ---- Marketplace: aÃ§Ã£o por-linha INSTALAR ----
    // Skill de linguagem (ou skill com recommends) â abre o MODAL: oferece instalar
    // a recomendada (base) junto E, opcionalmente, o environment da linguagem (via
    // terminal). Skill sem nada a oferecer â instala direto (um clique).
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
            // recomendada a oferecer: 1Âª recomendada que NÃO estÃ¡ instalada.
            let rec_slug = it
                .recommends
                .iter()
                .find(|s| !slug_installed(&model, s.as_str()))
                .cloned()
                .unwrap_or_default();
            // environment a oferecer: se o slug da skill Ã© uma das 7 linguagens.
            let env_lang = if env_langs.contains(it.slug.as_str()) {
                it.slug.clone()
            } else {
                String::new()
            };
            // Nada a oferecer â instala direto, sem modal.
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
            // dependÃªncia opcional (base recomendada) â NUNCA marcada por padrÃ£o.
            let rec_show = !rec_slug.is_empty();
            app.set_mp_rec_show(rec_show);
            app.set_mp_rec_check(false);
            if rec_show {
                app.set_mp_rec_label(tf("gui.mp_with_recommended", &[("slug", &rec_slug)]).into());
            }
            // environment opcional â NUNCA marcado por padrÃ£o.
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

    // ---- Instaladas: aÃ§Ã£o por-linha ATUALIZAR ----
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

    // ---- Instaladas: aÃ§Ã£o por-linha DESINSTALAR ----
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

    // ---- Marketplace: INSTALAR selecionadas (sÃ³ as nÃ£o-instaladas) ----
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

    // ---- Instaladas: ATUALIZAR selecionadas (sÃ³ instaladas-desatualizadas) ----
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

    // ---- Instaladas: DESINSTALAR selecionadas (sÃ³ instaladas) ----
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
    // GARANTIA: sÃ³ instaladas-DESATUALIZADAS (is_outdated âº installed Some E
    // latest > installed). JAMAIS instala uma skill nÃ£o instalada.
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

    // ---- rechecar versÃµes (re-resolve latest) ----
    {
        let weak = app.as_weak();
        let row_items = row_items.clone();
        app.on_check(move || {
            kick_resolve_all(&weak, &row_items);
            kick_market_ratings(weak.clone());
        });
    }

    // ==================== aba Environments ====================

    // escolher o mÃ©todo (chip) de uma linha de environment.
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
    // instalar o environment da linha â abre TERMINAL com `schematize env install`.
    {
        let env_model = env_model.clone();
        app.on_env_install(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                // Linguagem exige mÃ©todo escolhido; ferramenta ("tool") nÃ£o tem seletor.
                if r.category != "tool" && r.method_sel.is_empty() {
                    return;
                }
                let label = run_env_action("install", &r.lang.to_string(), &r.method_sel.to_string());
                r.op_label = label.into();
                env_model.set_row_data(i, r);
            }
        });
    }
    // desinstalar o environment da linha â abre TERMINAL com `schematize env remove`.
    {
        let env_model = env_model.clone();
        app.on_env_remove(move |idx| {
            let i = idx as usize;
            if let Some(mut r) = env_model.row_data(i) {
                // Linguagem exige mÃ©todo; ferramenta nÃ£o (o CLI ignora `--method`).
                if r.category != "tool" && r.method_sel.is_empty() {
                    return;
                }
                let label = run_env_action("remove", &r.lang.to_string(), &r.method_sel.to_string());
                r.op_label = label.into();
                env_model.set_row_data(i, r);
            }
        });
    }
    // recarregar o status (re-sonda a mÃ¡quina). SÃ­ncrono (local/rÃ¡pido; evita !Send).
    {
        let env_model = env_model.clone();
        app.on_env_refresh(move || {
            env_model.set_vec(build_env_rows());
        });
    }

    // ==================== modal de instalaÃ§Ã£o (Marketplace) ====================

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
            // lote in-process: a skill + (recomendada SÃ se o usuÃ¡rio marcou).
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
            // environment opcional â terminal (sÃ³ se marcado + mÃ©todo escolhido).
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
    // devolvem Ã  UI via `invoke_from_event_loop` (padrÃ£o threadâUI do Slint). O
    // estado do form/editor mora em propriedades do app (nada de Rc !Send cruzando
    // a fronteira da thread â os modelos sÃ£o REMONTADOS no event loop).
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

    // validar o slug a cada tecla (puro/rÃ¡pido â sem IO). Atualiza slug + erro inline.
    {
        let weak = app.as_weak();
        app.on_mg_slug_edited(move |s| {
            if let Some(app) = weak.upgrade() {
                let slug = s.to_string();
                app.set_mg_slug(s);
                // vazio â sem erro (sÃ³ desabilita o botÃ£o); invÃ¡lido â mostra o hint.
                let err = if slug.is_empty() || skilledit::validate_slug(&slug).is_ok() {
                    String::new()
                } else {
                    tor("gui.slug_invalid", "slug invÃ¡lido â use sÃ³ [a-z0-9-], comeÃ§ando por letra/dÃ­gito")
                };
                app.set_mg_slug_error(err.into());
            }
        });
    }

    // criar a skill â skilledit::scaffold(slug, nome, desc). Sucesso mostra o caminho
    // e re-sonda o dropdown; erro (ex.: jÃ¡ existe) mostra a mensagem.
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
                    tor("gui.slug_invalid", "slug invÃ¡lido â use sÃ³ [a-z0-9-], comeÃ§ando por letra/dÃ­gito").into(),
                );
                return;
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skilledit::scaffold(&slug, &name, &desc);
                // criou â jÃ¡ re-sonda a lista (passa a incluir a nova skill).
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
                                // "jÃ¡ existe" ganha mensagem amigÃ¡vel; senÃ£o a msg do lib.
                                let msg = if e.contains("jÃ¡ existe") {
                                    tor("gui.skill_exists", "essa skill jÃ¡ existe")
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

    // pÃ³s-criar: pula pro modo Editar jÃ¡ com a skill recÃ©m-criada carregada.
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

    // escolher uma skill â lista os arquivos editÃ¡veis (skilledit::list_files).
    {
        let weak = app.as_weak();
        app.on_mg_pick_skill(move |s| {
            let Some(app) = weak.upgrade() else { return };
            let slug = s.to_string();
            app.set_mg_sel_skill(s);
            // troca de skill zera a seleÃ§Ã£o de arquivo/editor/feedback.
            app.set_mg_sel_file(SharedString::new());
            app.set_mg_content(SharedString::new());
            app.set_mg_save_result(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                // lista os arquivos + status de FORK (oficial? jÃ¡ forkada?) da skill escolhida.
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

    // escolher um arquivo â carrega o conteÃºdo no editor (skilledit::read_file).
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

    // salvar o editor â grava LOCAL em ~/.claude/skills (skilledit::write_file).
    // `write_file` jÃ¡ FORKA automaticamente uma skill oficial antes de gravar; apÃ³s
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
                // pÃ³s-gravaÃ§Ã£o: a skill oficial pode ter virado fork agora.
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
    // Estado (dono da fÃ­sica + transformaÃ§Ã£o), dois VecModel (nÃ³s/arestas) e o
    // Timer da fÃ­sica. O grafo COMPARTILHA o projeto com a aba Overdev â carregado
    // junto na seleÃ§Ã£o/restauraÃ§Ã£o de projeto (mais abaixo).
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
    // Projeto atual (lado Rust) â persiste entre execuÃ§Ãµes via recent_projects.
    let od_current: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    // Fase 4: flag de parada do MONITOR (o botÃ£o Parar a levanta; o monitor a checa
    // a cada fatia e encerra). `Arc` porque cruza pra a thread do monitor. NÃO mata o
    // `claude` â ele roda no terminal externo (processo prÃ³prio); sÃ³ para o espelho.
    let od_stop_flag = Arc::new(AtomicBool::new(false));
    // HistÃ³rico do DB do overdev + commits (aditivos): estado completo no Rust +
    // modelos com a PÃGINA atual (paginaÃ§Ã£o Rust-side).
    let od_snaps_all: Rc<RefCell<Vec<overdevdb::SnapshotMeta>>> = Rc::new(RefCell::new(Vec::new()));
    let od_snaps_model = Rc::new(VecModel::<SnapRow>::from(Vec::new()));
    let od_commits_all: Rc<RefCell<Vec<githist::Commit>>> = Rc::new(RefCell::new(Vec::new()));
    let od_commits_model = Rc::new(VecModel::<CommitRow>::from(Vec::new()));
    app.set_od_snaps(ModelRc::from(od_snaps_model.clone()));
    app.set_od_commits(ModelRc::from(od_commits_model.clone()));
    // Restaura a Ãºltima escolha (mais recente), senÃ£o empty-state.
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
    // Governador de concorrência: "Rechecar" recomputa o teto (CPU/RAM/load − claudes-da-máquina).
    {
        let weak = app.as_weak();
        app.on_od_refresh_agents(move || {
            if let Some(app) = weak.upgrade() {
                apply_agent_budget(&app);
            }
        });
    }
    // Ação de skill (Q.A./Pentest/…): dispara o `command` (ex.: /eng-qa) no claude de um terminal
    // externo, no projeto selecionado. É o botão declarado via gui.json.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_run_skill_action(move |command| {
            let Some(app) = weak.upgrade() else { return };
            let Some(project) = cur.borrow().clone() else {
                app.set_od_run_status(tor("gui.od_no_project", "Escolha um projeto primeiro.").into());
                return;
            };
            match schematize::agentrun::launch_prompt_in_terminal(&project, command.as_str()) {
                Ok(_) => app.set_od_run_status(format!("Rodando {command} num terminal externo…").into()),
                Err(e) => app.set_od_run_status(format!("falhou ao rodar {command}: {e}").into()),
            }
        });
    }
    // Split multiagent: divide o checklist em K parts (checklist/part-NN.md) respeitando o teto do
    // governador; com dispatch, lança K claudes (um por fatia), cada um limitado a subagents_each.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_split(move |k, dispatch| {
            let Some(app) = weak.upgrade() else { return };
            let Some(project) = cur.borrow().clone() else {
                app.set_od_split_status("Selecione um projeto primeiro.".into());
                return;
            };
            let k = (k.max(2)) as usize;
            let b = schematize::agents::budget();
            if k > b.total_cap {
                app.set_od_split_status(
                    format!("{k} claudes passa do teto seguro ({}). Reduza o K.", b.total_cap).into(),
                );
                return;
            }
            match schematize::overdev::split(&project, k) {
                Ok(res) => {
                    let plan = b.split_plan(k);
                    let mut msg = format!(
                        "✓ {} itens em {} parte(s) (checklist/part-*.md) · {} subagents por claude.",
                        res.moved,
                        res.parts.len(),
                        plan.subagents_each
                    );
                    if dispatch {
                        if b.available < k {
                            msg.push_str(&format!(" NÃO lancei: só {} slot(s) livre(s) agora.", b.available));
                        } else {
                            let mut ok = 0;
                            for f in &res.parts {
                                let rel = f.strip_prefix(&project).unwrap_or(f);
                                let prompt = format!(
                                    "Rode o overdev deste projeto cuidando APENAS de `{}` (sua fatia do split). \
                                     Feche TODOS os itens `- [ ]` dele com prova. Você pode usar até {} subagents \
                                     em paralelo — NÃO ultrapasse (há outros claudes nas outras fatias). Não toque \
                                     nos outros part-*.md.",
                                    rel.display(),
                                    plan.subagents_each
                                );
                                if schematize::agentrun::launch_prompt_in_terminal(&project, &prompt).is_ok() {
                                    ok += 1;
                                }
                            }
                            msg.push_str(&format!(" Lancei {ok}/{} claude(s).", res.parts.len()));
                        }
                    }
                    app.set_od_split_status(msg.into());
                    apply_agent_budget(&app); // atualiza o "rodando/livre" após lançar
                }
                Err(e) => app.set_od_split_status(format!("split falhou: {e}").into()),
            }
        });
    }
    // "Gerar afazeres do archive" — dispara a skill schematize-archive (/archive-todos) num terminal
    // externo: varre o <projeto>_archive/ + git e deriva o .schematize/overdev/CHECKLIST.md do que
    // ficou aberto/prometido-e-não-provado. O archive é criticidade 0 (a skill cria se faltar).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_gen_from_archive(move || {
            let root = cur.borrow().clone();
            let Some(root) = root else {
                if let Some(app) = weak.upgrade() {
                    app.set_od_run_status(tor("gui.od_no_project", "Escolha um projeto primeiro.").into());
                }
                return;
            };
            if let Some(app) = weak.upgrade() {
                app.set_od_run_status(
                    tor("gui.od_gen_running", "Gerando afazeres do archive num terminal externo…").into(),
                );
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = agentrun::launch_prompt_in_terminal(&root, &agentrun::archive_todos_prompt());
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        match res {
                            Ok(term) => app.set_od_run_status(
                                format!("{} {}", tor("gui.od_gen_ok", "gerando no terminal"), term).into(),
                            ),
                            Err(e) => app.set_od_run_status(e.into()),
                        }
                    }
                });
            });
        });
    }
    // cadastrar um diretÃ³rio de desenvolvimento (picker nativo).
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
    // remover um diretÃ³rio de desenvolvimento.
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.on_od_remove_dev_dir(move |path| {
            config::remove_dev_dir(&path.to_string());
            refresh_proj_models(&pm, &dm, &pnm);
        });
    }
    // FIXAR uma pasta como projeto (picker nativo â config::pin_project). Uma pasta
    // fixada vira UM projeto no seletor mesmo sem marcador git (workspace/monorepo).
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.on_od_pin_folder(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(tor("gui.pin_folder", "Fixar pastaâ¦")).pick_folder() {
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
    // ---- Fase 3: marcar item HUMANO aberto como feito (- [H ]â- [H x]) ----
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
                        // salvar o CHECKLIST.md muda o estado 2-nÃ­veis: recarrega.
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
    // ---- Fase 3: prompt de correÃ§Ã£o do overdev (add_note kind="correcao") ----
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
    // ---- Fase 4: "Executar overdev" â passo 1: GUARDRAIL (mostra o comando) ----
    // NÃ£o dispara nada; sÃ³ mostra o comando que abrirÃ¡ no TERMINAL EXTERNO e abre o
    // mini-modal de confirmaÃ§Ã£o. O disparo real (launch_in_terminal) Ã© no `od-run-confirm`.
    {
        let weak = app.as_weak();
        app.on_od_run_request(move || {
            if let Some(app) = weak.upgrade() {
                app.set_od_agent_cmdline(
                    tor(
                        "gui.od_agent_cmdline",
                        "claude --dangerously-skip-permissions \"<prompt do overdev>\"  (terminal externo, processo prÃ³prio)",
                    )
                    .into(),
                );
                app.set_od_confirm_open(true);
            }
        });
    }
    // ---- Fase 4: guardrail â CANCELAR (fecha sem disparar) ----
    {
        let weak = app.as_weak();
        app.on_od_run_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.set_od_confirm_open(false);
            }
        });
    }
    // ---- Fase 4: guardrail â CONFIRMAR: abre o `claude` num TERMINAL EXTERNO ----
    // Chama `agentrun::launch_in_terminal` numa thread (processo prÃ³prio, RAM dele,
    // fora do app). Sucesso â mensagem "claude aberto no terminal <nome>â¦" + liga o
    // MONITOR leve. Erro (claude/terminal ausente) â mostra a msg e nÃ£o monitora.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let stop = od_stop_flag.clone();
        app.on_od_run_confirm(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if app.get_od_session_running() {
                    return; // jÃ¡ monitorando â nÃ£o dispara outro
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
                                        " â o overdev roda fora do app; acompanhe abaixo.",
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
    // ---- Fase 4: "Parar" â levanta a flag; o MONITOR encerra (nÃ£o mata o claude) ----
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
    // ---- Reload / Acompanhar: ANEXA o monitor a um overdev que jÃ¡ roda POR FORA ----
    // (terminal/processo prÃ³prio). Sem depender de ter clicado "Executar overdev":
    // (re)liga a `run_monitor` no projeto atual lendo o `.overdev/` do disco e passa
    // a espelhar ao vivo. Sem `.overdev/` â avisa e nÃ£o liga. Se um monitor jÃ¡ estÃ¡
    // vivo, sÃ³ reforÃ§a os tokens (o loop jÃ¡ reflete o resto). `attach=true` faz o
    // monitor seguir um run jÃ¡ em curso (ou postar 1x e encerrar se jÃ¡ finalizou).
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
            if !overdev_dir(&p).is_dir() {
                app.set_od_run_status(tor("gui.od_no_overdev_here", "nenhum overdev neste projeto").into());
                return;
            }
            // JÃ¡ monitorando: nÃ£o dispara outra thread (evita duplicata na mesma
            // `stop`); sÃ³ reforÃ§a os tokens agora.
            if app.get_od_session_running() {
                spawn_usage(weak.clone(), p.clone());
                app.set_od_run_status(tor("gui.od_attached", "acompanhando o overdev deste projetoâ¦").into());
                return;
            }
            // Zera o painel e liga o monitor anexado ao run externo.
            app.set_od_run_status(tor("gui.od_attached", "acompanhando o overdev deste projetoâ¦").into());
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
    // ---- "Atualizar tokens": relÃª `agent_usage` (PESADO) sob demanda, em thread ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_od_refresh_tokens(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if overdev_dir(&p).is_dir() {
                    spawn_usage(weak.clone(), p);
                } else {
                    app.set_od_run_status(tor("gui.od_no_overdev_here", "nenhum overdev neste projeto").into());
                }
            }
        });
    }

    // ==================== aba Grafo â interaÃ§Ã£o ====================
    // Ponteiro/roda chegam crus do Slint (coords relativas ao canvas, em px). O
    // hit-test e a decisÃ£o pan-vs-arrasto acontecem AQUI (como no egui). Cada
    // handler sincroniza o modelo/props no fim.

    // canvas mudou de tamanho â guarda; se havia fit pendente (carga), enquadra.
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
    // mouse-down: hit-test â fixa o nÃ³ a arrastar (com offset de pega) ou nada.
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
    // arrasto: nÃ³ fixo â move o nÃ³ (reaquece a fÃ­sica); senÃ£o â pan do fundo.
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
    // mouse-up: se nÃ£o houve arrasto, Ã© um CLIQUE â seleciona/deseleciona o nÃ³.
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
    // roda: zoom centrado no cursor (mesma matemÃ¡tica do egui).
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
    // botÃ£o "ajustar": enquadra tudo no canvas.
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
    // busca: realÃ§a/apaga nÃ³s por nome (recomputa flags e sincroniza).
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
    // clique em "abrir no editor" do nÃ³ selecionado â vscode://file/<abs>/â¦:<linha>.
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
    // exportar vault Obsidian do Ã­ndice (bÃ´nus â via panel::export_obsidian_at).
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
    // abrir o painel HTML (com o mesmo grafo) no navegador (bÃ´nus).
    {
        let gs = graph_state.clone();
        app.on_graph_open_browser(move || {
            let proj = gs.borrow().project.clone();
            if let Some(p) = proj {
                let _ = panel::open_in_browser(&p);
            }
        });
    }
    // "Reindexar" â chama a skill que organiza o grafo: dispara o Ã­ndice Â§39 (prompt NL) num
    // TERMINAL EXTERNO (processo prÃ³prio do `claude`, fora do app), numa thread. SÃ³ dados Send
    // cruzam (PathBuf + String). Sucesso â banner "Ã­ndice rodando no terminal <nome> â clique em
    // Recarregar quando terminar."; erro â a msg da lib (claude/terminal ausente).
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        app.on_graph_reindex(move || {
            let Some(root) = gs.borrow().project.clone() else {
                return;
            };
            let w = weak.clone();
            std::thread::spawn(move || {
                let res = agentrun::launch_prompt_in_terminal(&root, &agentrun::reindex_prompt());
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = w.upgrade() {
                        let msg = match res {
                            Ok(term) => format!(
                                "{}{}{}",
                                tor("gui.g_reindex_pre", "Ã­ndice rodando no terminal "),
                                term,
                                tor(
                                    "gui.g_reindex_post",
                                    " â clique em Recarregar quando terminar.",
                                ),
                            ),
                            Err(e) => e,
                        };
                        app.set_g_reindex_status(msg.into());
                    }
                });
            });
        });
    }
    // "Recarregar grafo" â re-roda load_graph + node_descriptions e atualiza a UI (apÃ³s o Ã­ndice
    // terminar no terminal). Limpa o banner do reindex.
    {
        let weak = app.as_weak();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_reload(move || {
            let proj = gs.borrow().project.clone();
            graph_load_and_kick(proj.as_deref(), &gt, &weak, &gs, &gn, &ge);
            if let Some(app) = weak.upgrade() {
                app.set_g_reindex_status(SharedString::new());
            }
        });
    }
    // Drill-down: abre o grafo DETALHADO do microserviço do nó selecionado
    // (`.schematize/grafos/<servico>.md`). Sem grafo detalhado desse serviço → avisa e mantém a visão.
    {
        let weak = app.as_weak();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_drill(move || {
            let (proj, servico) = {
                let st = gs.borrow();
                let servico = st.sel.map(|i| st.nodes[i].id.clone());
                (st.project.clone(), servico)
            };
            let Some(servico) = servico else { return };
            let carregou = load_service_into(&mut gs.borrow_mut(), proj.as_deref(), &servico);
            if let Some(app) = weak.upgrade() {
                if carregou {
                    graph_sync(&app, &gs.borrow(), &gn, &ge);
                    graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
                    app.set_g_reindex_status(SharedString::new());
                } else {
                    app.set_g_reindex_status(
                        tor("gui.g_no_service_graph", "sem grafo detalhado para este serviço (rode Reindexar).")
                            .into(),
                    );
                }
            }
        });
    }
    // Volta da visão por-serviço para o GRAFO GLOBAL da aplicação.
    {
        let weak = app.as_weak();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_global(move || {
            let proj = gs.borrow().project.clone();
            graph_load_and_kick(proj.as_deref(), &gt, &weak, &gs, &gn, &ge);
            if let Some(app) = weak.upgrade() {
                app.set_g_reindex_status(SharedString::new());
            }
        });
    }
    // "x" do bloco de info â deseleciona o nÃ³ (fecha o bloco) e ressincroniza.
    {
        let weak = app.as_weak();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.on_graph_clear_sel(move || {
            let mut st = gs.borrow_mut();
            st.sel = None;
            st.refresh_flags();
            if let Some(app) = weak.upgrade() {
                graph_sync(&app, &st, &gn, &ge);
            }
        });
    }

    // ==================== Database builder (tela 6) ====================
    // Schema canÃ´nico compartilhado (Send+Sync â cruza pra a thread do Postgres e Ã©
    // lido pelos callbacks na UI thread). Grafo do schema em estado DEDICADO.
    let db_schema: Arc<Mutex<database::Schema>> = Arc::new(Mutex::new(database::Schema::default()));
    let db_graph_state = Rc::new(RefCell::new(GraphState { scale: 1.0, alpha: 1.0, ..Default::default() }));
    let db_graph_nodes = Rc::new(VecModel::<GraphNode>::from(Vec::new()));
    let db_graph_edges = Rc::new(VecModel::<GraphEdge>::from(Vec::new()));
    let db_graph_timer = Rc::new(slint::Timer::default());
    app.set_db_graph_nodes(ModelRc::from(db_graph_nodes.clone()));
    app.set_db_graph_edges(ModelRc::from(db_graph_edges.clone()));
    db_rebuild(&app, &db_schema.lock().unwrap());

    // escolher arquivo SQLite (picker nativo).
    {
        let weak = app.as_weak();
        app.on_db_pick_sqlite(move || {
            if let Some(path) = rfd::FileDialog::new().set_title(t("gui.open_folder")).pick_file() {
                if let Some(app) = weak.upgrade() {
                    app.set_db_sqlite_path(path.to_string_lossy().into_owned().into());
                }
            }
        });
    }
    // introspectar SQLite â LOCAL e rÃ¡pido (arquivo); roda sÃ­ncrono e mutaciona o
    // schema direto (como env status / ssh). Erro claro se o arquivo nÃ£o abrir.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_introspect_sqlite(move || {
            let Some(app) = weak.upgrade() else { return };
            let path = app.get_db_sqlite_path().to_string();
            if path.is_empty() {
                return;
            }
            app.set_db_error(SharedString::new());
            match database::introspect_sqlite(Path::new(&path)) {
                Ok(schema) => {
                    let n = schema.tables.len();
                    *sh.lock().unwrap() = schema;
                    db_rebuild(&app, &sh.lock().unwrap());
                    app.set_db_status(format!("{} — {} tabela(s)", tor("gui.db_loaded", "Schema carregado"), n).into());
                    app.set_db_view(1);
                }
                Err(e) => app.set_db_error(e.into()),
            }
        });
    }
    // introspectar Postgres â usa `psql` (subprocesso, pode bloquear) â THREAD. O
    // Schema (Send) volta e Ã© gravado no lock DENTRO do event loop; a UI Ã© remontada.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_introspect_postgres(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_db_busy() {
                return;
            }
            let conn = app.get_db_pg_conn().to_string();
            if conn.trim().is_empty() {
                return;
            }
            app.set_db_busy(true);
            app.set_db_error(SharedString::new());
            app.set_db_status(SharedString::new());
            let weak = weak.clone();
            let sh = sh.clone();
            std::thread::spawn(move || {
                let res = database::introspect_postgres(&conn);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_db_busy(false);
                        match res {
                            Ok(schema) => {
                                let n = schema.tables.len();
                                *sh.lock().unwrap() = schema;
                                db_rebuild(&app, &sh.lock().unwrap());
                                app.set_db_status(format!("{} — {} tabela(s)", tor("gui.db_loaded", "Schema carregado"), n).into());
                                app.set_db_view(1);
                            }
                            Err(e) => app.set_db_error(e.into()),
                        }
                    }
                });
            });
        });
    }
    // carregar schema.json (picker â serde). Erro claro se o JSON nÃ£o casar.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_load_json(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("schema", &["json"]).pick_file() else {
                return;
            };
            app.set_db_error(SharedString::new());
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<database::Schema>(&s).map_err(|e| e.to_string()))
            {
                Ok(schema) => {
                    let n = schema.tables.len();
                    *sh.lock().unwrap() = schema;
                    db_rebuild(&app, &sh.lock().unwrap());
                    app.set_db_status(format!("{} — {} tabela(s)", tor("gui.db_loaded", "Schema carregado"), n).into());
                    app.set_db_view(1);
                }
                Err(e) => app.set_db_error(tf("err.prefix", &[("e", &e)]).into()),
            }
        });
    }
    // salvar schema.json (picker â serde pretty).
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_save_json(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new()
                .add_filter("schema", &["json"])
                .set_file_name("schema.json")
                .save_file()
            else {
                return;
            };
            let json = serde_json::to_string_pretty(&*sh.lock().unwrap()).unwrap_or_default();
            match std::fs::write(&path, json) {
                Ok(()) => app.set_db_status(
                    format!("{} {}", tor("gui.db_saved", "schema.json salvo em"), path.display()).into(),
                ),
                Err(e) => app.set_db_error(tf("err.prefix", &[("e", &e.to_string())]).into()),
            }
        });
    }
    // escolher a tabela alvo do editor (coluna/FK).
    {
        let weak = app.as_weak();
        app.on_db_pick_table(move |name| {
            if let Some(app) = weak.upgrade() {
                app.set_db_sel_table(name);
            }
        });
    }
    // adicionar uma tabela vazia.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_add_table(move || {
            let Some(app) = weak.upgrade() else { return };
            let name = app.get_db_new_table().to_string();
            if name.trim().is_empty() {
                return;
            }
            {
                let mut s = sh.lock().unwrap();
                if s.tables.iter().any(|t| t.name == name) {
                    app.set_db_error(tor("gui.db_table_exists", "jÃ¡ existe uma tabela com esse nome").into());
                    return;
                }
                s.tables.push(database::Table { name: name.clone(), ..Default::default() });
            }
            app.set_db_error(SharedString::new());
            app.set_db_new_table(SharedString::new());
            app.set_db_sel_table(name.into());
            db_rebuild(&app, &sh.lock().unwrap());
        });
    }
    // adicionar uma coluna Ã  tabela alvo.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_add_column(move || {
            let Some(app) = weak.upgrade() else { return };
            let table = app.get_db_sel_table().to_string();
            let name = app.get_db_new_col().to_string();
            if table.is_empty() || name.trim().is_empty() {
                return;
            }
            let ty = app.get_db_new_col_type().to_string();
            let col = database::Column {
                name: name.clone(),
                ty: if ty.trim().is_empty() { "TEXT".into() } else { ty },
                nullable: app.get_db_new_col_nullable(),
                pk: app.get_db_new_col_pk(),
                unique: app.get_db_new_col_unique(),
            };
            {
                let mut s = sh.lock().unwrap();
                if let Some(t) = s.tables.iter_mut().find(|t| t.name == table) {
                    t.columns.push(col);
                }
            }
            app.set_db_new_col(SharedString::new());
            db_rebuild(&app, &sh.lock().unwrap());
        });
    }
    // adicionar uma FK Ã  tabela alvo.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_add_fk(move || {
            let Some(app) = weak.upgrade() else { return };
            let table = app.get_db_sel_table().to_string();
            let column = app.get_db_fk_col().to_string();
            let ref_table = app.get_db_fk_reftable().to_string();
            let ref_column = app.get_db_fk_refcol().to_string();
            if table.is_empty() || column.trim().is_empty() || ref_table.trim().is_empty() {
                return;
            }
            {
                let mut s = sh.lock().unwrap();
                if let Some(t) = s.tables.iter_mut().find(|t| t.name == table) {
                    t.fks.push(database::Fk {
                        column,
                        ref_table,
                        ref_column: if ref_column.trim().is_empty() { "id".into() } else { ref_column },
                    });
                }
            }
            app.set_db_fk_col(SharedString::new());
            app.set_db_fk_reftable(SharedString::new());
            app.set_db_fk_refcol(SharedString::new());
            db_rebuild(&app, &sh.lock().unwrap());
        });
    }
    // gerar SQL (to_sql) â visor read-only.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_gen_sql(move || {
            if let Some(app) = weak.upgrade() {
                app.set_db_gen_title(tor("gui.db_gen_sql", "Gerar SQL").into());
                app.set_db_gen_content(database::to_sql(&sh.lock().unwrap()).into());
                app.set_db_gen_open(true);
            }
        });
    }
    // gerar migration (to_migration) â visor read-only.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        app.on_db_gen_migration(move || {
            if let Some(app) = weak.upgrade() {
                app.set_db_gen_title(tor("gui.db_gen_migration", "Gerar migration").into());
                app.set_db_gen_content(database::to_migration(&sh.lock().unwrap()).into());
                app.set_db_gen_open(true);
            }
        });
    }
    // salvar o conteÃºdo do visor num arquivo (picker nativo).
    {
        let weak = app.as_weak();
        app.on_db_gen_save(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("sql", &["sql"]).set_file_name("schema.sql").save_file()
            else {
                return;
            };
            let _ = std::fs::write(&path, app.get_db_gen_content().to_string());
        });
    }
    // (re)construir o grafo do schema atual (tabela=nÃ³, FK=aresta) e ligar a fÃ­sica.
    {
        let weak = app.as_weak();
        let sh = db_schema.clone();
        let gt = db_graph_timer.clone();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_show_graph(move || {
            let (nodes, edges, descs) = {
                let s = sh.lock().unwrap();
                let (n, e) = database::to_graph(&s);
                (n, e, database::table_descriptions(&s))
            };
            load_db_graph_into(&mut gs.borrow_mut(), nodes, edges, descs);
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &gs.borrow(), &gn, &ge);
            }
            db_graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
        });
    }
    // gerar por descriÃ§Ã£o (IA): dispara a skill schematize-database num TERMINAL
    // EXTERNO (processo prÃ³prio do claude, fora do app). Usa o projeto atual (od_current).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.on_db_ai_generate(move || {
            let Some(app) = weak.upgrade() else { return };
            let desc = app.get_db_ai_desc().to_string();
            if desc.trim().is_empty() {
                return;
            }
            let Some(root) = cur.borrow().clone() else {
                app.set_db_ai_status(tor("gui.db_ai_no_project", "Selecione um projeto na tela Overdev/Grafo primeiro.").into());
                return;
            };
            let base = basename_of(&root);
            let prompt = db_ai_prompt(&base, &desc);
            let w = weak.clone();
            std::thread::spawn(move || {
                let res = agentrun::launch_prompt_in_terminal(&root, &prompt);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = w.upgrade() {
                        let msg = match res {
                            Ok(term) => format!(
                                "{}{}{}",
                                tor("gui.db_ai_running_pre", "schematize-database rodando no terminal "),
                                term,
                                tor("gui.db_ai_running_post", " â carregue o schema.json quando terminar."),
                            ),
                            Err(e) => e,
                        };
                        app.set_db_ai_status(msg.into());
                    }
                });
            });
        });
    }
    // ---- grafo do schema: interaÃ§Ã£o (pan/zoom/arrasto/clique) â estado dedicado ----
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_canvas_resized(move |w, h| {
            let mut st = gs.borrow_mut();
            st.canvas_w = w;
            st.canvas_h = h;
            if st.fit_pending && w > 1.0 && h > 1.0 {
                st.fit();
                st.fit_pending = false;
            }
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let gs = db_graph_state.clone();
        app.on_db_graph_press(move |mx, my| {
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
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        let gt = db_graph_timer.clone();
        app.on_db_graph_move(move |mx, my| {
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
                    db_graph_sync(&app, &st, &gn, &ge);
                }
            }
            if need_kick {
                db_graph_kick(&gt, weak.clone(), gs.clone(), gn.clone(), ge.clone());
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_release(move || {
            let mut st = gs.borrow_mut();
            if !st.moved {
                let (wx, wy) = st.to_world(st.last_ptr.0, st.last_ptr.1);
                st.sel = st.hit(wx, wy);
                st.refresh_flags();
            }
            st.drag_node = None;
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_scroll(move |mx, my, dy| {
            let mut st = gs.borrow_mut();
            let cx = st.canvas_w / 2.0;
            let cy = st.canvas_h / 2.0;
            let m = (mx - cx - st.ox, my - cy - st.oy);
            let f = (dy * 0.0015).exp();
            st.ox -= m.0 * (f - 1.0);
            st.oy -= m.1 * (f - 1.0);
            st.scale = (st.scale * f).clamp(0.12, 6.0);
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_fit(move || {
            let mut st = gs.borrow_mut();
            st.fit();
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }
    {
        let weak = app.as_weak();
        let gs = db_graph_state.clone();
        let gn = db_graph_nodes.clone();
        let ge = db_graph_edges.clone();
        app.on_db_graph_clear_sel(move || {
            let mut st = gs.borrow_mut();
            st.sel = None;
            st.refresh_flags();
            if let Some(app) = weak.upgrade() {
                db_graph_sync(&app, &st, &gn, &ge);
            }
        });
    }

    // ==================== PaginaÃ§Ã£o do Mercado ====================
    // Recomputa os Ã­ndices de exibiÃ§Ã£o (disp) quando a sub-aba muda.
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
    // Roda em THREAD (bw/subprocesso pode bloquear); sÃ³ o resultado (String, Send)
    // volta pela event loop. A chave PRIVADA nunca chega Ã  UI (o lib a esconde).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_export_bw(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let i = idx as usize;
            let Some(mut r) = m.row_data(i) else { return };
            let name = r.name.to_string();
            // marca a linha como ocupada e limpa o banner anterior.
            r.op_label = tor("gui.ssh_bw_exporting", "exportandoâ¦").into();
            r.op_error = false;
            m.set_row_data(i, r);
            app.set_ssh_bw_result(SharedString::new());
            let weak2 = app.as_weak();
            std::thread::spawn(move || {
                let res = sshkeys::export_bitwarden(&name, None);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak2.upgrade() {
                        // solta o "ocupado" da linha (o modelo Ã© o mesmo VecModel).
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
                    app.set_ssh_gen_status(format!("{} Â· {}", info.name, info.fingerprint).into());
                    // PROVA da chave: bits Â· fingerprint Â· tipo (ssh-keygen -l). Confere a forÃ§a.
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
    // copiar a PÃBLICA (export_public + clipboard). NUNCA toca a privada.
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
    // pedir remoÃ§Ã£o â abre o modal de confirmaÃ§Ã£o.
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
                        tor("gui.ssh_remove_note", "Isto apaga o par (privada + pÃºblica).")
                    )
                    .into(),
                );
                app.set_ssh_confirm_open(true);
            }
        });
    }
    // confirmar remoÃ§Ã£o (remove o par).
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

    // ==================== Tela ConfiguraÃ§Ãµes ====================
    let cur_lang = i18n::current_code();
    let cfg_lang_model = Rc::new(VecModel::<LangItem>::from(build_lang_items(&cur_lang)));
    app.set_cfg_langs(ModelRc::from(cfg_lang_model.clone()));
    app.set_cfg_lang_code(cur_lang.clone().into());
    app.set_cfg_lang_name(i18n::name_of(&cur_lang).unwrap_or("").into());
    app.set_cfg_autostart_on(autostart::is_active());
    app.set_cfg_hooks_on(settings::overdev_enabled());
    // trocar idioma AO VIVO: persiste + recarrega TODOS os rÃ³tulos estÃ¡ticos (L).
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
    // autostart do agente (systemd --user + XDG). exe = binÃ¡rio do CLI schematize.
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
    // atalho: reusa o modal de diretÃ³rios de dev / projetos fixados.
    {
        let weak = app.as_weak();
        app.on_cfg_manage_dirs(move || {
            if let Some(app) = weak.upgrade() {
                app.set_dev_modal_open(true);
            }
        });
    }
    // DiagnÃ³stico: alterna o diagnÃ³stico de rede (online) â mais lento quando ligado.
    {
        let weak = app.as_weak();
        app.on_cfg_debug_toggle_online(move || {
            if let Some(app) = weak.upgrade() {
                app.set_cfg_debug_online(!app.get_cfg_debug_online());
            }
        });
    }
    // DiagnÃ³stico: gera o relatÃ³rio de debug numa THREAD (nÃ£o trava a UI). SÃ³ dados
    // Send cruzam a fronteira â o caminho volta como String via invoke_from_event_loop.
    // Offline por default (rÃ¡pido); com o toggle marcado passa online=true (mais lento).
    {
        let weak = app.as_weak();
        app.on_cfg_debug_generate(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_cfg_debug_running() {
                return;
            }
            let online = app.get_cfg_debug_online();
            // marca em andamento + limpa o resultado anterior.
            app.set_cfg_debug_running(true);
            app.set_cfg_debug_path(SharedString::new());
            app.set_cfg_debug_summary(SharedString::new());
            app.set_cfg_debug_error(SharedString::new());
            let weak = app.as_weak();
            std::thread::spawn(move || {
                let res = debugreport::write_report(None, online); // Result<PathBuf,String> (Send)
                let summary = debugreport::short_summary(); // String (Send)
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_cfg_debug_running(false);
                        match res {
                            Ok(path) => {
                                app.set_cfg_debug_path(path.to_string_lossy().into_owned().into());
                                app.set_cfg_debug_summary(summary.into());
                                app.set_cfg_debug_error(SharedString::new());
                            }
                            Err(e) => {
                                app.set_cfg_debug_error(tf("err.prefix", &[("e", &e)]).into());
                            }
                        }
                    }
                });
            });
        });
    }
    // DiagnÃ³stico: abre a PASTA do relatÃ³rio no gerenciador de arquivos (reusa o
    // mesmo mecanismo do "Abrir pasta" da barra de projeto: open_path_in_files).
    {
        let weak = app.as_weak();
        app.on_cfg_debug_open_folder(move || {
            let Some(app) = weak.upgrade() else { return };
            let path = app.get_cfg_debug_path().to_string();
            if path.is_empty() {
                return;
            }
            let p = Path::new(&path);
            let dir = p.parent().unwrap_or(p);
            open_path_in_files(dir);
        });
    }

    // ==================== Overdev â histÃ³rico DB + commits ====================
    // recarrega o histÃ³rico do projeto atual (chamado ao entrar na tela / reload).
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
    // paginaÃ§Ã£o do histÃ³rico do DB.
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
    // Ver: conteÃºdo do snapshot num visor read-only.
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
    // Restaurar: pede confirmaÃ§Ã£o.
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
                // recarrega overdev (checklist) + histÃ³rico refletindo o disco.
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
    // paginaÃ§Ã£o dos commits.
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

    // ==================== VersÃ£o do app + self-update ====================
    // "Verificar atualizaÃ§Ã£o" â app_update_available() em thread; se hÃ¡ versÃ£o nova,
    // acende o botÃ£o "Atualizar app" que roda selfupdate::run() em thread e, ao
    // concluir, sugere reiniciar (o restart jÃ¡ existe: relanÃ§a a janela nova).
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
                                    format!("{} v{new}", tor("gui.app_new_version", "Nova versÃ£o disponÃ­vel:")).into(),
                                );
                            }
                            None => {
                                app.set_app_has_update(false);
                                app.set_app_update_status(tor("gui.app_up_to_date", "VocÃª estÃ¡ atualizado").into());
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
            app.set_app_update_status(tor("gui.app_updating", "Atualizandoâ¦").into());
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

    // Gestor de atualizações (schematize-updater): checa na ABERTURA se está instalado; se faltar,
    // a UI mostra o prompt "instalar". Cobre instalação limpa E update — sem o updater, o update
    // central não roda. O botão baixa o binário do updater (ensure_updater) numa thread.
    {
        let weak = app.as_weak();
        app.on_install_updater(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_updater_installing() {
                return;
            }
            app.set_updater_installing(true);
            app.set_updater_status(SharedString::new());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = selfupdate::ensure_updater();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_updater_installing(false);
                        match res {
                            Ok(_p) => {
                                app.set_updater_missing(false);
                                app.set_updater_status(
                                    tor("gui.updater_installed", "Gestor de atualizações instalado.").into(),
                                );
                            }
                            Err(e) => app.set_updater_status(tf("err.prefix", &[("e", &e)]).into()),
                        }
                    }
                });
            });
        });
    }
    // Estado inicial do prompt: o updater está presente?
    app.set_updater_missing(selfupdate::updater_bin().is_none());
    // Startup: checa update do app em background pra a bolinha de update do header (versão) acender
    // sozinha, sem o usuário precisar clicar "Verificar atualização".
    {
        let weak = app.as_weak();
        std::thread::spawn(move || {
            let has = upgrade::app_update_available().is_some();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    app.set_app_has_update(has);
                }
            });
        });
    }

    // ==================== Sininho de notificaÃ§Ãµes ====================
    // Os modelos (Global/Pessoal) sÃ£o REMONTADOS no event loop a cada abertura (nÃ£o
    // cruzam a fronteira da thread â padrÃ£o threadâUI do resto da GUI). A aÃ§Ã£o de
    // cada item viaja pelo prÃ³prio callback (kind, action), sem estado Rust extra.
    app.set_notif_global(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));
    app.set_notif_personal(ModelRc::from(Rc::new(VecModel::<NotifItem>::from(Vec::new()))));

    // recompute sÃ³ a contagem (badge) â barato de disparar, roda em thread.
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
    // executar a aÃ§Ã£o de uma notificaÃ§Ã£o â (kind, action) vÃªm do prÃ³prio item.
    {
        let weak = app.as_weak();
        app.on_notif_action(move |kind, action| {
            let Some(app) = weak.upgrade() else { return };
            match kind.as_str() {
                // nova versÃ£o do app â fecha o painel, vai pra ConfiguraÃ§Ãµes e dispara o update.
                "app_update" => {
                    app.set_notif_open(false);
                    app.set_screen(5);
                    app.set_app_has_update(true);
                    app.invoke_app_do_update();
                }
                // post do blog â abre a URL no navegador.
                "news" => {
                    let url = action.to_string();
                    if !url.is_empty() {
                        util::open_url(&url);
                    }
                }
                // skill desatualizada â leva pra aba Instaladas do Mercado.
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
    // contagem inicial + refresh periÃ³dico (a cada 90s) do badge, em thread.
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
    // "Comparar com oficial" â compare_update(slug) em thread; abre o painel com
    // baseânova, arquivos (status) e o diff. NÃO sobrescreve nada.
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
                                app.set_cmp_versions(format!("v{base} â v{new}").into());
                                app.set_cmp_diff(if diff.trim().is_empty() {
                                    tor("gui.compare_identical", "(sem diferenÃ§as de conteÃºdo)").into()
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
    // Estado da sessÃ£o + fluxo de login OAuth device flow. `device_start` e o loop
    // de `device_poll_once` sÃ£o REDE â rodam numa thread (nunca bloqueiam o event
    // loop); a UI Ã© tocada sÃ³ via `invoke_from_event_loop`. O loop Ã© CANCELÃVEL: o
    // flag corrente vive num `Rc<RefCell<Arc<AtomicBool>>>` (padrÃ£o do worker do
    // overdev). Cada login troca por um flag NOVO e levanta o antigo, encerrando
    // qualquer thread remanescente; Cancelar/Sair levantam o flag corrente.
    // SÃ³ dados `Send` (String/PathBuf/Arc) cruzam a fronteira.
    let acc_stop: Rc<RefCell<Arc<AtomicBool>>> = Rc::new(RefCell::new(Arc::new(AtomicBool::new(false))));
    // Estado inicial: reflete a sessÃ£o persistida em disco.
    app.set_acc_logged_in(account::is_logged_in());
    app.set_acc_sub(account::account_sub().unwrap_or_default().into());

    // iniciar o device flow.
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.on_acc_login(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_acc_polling() {
                return; // jÃ¡ hÃ¡ um login em andamento
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
                        // Mostra o cÃ³digo + a URL e abre o modal.
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
                        // Loop de poll (respeita interval/expires_in; cancelÃ¡vel via `stop`).
                        let start = Instant::now();
                        let mut interval = dl.interval.max(1);
                        loop {
                            if stop.load(Ordering::SeqCst) {
                                return; // cancelado/substituÃ­do â a UI jÃ¡ foi tratada
                            }
                            if start.elapsed().as_secs() >= dl.expires_in {
                                let weak = weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        app.set_acc_modal_open(false);
                                        app.set_acc_polling(false);
                                        app.set_acc_status(
                                            tor("gui.acc_expired", "O cÃ³digo expirou. Tente novamente.").into(),
                                        );
                                    }
                                });
                                return;
                            }
                            // dorme `interval` em passos de 1s pra reagir rÃ¡pido ao cancelamento.
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
                                                tor("gui.acc_expired", "O cÃ³digo expirou. Tente novamente.").into(),
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
                                                    // recomputa o badge do sino (notificaÃ§Ãµes do
                                                    // servidor aparecem quando logado).
                                                    app.invoke_notif_refresh();
                                                }
                                                Some(e) => app.set_acc_status(
                                                    format!("{} {e}", tor("gui.acc_save_error", "Falha ao salvar a sessÃ£o:")).into(),
                                                ),
                                            }
                                        }
                                    });
                                    return;
                                }
                                // erro de rede transitÃ³rio: mantÃ©m o poll (nÃ£o derruba o fluxo).
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

    // sair (logout): encerra a sessÃ£o + atualiza a UI + recomputa o badge do sino.
    {
        let weak = app.as_weak();
        let acc_stop = acc_stop.clone();
        app.on_acc_logout(move || {
            // por seguranÃ§a, para qualquer poll em andamento.
            acc_stop.borrow().store(true, Ordering::SeqCst);
            account::logout();
            if let Some(app) = weak.upgrade() {
                app.set_acc_logged_in(false);
                app.set_acc_sub(SharedString::new());
                app.set_acc_polling(false);
                app.set_acc_modal_open(false);
                app.set_acc_status(SharedString::new());
                // notificaÃ§Ãµes do servidor somem quando deslogado â recomputa o badge.
                app.invoke_notif_refresh();
            }
        });
    }

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cria um `.overdev/CHECKLIST.md` temporÃ¡rio e ÃNICO (testes rodam em paralelo).
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
- [ ] item mÃ¡quina aberto A
- [x] item mÃ¡quina feito B
- [~] item on-hold C
- [H ] item humano aberto D
- [H x] item humano feito E
- [H ] item humano aberto F
nÃ£o Ã© item
  - [ ] item indentado aberto G
";

    #[test]
    fn parse_2niveis_classifica_e_indexa_humanos() {
        let root = scratch(FIX);
        let its = parse_checklist_items(&root);
        // 7 itens de checklist (a linha "nÃ£o Ã© item" Ã© ignorada).
        assert_eq!(its.len(), 7);
        let by_kind = |k: &str| its.iter().filter(|i| i.kind == k).count();
        assert_eq!(by_kind("open"), 2, "mÃ¡quina abertos (inclui indentado)");
        assert_eq!(by_kind("done"), 1, "mÃ¡quina feito");
        assert_eq!(by_kind("hold"), 1, "on-hold");
        assert_eq!(by_kind("hopen"), 2, "humanos abertos");
        assert_eq!(by_kind("hdone"), 1, "humano feito");
        // hindex numera sÃ³ os humanos abertos, 1-based, na ordem do arquivo.
        let hopen: Vec<i32> = its.iter().filter(|i| i.kind == "hopen").map(|i| i.hindex).collect();
        assert_eq!(hopen, vec![1, 2]);
        // itens de mÃ¡quina nÃ£o tÃªm origem-humano nem Ã­ndice.
        let mo = its.iter().find(|i| i.kind == "open").unwrap();
        assert!(mo.machine && mo.hindex == -1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn marca_humano_por_indice_so_o_que_casa() {
        let root = scratch(FIX);
        // fecha o 2Âº humano aberto (F) â vira - [H x]; D segue aberto.
        mark_human_done_at(&root, 2).unwrap();
        let its = parse_checklist_items(&root);
        assert_eq!(its.iter().filter(|i| i.kind == "hopen").count(), 1, "sobra 1 humano aberto");
        assert!(its.iter().any(|i| i.kind == "hopen" && i.text.contains("aberto D")));
        assert!(its.iter().any(|i| i.kind == "hdone" && i.text.contains("aberto F")));
        // nÃ£o toca itens de mÃ¡quina.
        assert_eq!(its.iter().filter(|i| i.kind == "open").count(), 2);
        // Ã­ndice fora de faixa â erro, arquivo intacto.
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
