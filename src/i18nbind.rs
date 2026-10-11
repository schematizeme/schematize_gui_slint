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
    // Tooltip do selo verificado (só o check + hover; sem texto ao lado).
    l.set_verified(t("gui.verified_badge").into());
    l.set_tab_overdev(t("gui.tab_overdev").into());
    l.set_tab_graph(t("gui.tab_graph").into());
    l.set_coming_soon(t("gui.coming_soon").into());
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
    l.set_git_title(t("gui.git_title").into());
    l.set_git_body(t("gui.git_body").into());
    l.set_git_open(t("gui.git_open_window").into());
    l.set_git_missing(t("gui.git_missing").into());
    l.set_git_install(t("gui.git_install").into());
    l.set_optimizer_title(t("gui.optimizer_title").into());
    l.set_optimizer_body(t("gui.optimizer_body").into());
    l.set_optimizer_open_disco(t("gui.optimizer_open_disco").into());
    l.set_optimizer_missing(t("gui.optimizer_missing").into());
    l.set_optimizer_install(t("gui.optimizer_install").into());
    l.set_skills_title(t("gui.skills_title").into());
    l.set_skills_body(t("gui.skills_body").into());
    l.set_skills_open(t("gui.skills_open").into());
    l.set_skills_missing(t("gui.skills_missing").into());
    l.set_skills_install(t("gui.skills_install").into());
    l.set_skills_tab(t("gui.skills_tab").into());
    // Faixa da janela do Overdev (E6 M5). As três frases próprias da faixa moram nos 20 catálogos
    // do CLI desde o pino que trouxe o commit `8d5a0b6`; «Instalar a janela», «Perguntas» e
    // «Histórico» já existiam.
    l.set_overdev_open(tor("gui.overdev_open", "Abrir a janela do Overdev").into());
    l.set_overdev_missing(
        tor("gui.overdev_missing", "A janela do Overdev não está instalada nesta máquina.").into(),
    );
    l.set_overdev_install(t("gui.skills_install").into());
    l.set_overdev_aba_perguntas(t("gui.quiz_title").into());
    l.set_overdev_aba_checklist(tor("gui.overdev_aba_checklist", "Checklist").into());
    l.set_overdev_aba_historico(t("gui.od_history").into());
    // modal de instalação do Marketplace
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
    l.set_open_browser(t("gui.open_browser").into());
    l.set_od_open_terminal(tor("gui.od_open_terminal", "abrir no terminal").into());
    // Rótulos dos atalhos da faixa para as abas «Notas» e «Execução» da janela do overdev.
    l.set_od_notes_title(tor("gui.od_notes", "Notas e correções").into());
    l.set_od_run(tor("gui.od_run", "Executar overdev").into());
    l.set_od_gen_archive(tor("gui.od_gen_archive", "Gerar afazeres do archive").into());
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

    // ---- Git (tela 8) ----
    l.set_home_git(tor("gui.home_git", "Git").into());
    l.set_home_git_desc(
        tor("gui.home_git_desc", "Contas, o que ainda não saiu daqui, e seus repositórios.").into(),
    );

    // ---- Atalho para a aba «Caixa» da janela · skills do projeto ----
    l.set_od_caixa_title(tor("gui.od_caixa_title", "Acrescentar ao projeto").into());
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
}

#[cfg(test)]
mod tests {
    /// As chaves que esta GUI usa e o catálogo do CLI AINDA não tem. **Vazia, e deve ficar.**
    ///
    /// **O quê:** exceção nomeada e mínima ao guard `nenhuma_chave_chega_crua_na_tela`.
    ///
    /// **Por que o mecanismo continua:** os 20 catálogos moram em `schematize_cli_rs/src/i18n/`,
    /// outro repo, e o pino do `Cargo.lock` só sobe para commits já publicados. Frase nova da GUI
    /// nasce aqui, com o português de reserva do `tor()`, até o pino chegar. **Quando o catálogo
    /// ganhar a chave, o guard reprova esta lista** — a exceção se desfaz sozinha. As três
    /// chaves da faixa do Overdev saíram dela com o pino em `8d5a0b6`.
    const CHAVES_PENDENTES_NO_CLI: [&str; 0] = [];

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
        // **TODO o crate, e não só este arquivo.** O `build.rs` junta `src/**/*.rs` (só o
        // código de produção de cada um) porque o `include_str!` não aceita glob e uma lista
        // à mão apodreceria no primeiro arquivo novo. Enquanto isto lia só o `i18nbind.rs`,
        // havia 71 usos em outros arquivos e 24 chaves deles fora do catálogo — o guard
        // reprovava sobre o arquivo que já estava certo e era cego para o resto.
        let producao = include_str!(concat!(env!("OUT_DIR"), "/fontes_i18n.txt"));
        // **O varredor casa `("gui.`, e não `t("`.** A primeira versão procurava `t("` para
        // pegar as duas formas — e não pegava: `tor("` não contém a subsequência `t("`, ela
        // tem `r("`. O teste passava varrendo 67 chaves onde havia 280, e só o piso do
        // self-check abaixo revelou isso. Casar a ABERTURA do argumento pega `t(`, `tor(` e
        // `tf(` sem depender do nome da função.
        let mut vistas = 0;
        // **Casa `(` + espaço em branco + `"gui.`**, e não só `("gui.`: um `tor(` quebrado em duas
        // linhas pelo rustfmt (chave e fallback longos) escapava do varredor.
        let pedacos: Vec<&str> = producao.split("\"gui.").collect();
        for (i, trecho) in pedacos.iter().enumerate().skip(1) {
            if !pedacos[i - 1].trim_end().ends_with('(') {
                continue;
            }
            let Some(resto) = trecho.split('"').next() else { continue };
            let chave = format!("gui.{resto}");
            // Só as chaves literais: um argumento montado em tempo de execução não passa aqui.
            if resto.is_empty()
                || !resto.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
            {
                continue;
            }
            vistas += 1;
            // As chaves NOVAS que ainda não chegaram ao CLI são aceitas — e só elas.
            if CHAVES_PENDENTES_NO_CLI.contains(&chave.as_str()) {
                continue;
            }
            assert_ne!(
                crate::prelude::t(&chave),
                chave,
                "`{chave}` não existe no catálogo.\n\nCom `t()` a tela mostra a CHAVE CRUA ao \
                 usuário; com `tor()` ela mostra o PORTUGUÊS do código nos 20 idiomas, calada. \
                 Nos dois casos nada mais reprova. Acrescente a chave aos 20 catálogos em \
                 `schematize_cli_rs/src/i18n/`."
            );
        }
        // **A lista de pendentes tem prazo embutido:** chave que JÁ está no catálogo não pode
        // ficar nela, ou a exceção viraria porta aberta para o português calado.
        for k in CHAVES_PENDENTES_NO_CLI {
            assert_eq!(
                crate::prelude::t(k),
                *k,
                "`{k}` já está no catálogo do CLI — tire-a de CHAVES_PENDENTES_NO_CLI"
            );
        }
        // Self-check: o varredor tem de ter ACHADO chaves. Um split que parasse de casar faria
        // o laço rodar zero vezes e o teste passar sempre — guard que não pode falhar não mede.
        //
        // **O piso é BAIXO de propósito, e eu já o pus alto demais uma vez.**
        //
        // Ele existe para pegar o varredor CEGO — o caso em que o laço roda zero vezes e o
        // teste passa sempre. Não para congelar a contagem. Pus 250 quando havia 280, e a
        // extradição da tela Git levou 29 rótulos consigo: o teste reprovou sobre um hub que
        // tinha ficado MENOR de propósito. Este número vai cair de novo a cada fase, e o
        // guard não pode brigar com o trabalho que ele acompanha.
        assert!(vistas > 100, "só {vistas} chaves varridas — o varredor está cego");
    }
}
