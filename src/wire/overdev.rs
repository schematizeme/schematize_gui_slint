//! Fiação da aba Overdev (o hub): seletor de projeto, dev_dirs/pins, ações de skills e atalhos
//! do projeto (terminal, painel HTML, afazeres do archive).
//!
//! O trabalho do overdev em si (checklist, execução, caixa, notas, split) é da janela
//! `schematize-overdev-gui` — ver [`crate::wire::envs`] (a faixa que a abre).
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, cx: &Ctx) {
    let graph_state = cx.graph_state.clone();
    let graph_nodes = cx.graph_nodes.clone();
    let graph_edges = cx.graph_edges.clone();
    let graph_timer = cx.graph_timer.clone();
    let graph_loaded = cx.graph_loaded.clone();
    let od_proj_model = cx.od_proj_model.clone();
    let od_dev_model = cx.od_dev_model.clone();
    let od_pin_model = cx.od_pin_model.clone();
    let od_current = cx.od_current.clone();

    // escolher um projeto do seletor.
    {
        let weak = app.as_weak();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        app.global::<Od>().on_pick_project(move |path| {
            if path.is_empty() {
                return;
            }
            if let Some(app) = weak.upgrade() {
                select_project(&app, &pm, &dm, &pnm, &cur, PathBuf::from(path.to_string()));
                graph_mark_dirty(&gl); // grafo carrega só quando a aba Grafo abrir
                app.global::<Od>().invoke_refresh_history();
            }
        });
    }
    // abrir uma pasta avulsa (picker NATIVO do sistema).
    {
        let weak = app.as_weak();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        app.global::<Od>().on_open_folder(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(t("gui.open_folder")).pick_folder()
            {
                if let Some(app) = weak.upgrade() {
                    select_project(&app, &pm, &dm, &pnm, &cur, dir);
                    graph_mark_dirty(&gl); // grafo carrega só quando a aba Grafo abrir
                    app.global::<Od>().invoke_refresh_history();
                }
            }
        });
    }
    // abrir a pasta do projeto ATUAL no gerenciador de arquivos (xdg-open <root>).
    {
        let cur = od_current.clone();
        app.global::<Od>().on_open_project_folder(move || {
            if let Some(p) = cur.borrow().as_ref() {
                open_path_in_files(p);
            }
        });
    }
    // Abrir o projeto ATUAL em NOVA JANELA (processo próprio) — overdev de projetos diferentes em
    // paralelo, cada um numa tela. Isolamento por CONSTRUÇÃO (processos separados, sem estado
    // compartilhado): zero vazamento entre projetos. `--project <path>` faz a nova instância abrir
    // já no projeto + aba Overdev.
    {
        let cur = od_current.clone();
        app.global::<Od>().on_new_window(move || {
            let Some(p) = cur.borrow().clone() else { return };
            if let Ok(exe) = std::env::current_exe() {
                let mut cmd = std::process::Command::new(exe);
                cmd.arg("--project").arg(&p);
                cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
                crate::sysenv::desacopla_processo(&mut cmd); // sobrevive a esta janela
                let _ = cmd.spawn();
            }
        });
    }
    // abrir o projeto ATUAL no VSCode (`code <root>` / vscode://file/<root>).
    {
        let cur = od_current.clone();
        app.global::<Od>().on_open_vscode(move || {
            if let Some(p) = cur.borrow().as_ref() {
                open_in_vscode(p);
            }
        });
    }
    // recarregar: re-sonda dev_dirs/projetos e recarrega o projeto atual.
    {
        let weak = app.as_weak();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        app.global::<Od>().on_reload(move || {
            refresh_proj_models(&pm, &dm, &pnm);
            if let Some(app) = weak.upgrade() {
                let p = cur.borrow().clone();
                load_projeto(&app, p.as_deref());
                graph_mark_dirty(&gl); // grafo carrega só quando a aba Grafo abrir
                app.global::<Od>().invoke_refresh_history();
            }
        });
    }
    // Ação de skill (Q.A./Pentest/…): dispara o `command` (ex.: /eng-qa) no claude de um terminal
    // externo, no projeto selecionado. É o botão declarado via gui.json.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_run_skill_action(move |command| {
            let Some(app) = weak.upgrade() else { return };
            let Some(project) = cur.borrow().clone() else {
                app.global::<Od>().set_run_status(
                    tor("gui.od_no_project", "Escolha um projeto primeiro.").into(),
                );
                return;
            };
            match crate::overdevcli::terminal(
                &project,
                crate::overdevcli::Terminal::Prompt(command.as_str()),
            ) {
                Ok(_) => app
                    .global::<Od>()
                    .set_run_status(format!("Rodando {command} num terminal externo…").into()),
                Err(e) => app
                    .global::<Od>()
                    .set_run_status(format!("falhou ao rodar {command}: {e}").into()),
            }
        });
    }
    // Abre um TERMINAL interativo na pasta do projeto, com o claude pronto (bypass ligado).
    // Diferente do "Executar overdev": lá o agente recebe um objetivo e trabalha sozinho;
    // aqui a sessão é do humano, sem prompt embutido, e o shell continua vivo quando o
    // claude sai. Por isso NÃO sobe supervisor — não há run pra vigiar.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_open_terminal(move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(project) = cur.borrow().clone() else {
                app.global::<Od>().set_run_status(
                    tor("gui.od_no_project", "Escolha um projeto primeiro.").into(),
                );
                return;
            };
            match crate::overdevcli::terminal(&project, crate::overdevcli::Terminal::Vazio) {
                Ok(term) => app.global::<Od>().set_run_status(
                    format!("terminal `{term}` aberto em {}", project.display()).into(),
                ),
                Err(e) => app
                    .global::<Od>()
                    .set_run_status(format!("não consegui abrir o terminal: {e}").into()),
            }
        });
    }
    // "Gerar afazeres do archive" — dispara a skill schematize-archive (/archive-todos) num terminal
    // externo: varre o <projeto>_archive/ + git e deriva o .schematize/overdev/CHECKLIST.md do que
    // ficou aberto/prometido-e-não-provado. O archive é criticidade 0 (a skill cria se faltar).
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_gen_from_archive(move || {
            let root = cur.borrow().clone();
            let Some(root) = root else {
                if let Some(app) = weak.upgrade() {
                    app.global::<Od>().set_run_status(
                        tor("gui.od_no_project", "Escolha um projeto primeiro.").into(),
                    );
                }
                return;
            };
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_run_status(
                    tor("gui.od_gen_running", "Gerando afazeres do archive num terminal externo…")
                        .into(),
                );
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = crate::overdevcli::terminal(
                    &root,
                    crate::overdevcli::Terminal::Receita("archive-todos"),
                );
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        match res {
                            Ok(term) => app.global::<Od>().set_run_status(
                                format!("{} {}", tor("gui.od_gen_ok", "gerando no terminal"), term)
                                    .into(),
                            ),
                            Err(e) => app.global::<Od>().set_run_status(e.into()),
                        }
                    }
                });
            });
        });
    }
    // cadastrar um diretório de desenvolvimento (picker nativo).
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.global::<Od>().on_add_dev_dir(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(t("gui.add_dev_dir")).pick_folder()
            {
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
        app.global::<Od>().on_remove_dev_dir(move |path| {
            config::remove_dev_dir(path.as_ref());
            refresh_proj_models(&pm, &dm, &pnm);
        });
    }
    // FIXAR uma pasta como projeto (picker nativo → config::pin_project). Uma pasta
    // fixada vira UM projeto no seletor mesmo sem marcador git (workspace/monorepo).
    {
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        app.global::<Od>().on_pin_folder(move || {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(tor("gui.pin_folder", "Fixar pasta…"))
                .pick_folder()
            {
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
        app.global::<Od>().on_unpin(move |path| {
            config::unpin_project(path.as_ref());
            refresh_proj_models(&pm, &dm, &pnm);
        });
    }
    // abrir o painel HTML do projeto atual no navegador.
    {
        let cur = od_current.clone();
        let weak = app.as_weak();
        app.global::<Od>().on_open_browser(move || {
            if let Some(p) = cur.borrow().as_ref() {
                if let Err(e) = crate::overdevcli::abrir_painel(p) {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Od>().set_run_status(e.into());
                    }
                }
            }
        });
    }

    // ---- entrada na aba Grafo: carrega o grafo do projeto SÓ agora ----
    // Disparado pelo `changed screen` do .slint. É aqui (e só aqui) que se paga o
    // parse do índice e a física — nunca ao escolher um projeto na aba Overdev.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        let gt = graph_timer.clone();
        let gs = graph_state.clone();
        let gn = graph_nodes.clone();
        let ge = graph_edges.clone();
        app.global::<G>().on_enter(move || {
            let p = cur.borrow().clone();
            graph_enter(p.as_deref(), &gl, &gt, &weak, &gs, &gn, &ge);
        });
    }
}
