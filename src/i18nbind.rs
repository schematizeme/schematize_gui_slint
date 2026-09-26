//! Ponte i18n → propriedades do `global L` do Slint.
//! O quê: lê o catálogo do lib (`schematize::i18n`) e escreve TODA string estática
//! da UI. Onde: chamado uma vez no arranque e a cada troca de idioma.

use crate::prelude::*;

/// Traduz uma chave; se ela AINDA não existe no lib (o `t()` do lib devolve a
/// própria chave quando não acha), cai no `fallback` embutido. Usado só para as
/// chaves NOVAS desta fase (Home/navegação) — assim a UI já mostra texto decente
/// e, quando o lib ganhar essas chaves, passa a usar a tradução automaticamente.
/// As chaves novas estão listadas no relatório de entrega.
pub(crate) fn tor(key: &str, fallback: &str) -> String {
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
pub(crate) fn install_i18n(app: &AppWindow) {
    let l = app.global::<L>();
    l.set_window_title("schematize".into());
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
    l.set_mercado_title(t("gui.mercado_title").into());
    l.set_mercado_body(t("gui.mercado_body").into());
    l.set_mercado_open(t("gui.mercado_open").into());
    l.set_mercado_missing(t("gui.mercado_missing").into());
    l.set_mercado_install(t("gui.mercado_install").into());
    l.set_deployer_title(t("gui.deployer_title").into());
    l.set_deployer_body(t("gui.deployer_body").into());
    l.set_deployer_open_keys(t("gui.deployer_open_keys").into());
    l.set_deployer_open_hosts(t("gui.deployer_open_hosts").into());
    l.set_deployer_missing(t("gui.deployer_missing").into());
    l.set_deployer_install(t("gui.deployer_install").into());
    l.set_database_title(t("gui.database_title").into());
    l.set_database_body(t("gui.database_body").into());
    l.set_database_open(t("gui.database_open").into());
    l.set_database_missing(t("gui.database_missing").into());
    l.set_database_install(t("gui.database_install").into());
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
    l.set_open_folder(t("gui.open_folder").into());
    l.set_reload(t("gui.reload").into());
    l.set_dev_dirs(t("gui.dev_dirs").into());
    l.set_add_dev_dir(t("gui.add_dev_dir").into());
    l.set_dev_dirs_empty(t("gui.dev_dirs_empty").into());
    // Projetos fixados (pins) — chaves NOVAS com fallback embutido via `tor`.
    l.set_pinned_projects(tor("gui.pinned_projects", "Projetos fixados").into());
    l.set_pin_folder(tor("gui.pin_folder", "Fixar pasta…").into());
    l.set_pin_hint(
        tor(
            "gui.pin_hint",
            "Uma pasta fixada vira UM projeto no selector — útil pra workspace de microserviços.",
        )
        .into(),
    );
    l.set_no_overdev(t("gui.no_overdev").into());
    l.set_od_decisions(t("gui.od_decisions").into());
    l.set_od_plan(t("gui.od_plan").into());
    l.set_od_questions(t("gui.od_questions").into());
    l.set_open_browser(t("gui.open_browser").into());
    l.set_od_open_terminal(tor("gui.od_open_terminal", "abrir no terminal").into());
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
    // checklist FATIADO: filtros + rodapé de paginação (a UI nunca recebe a lista toda)
    l.set_od_cl_all(tor("gui.od_cl_all", "Todos").into());
    l.set_od_cl_open(tor("gui.od_cl_open", "Abertos").into());
    l.set_od_cl_done(tor("gui.od_cl_done", "Feitos").into());
    l.set_od_cl_hold(tor("gui.od_cl_hold", "On-hold").into());
    l.set_od_cl_human(tor("gui.od_cl_human", "Humanos").into());
    l.set_od_cl_of(tor("gui.od_cl_of", "de").into());
    l.set_od_cl_empty(tor("gui.od_cl_empty", "Nenhum item neste filtro.").into());
    // editor acoplado sob demanda (o TextEdit só existe enquanto aberto)
    l.set_od_editor_show(tor("gui.od_editor_show", "Abrir editor").into());
    l.set_od_editor_hide(tor("gui.od_editor_hide", "Fechar editor").into());
    l.set_od_editor_big(tor("gui.od_editor_big", "Arquivo grande demais para editar aqui").into());
    l.set_od_editor_external(tor("gui.od_editor_external", "Abrir no editor externo").into());
    l.set_od_notes_title(tor("gui.od_notes", "Notas e correções").into());
    // aba Overdev — Fase 4 (terminal externo + monitor leve). Chaves NOVAS via `tor`.
    l.set_od_run(tor("gui.od_run", "Executar overdev").into());
    l.set_od_gen_archive(tor("gui.od_gen_archive", "Gerar afazeres do archive").into());
    l.set_od_stop(tor("gui.od_stop", "Parar").into());
    l.set_od_running(tor("gui.od_running", "monitorando…").into());
    l.set_od_mon_active(tor("gui.od_mon_active", "rodando").into());
    l.set_od_confirm_run(tor(
        "gui.od_confirm_run",
        "Isto abre o `claude` num TERMINAL EXTERNO (processo próprio, fora do app) e roda o overdev \
         neste projeto com acesso ao seu ambiente — ele pode editar arquivos. O app apenas MONITORA o \
         progresso. Confira o comando abaixo antes de confirmar.",
    ).into());
    l.set_od_agent_cmd(tor("gui.od_agent_cmd", "Comando do agente").into());
    l.set_od_ext_terminal(
        tor(
            "gui.od_ext_terminal",
            "claude rodando em terminal externo (processo próprio) — o load dele fica fora do app.",
        )
        .into(),
    );
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
    // Botões/estados NOVOS da aba Grafo (reindexar + recarregar + nó sem descrição).
    // Chaves novas com fallback pt-BR embutido via `tor` até entrarem no lib.
    l.set_g_reindex(tor("gui.g_reindex", "Reindexar").into());
    l.set_g_reload(tor("gui.g_reload", "Recarregar").into());
    l.set_g_drill(tor("gui.g_drill", "Grafo do serviço").into());
    l.set_g_global(tor("gui.g_global", "← Grafo global").into());
    l.set_g_node_nodesc(
        tor("gui.g_node_nodesc", "(sem descrição no índice — rode Reindexar)").into(),
    );
    // Home + navegação (Fase 1) — chaves NOVAS, com fallback embutido via `tor`
    // até serem adicionadas ao lib. Ver lista no relatório de entrega.
    l.set_home_title(tor("gui.home_title", "O que você quer fazer?").into());
    l.set_home_market(tor("gui.home_market", "Mercado de Skills").into());
    l.set_home_overdev_desc(
        tor("gui.home_overdev_desc", "Acompanhe o desenvolvimento contínuo do projeto.").into(),
    );
    l.set_home_market_desc(
        tor("gui.home_market_desc", "Instale, atualize e descubra skills e environments.").into(),
    );
    l.set_home_graph_desc(
        tor("gui.home_graph_desc", "Explore o grafo de microfunções do projeto.").into(),
    );
    l.set_home_environments(tor("gui.home_environments", "Environments").into());
    l.set_home_environments_desc(
        tor("gui.home_environments_desc", "Gerencie os runtimes de linguagem.").into(),
    );
    l.set_home_ssh(tor("gui.home_ssh", "SSH").into());
    l.set_home_ssh_desc(tor("gui.home_ssh_desc", "Chaves e acesso remoto.").into());
    l.set_home_settings(tor("gui.home_settings", "Configurações").into());
    l.set_home_settings_desc(tor("gui.home_settings_desc", "Idioma, tema e preferências.").into());
    l.set_open_vscode(tor("gui.open_vscode", "Abrir no VSCode").into());
    l.set_open_loose_project(tor("gui.open_loose_project", "Abrir projeto avulso…").into());
    // aba Gerenciar (criar + editar skills) — chaves NOVAS via `tor`. Ver relatório.
    l.set_manage(tor("gui.manage", "Gerenciar").into());
    // Tela SSH — chaves NOVAS via `tor`.
    // SSH — entropia (do lib, por tipo) + prova + Bitwarden. Chaves NOVAS via `tor`.
    // Tela Configurações — chaves NOVAS via `tor`.
    l.set_cfg_title(tor("gui.cfg_title", "Configurações").into());
    l.set_cfg_language(tor("gui.cfg_language", "Idioma").into());
    l.set_cfg_theme(tor("gui.cfg_theme", "Tema").into());
    l.set_cfg_autostart(tor("gui.cfg_autostart", "Autostart do agente").into());
    l.set_cfg_autostart_desc(
        tor("gui.cfg_autostart_desc", "Inicia o agente de atualização junto com a sua sessão.")
            .into(),
    );
    l.set_cfg_hooks(tor("gui.cfg_hooks", "Hooks do overdev").into());
    l.set_cfg_hooks_desc(
        tor("gui.cfg_hooks_desc", "Registra os hooks (Stop/PreToolUse) do overdev no Claude Code.")
            .into(),
    );
    l.set_cfg_dirs(tor("gui.cfg_dirs", "Diretórios de dev e projetos fixados").into());
    l.set_cfg_dirs_desc(
        tor("gui.cfg_dirs_desc", "Onde o schematize procura os seus projetos.").into(),
    );
    l.set_cfg_manage(tor("gui.cfg_manage", "Gerenciar…").into());
    // Diagnóstico (relatório de debug) — chaves NOVAS via `tor`.
    l.set_cfg_debug_title(tor("gui.cfg_debug_title", "Diagnóstico").into());
    l.set_cfg_debug_btn(tor("gui.cfg_debug_btn", "Gerar relatório de debug").into());
    l.set_cfg_debug_generating(tor("gui.cfg_debug_generating", "Gerando…").into());
    l.set_cfg_debug_open(tor("gui.cfg_debug_open", "Abrir pasta").into());
    l.set_cfg_debug_note(
        tor("gui.cfg_debug_note", "modo 600 · segredos redigidos · revise antes de compartilhar")
            .into(),
    );
    l.set_cfg_debug_net(
        tor("gui.cfg_debug_net", "incluir diagnóstico de rede (mais lento)").into(),
    );
    l.set_cfg_debug_saved(tor("gui.cfg_debug_saved", "Relatório gravado em").into());
    // Overdev — aditivos.
    l.set_od_history(tor("gui.od_history", "Histórico (cópia de segurança)").into());
    l.set_od_history_note(tor("gui.od_history_note", "O agente pode editar/apagar os arquivos do .overdev/ — este é o backup versionado deles.").into());
    l.set_od_view(tor("gui.od_view", "Ver").into());
    l.set_od_restore(tor("gui.od_restore", "Restaurar").into());
    l.set_od_snap_empty(tor("gui.od_snap_empty", "Sem snapshots ainda.").into());
    l.set_od_commits(tor("gui.od_commits", "Commits e push").into());
    l.set_od_commits_empty(
        tor("gui.od_commits_empty", "Sem commits (ou não é um repositório git).").into(),
    );
    l.set_od_close(tor("gui.od_close", "Fechar").into());
    // Paginação.
    l.set_pg_prev(tor("gui.pg_prev", "‹ Anterior").into());
    l.set_pg_next(tor("gui.pg_next", "Próximo ›").into());
    l.set_pg_of(tor("gui.pg_of", "de").into());
    // Versão do app + self-update (Configurações). Chaves NOVAS via `tor`.
    l.set_app_version_title(tor("gui.app_version_title", "Versão do app").into());
    l.set_app_check_update(tor("gui.app_check_update", "Verificar atualização").into());
    l.set_app_checking(tor("gui.app_checking", "Verificando…").into());
    l.set_app_update_btn(tor("gui.app_update_btn", "Atualizar app").into());
    l.set_app_updating(tor("gui.app_updating", "Atualizando…").into());
    l.set_app_restart_hint(
        tor("gui.app_restart_hint", "Atualização concluída — reinicie o app.").into(),
    );
    l.set_app_restart(tor("gui.app_restart", "Reiniciar").into());
    l.set_gestor_missing_msg(tor("gui.gestor_missing", "O gestor de atualizações (schematize-market) não está instalado — ele cuida de instalar/atualizar o app.").into());
    l.set_gestor_install_btn(tor("gui.gestor_install", "Instalar gestor de atualizações").into());
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
    l.set_acc_logged_out_hint(
        tor(
            "gui.acc_logged_out_hint",
            "Entre na plataforma para receber notificações e sincronizar suas skills.",
        )
        .into(),
    );
    l.set_acc_modal_title(tor("gui.acc_modal_title", "Entrar na plataforma").into());
    l.set_acc_code_label(
        tor("gui.acc_code_label", "Abra o endereço abaixo no navegador e digite este código:")
            .into(),
    );
    l.set_acc_open_browser(tor("gui.acc_open_browser", "Abrir no navegador").into());
    l.set_acc_verification_at(tor("gui.acc_verification_at", "Acesse:").into());
    l.set_acc_waiting(tor("gui.acc_waiting", "Aguardando confirmação…").into());
    l.set_acc_cancel(tor("gui.acc_cancel", "Cancelar").into());
    l.set_acc_indicator_tip(tor("gui.acc_indicator_tip", "Conectado — abrir Conta").into());
    // Database builder (tela 6) — chaves NOVAS via `tor`.
    l.set_home_database(tor("gui.home_database", "Banco de dados").into());
    l.set_home_database_desc(
        tor("gui.home_database_desc", "Leia, modele e gere o schema do seu banco.").into(),
    );
    // Os 40 rótulos do Database builder saíram daqui com a tela (E1 M5, ADR-0018).
    //
    // Eram `db_sub_connect`, `db_add_column`, `db_fk_refcol`, `db_gen_migration`… — o
    // vocabulário inteiro de uma tela de modelagem, num hub que não modela mais nada. Quem
    // desenha schema agora é a janela do `schematize-database`, e os rótulos dela moram lá.

    // ---- Disco (tela 7) ----
    l.set_home_disk(tor("gui.home_disk", "Disco").into());
    l.set_home_disk_desc(
        tor("gui.home_disk_desc", "O que está enchendo o disco e pode ser refeito.").into(),
    );
    l.set_disk_min_days(tor("gui.disk_min_days", "Parado há").into());
    l.set_disk_any(tor("gui.disk_any", "qualquer").into());
    l.set_disk_scan(tor("gui.disk_scan", "Varrer").into());
    l.set_disk_scanning(tor("gui.disk_scanning", "varrendo…").into());
    l.set_disk_intro(tor(
        "gui.disk_intro",
        "Mede o lixo RECRIÁVEL dos seus diretórios de dev — artefato de build, cache de toolchain e camada de Docker — e agrupa por disco. Nada é medido nem apagado sem você pedir: comece por Varrer.",
    ).into());
    l.set_disk_by_mount(tor("gui.disk_by_mount", "POR DISCO").into());
    l.set_disk_by_kind(tor("gui.disk_by_kind", "POR TIPO").into());
    l.set_disk_docker(tor("gui.disk_docker", "DOCKER").into());
    l.set_disk_largest(tor("gui.disk_largest", "MAIORES").into());
    l.set_disk_reclaimable(tor("gui.disk_reclaimable", "recuperável:").into());
    l.set_disk_prune(tor("gui.disk_prune", "Podar").into());
    l.set_disk_prune_data(tor("gui.disk_prune_data", "Apagar volumes").into());
    l.set_disk_open(tor("gui.disk_open", "Abrir").into());
    l.set_disk_delete(tor("gui.disk_delete", "Apagar").into());
    l.set_disk_data_warning(tor(
        "gui.disk_data_warning",
        "Volume do Docker é DADO, não build: banco de dev, upload de teste. Isto não se refaz compilando.",
    ).into());

    // ---- Git (tela 8) ----
    l.set_home_git(tor("gui.home_git", "Git").into());
    l.set_home_git_desc(
        tor("gui.home_git_desc", "Contas, o que ainda não saiu daqui, e seus repositórios.").into(),
    );
    l.set_git_projects(tor("gui.git_projects", "Projetos").into());
    l.set_git_accounts(tor("gui.git_accounts", "Contas").into());
    l.set_git_repos(tor("gui.git_repos", "Repositórios").into());
    l.set_git_scanning(tor("gui.git_scanning", "lendo os repositórios…").into());
    l.set_git_rescan(tor("gui.git_rescan", "Reler").into());
    l.set_git_intro(tor(
        "gui.git_intro",
        "Git não guarda histórico de push — mas guarda o que ainda NÃO foi enviado. É isso que esta lista mostra: commit que só existe nesta máquina some com a máquina.",
    ).into());
    l.set_git_at_risk(
        tor("gui.git_at_risk", "projeto(s) com commit que só existe nesta máquina.").into(),
    );
    l.set_git_no_remote(tor("gui.git_no_remote", "sem remoto").into());
    l.set_git_dirty(tor("gui.git_dirty", "sujo").into());
    l.set_git_open(tor("gui.git_open", "Abrir").into());
    l.set_git_use_account(tor("gui.git_use_account", "Usar a conta neste repositório").into());
    l.set_git_apply(tor("gui.git_apply", "Aplicar").into());
    l.set_git_no_accounts(
        tor("gui.git_no_accounts", "Nenhuma conta cadastrada — cadastre em Contas.").into(),
    );
    l.set_git_history(tor("gui.git_history", "Histórico").into());
    l.set_git_accounts_intro(tor(
        "gui.git_accounts_intro",
        "Quem trabalha com mais de uma conta empurra commit com a identidade errada — e no GitHub isso é público. Cadastre cada conta uma vez e aplique-a por repositório.",
    ).into());
    l.set_git_write_alias(tor("gui.git_write_alias", "Escrever alias SSH").into());
    l.set_git_alias_ok(tor("gui.git_alias_ok", "alias ok").into());
    l.set_git_remove(tor("gui.git_remove", "Remover").into());
    l.set_git_add_account(tor("gui.git_add_account", "CADASTRAR CONTA").into());
    l.set_git_f_label(tor("gui.git_f_label", "rótulo (pessoal)").into());
    l.set_git_f_user(tor("gui.git_f_user", "usuário no serviço").into());
    l.set_git_f_email(tor("gui.git_f_email", "e-mail do commit").into());
    l.set_git_f_service(tor("gui.git_f_service", "serviço").into());
    l.set_git_f_key(tor("gui.git_f_key", "chave em ~/.ssh (vazio = gh)").into());
    l.set_git_save(tor("gui.git_save", "Salvar").into());
    l.set_git_no_secrets(tor(
        "gui.git_no_secrets",
        "Guardamos só o NOME do arquivo de chave e o host — nunca a chave nem token. Autenticar segue com o gh / agente SSH.",
    ).into());
    l.set_git_private(tor("gui.git_private", "privado").into());
    l.set_git_public(tor("gui.git_public", "público").into());
    l.set_git_loading_repos(tor("gui.git_loading_repos", "consultando o gh…").into());

    // ---- Resolver item humano · caixa de entrada · skills do projeto ----
    l.set_od_answer(tor("gui.od_answer", "Responder").into());
    l.set_od_refuse(tor("gui.od_refuse", "Recusar").into());
    l.set_od_answer_title(tor("gui.od_answer_title", "Responder").into());
    l.set_od_refuse_title(tor("gui.od_refuse_title", "Recusar").into());
    l.set_od_answer_hint(tor(
        "gui.od_answer_hint",
        "Sua resposta LIBERA o item de máquina que estava travado por isto. Você não precisa ter feito nada — decidir já basta.",
    ).into());
    l.set_od_refuse_hint(tor(
        "gui.od_refuse_hint",
        "Recusar CANCELA o item de máquina vinculado — o agente não vai retomá-lo. Diga por quê: fica registrado nas decisões do projeto.",
    ).into());
    l.set_od_caixa_title(tor("gui.od_caixa_title", "Acrescentar ao projeto").into());
    l.set_od_caixa_hint(tor(
        "gui.od_caixa_hint",
        "Escreva o que mais precisa ser feito. Isto NÃO toca o checklist agora — nenhum agente é interrompido. Um agente organiza depois, e só então entra.",
    ).into());
    l.set_od_caixa_placeholder(
        tor("gui.od_caixa_placeholder", "ex.: precisa exportar em CSV e mandar por e-mail…").into(),
    );
    l.set_od_caixa_add(tor("gui.od_caixa_add", "Capturar").into());
    l.set_od_caixa_agent(tor("gui.od_caixa_agent", "Organizar com agente").into());
    l.set_od_caixa_merge(tor("gui.od_caixa_merge", "Fundir no checklist").into());
    l.set_od_caixa_pending(tor("gui.od_caixa_pending", "a organizar").into());
    l.set_od_caixa_ready(tor("gui.od_caixa_ready", "a fundir").into());
    l.set_od_skills_outdated(
        tor("gui.od_skills_outdated", "evoluíram desde que moldaram este projeto").into(),
    );
    l.set_od_skills_rerun(tor("gui.od_skills_rerun", "Rerodar skills").into());
    // ---- Portal (Home) ----
    l.set_home_academy(tor("gui.home_academy", "Academy").into());
    l.set_home_academy_desc(
        tor("gui.home_academy_desc", "Cursos estruturados de como usar IA.").into(),
    );
    l.set_home_research(tor("gui.home_research", "Research").into());
    l.set_home_research_desc(
        tor("gui.home_research_desc", "Artigos densos, para disseminar conhecimento.").into(),
    );
    l.set_home_blog(tor("gui.home_blog", "Blog").into());
    l.set_home_blog_desc(tor("gui.home_blog_desc", "Opinião, notícias e novidades.").into());
    l.set_notif_history(tor("gui.notif_history", "Histórico").into());

    // ---- Gestor de VPS -----------------------------------------------------
    // `tor` = traduz com FALLBACK: chave ainda não traduzida cai no texto pt-BR em vez
    // de renderizar vazio. Tela nova nasce utilizável nos 11 locales e a tradução chega
    // depois — o contrário deixaria a UI muda pra quem não usa pt.
    l.set_home_vps(tor("gui.home_vps", "VPS").into());
    l.set_home_vps_desc(tor(
        "gui.home_vps_desc",
        "Servidores registrados, deploy auditado e o que cada host aceita — sem chave no contexto.",
    ).into());
    // O QUIZ do overdev. `tor` e não `t` porque as chaves são novas: se um lib antigo
    // estiver linkado, a UI mostra o fallback em vez do nome da chave cru na tela.
    l.set_quiz_title(tor("gui.quiz_title", "Perguntas").into());
    l.set_quiz_esperando_voce(tor("gui.quiz_esperando_voce", "esperando você").into());
    l.set_quiz_revisar(tor("gui.quiz_revisar", "a revisar pela máquina").into());
    l.set_quiz_vazio(tor("gui.quiz_vazio", "Nenhuma pergunta na fila.").into());
    l.set_quiz_sugerida(tor("gui.quiz_sugerida", "sugerida").into());
    l.set_quiz_destrava(tor("gui.quiz_destrava", "Destrava:").into());
    l.set_quiz_confirmar(tor("gui.quiz_confirmar", "Confirmar").into());
    l.set_quiz_enviar(tor("gui.quiz_enviar", "Enviar").into());
    l.set_quiz_outro(tor("gui.quiz_outro", "outro…").into());
    l.set_quiz_resposta(tor("gui.quiz_resposta", "sua resposta…").into());
    l.set_quiz_feito(tor("gui.quiz_feito", "Marcar feito").into());
    l.set_quiz_recusar(tor("gui.quiz_recusar", "Recusar").into());
    l.set_quiz_respondeu(tor("gui.quiz_respondeu", "Você respondeu:").into());
    l.set_quiz_aguarda_revisao(
        tor(
            "gui.quiz_aguarda_revisao",
            "A máquina ainda não revisou esta resposta — o item vinculado segue bloqueado.",
        )
        .into(),
    );
    l.set_quiz_revisao(tor("gui.quiz_revisao", "Revisão da máquina:").into());
}

#[cfg(test)]
mod tests {
    /// **TODA chave que esta ligação usa tem de EXISTIR no catálogo.**
    ///
    /// Este teste nasceu de um defeito visto na tela: a aba de Banco de dados abriu mostrando
    /// `gui.database_title`, `gui.database_body` e `gui.database_missing` — as CHAVES, cruas,
    /// como título e corpo. O motivo era mundano (o catálogo mora no crate irmão e as frases
    /// novas ainda não tinham sido publicadas), e o que importa é o resto: **nada reprovava**.
    /// O `t()` devolve a própria chave quando não acha a tradução, o Rust compila, o Slint
    /// desenha, e quem vê é o usuário.
    ///
    /// Varre o fonte deste arquivo atrás de `t("…")` e cobra que nenhuma volte igual a si
    /// mesma. É exaustivo por construção: chave nova entra aqui junto com a linha que a usa.
    ///
    /// **Vale para o `t(` E para o `tor(`, e os dois falham de jeitos diferentes.**
    ///
    /// O `t()` devolve a própria CHAVE — `gui.database_title` aparece como título da tela. O
    /// `tor()` tem texto de reserva, então ele devolve o **português do código em todos os 20
    /// idiomas**, calado: mais brando, e por isso muito mais fácil de acumular. Quando isto foi
    /// medido pela primeira vez, **163 das 298 chaves `tor` não estavam no catálogo**.
    ///
    /// Elas foram sanadas: 52 eram de telas que já não existem e morreram com as ligações, as
    /// 111 restantes foram traduzidas nos 20 idiomas. Este teste é o que impede a volta — com a
    /// dívida em zero, ele pode ser estrito, e antes não podia.
    #[test]
    fn nenhuma_chave_chega_crua_na_tela() {
        let fonte = include_str!("i18nbind.rs");
        let producao = fonte.split("#[cfg(test)]").next().expect("há código antes dos testes");
        // **O varredor casa `("gui.`, e não `t("`.** A primeira versão procurava `t("` para
        // pegar as duas formas — e não pegava: `tor("` não contém a subsequência `t("`, ela
        // tem `r("`. O teste passava varrendo 67 chaves onde havia 280, e só o piso do
        // self-check abaixo revelou isso. Casar a ABERTURA do argumento pega `t(`, `tor(` e
        // `tf(` sem depender do nome da função.
        let mut vistas = 0;
        for trecho in producao.split("(\"gui.").skip(1) {
            let Some(resto) = trecho.split('"').next() else { continue };
            let chave = format!("gui.{resto}");
            // Só as chaves literais: um argumento montado em tempo de execução não passa aqui.
            if resto.is_empty()
                || !resto.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
            {
                continue;
            }
            vistas += 1;
            assert_ne!(
                crate::prelude::t(&chave),
                chave,
                "`{chave}` não existe no catálogo.\n\nCom `t()` a tela mostra a CHAVE CRUA ao \
                 usuário; com `tor()` ela mostra o PORTUGUÊS do código nos 20 idiomas, calada. \
                 Nos dois casos nada mais reprova. Acrescente a chave aos 20 catálogos em \
                 `schematize_cli_rs/src/i18n/`."
            );
        }
        // Self-check: o varredor tem de ter ACHADO chaves. Um split que parasse de casar faria
        // o laço rodar zero vezes e o teste passar sempre — guard que não pode falhar não mede.
        //
        // O piso é 250, e hoje há bem mais. Ele existe para pegar o varredor cego, não para
        // congelar a contagem: subir a cada chave nova transformaria este teste num contador
        // que reprova toda vez que alguém traduz alguma coisa.
        assert!(vistas > 250, "só {vistas} chaves varridas — o varredor está cego");
    }
}
