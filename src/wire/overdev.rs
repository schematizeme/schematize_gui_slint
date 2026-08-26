//! Fiação da aba Overdev: seletor de projeto, checklist fatiado, editor acoplado,
//! notas/correções e o disparo/monitoramento do overdev em terminal externo.
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
    let od_cl = cx.od_cl.clone();
    let od_current = cx.od_current.clone();
    let od_stop_flag = cx.od_stop_flag.clone();

    // escolher um projeto do seletor.
    {
        let weak = app.as_weak();
        let cl = od_cl.clone();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        let sf = od_stop_flag.clone();
        app.global::<Od>().on_pick_project(move |path| {
            if path.is_empty() {
                return;
            }
            if let Some(app) = weak.upgrade() {
                select_project(&app, &cl, &pm, &dm, &pnm, &cur, &sf, PathBuf::from(path.to_string()));
                graph_mark_dirty(&gl); // grafo carrega só quando a aba Grafo abrir
                app.global::<Od>().invoke_refresh_history();
            }
        });
    }
    // abrir uma pasta avulsa (picker NATIVO do sistema).
    {
        let weak = app.as_weak();
        let cl = od_cl.clone();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        let sf = od_stop_flag.clone();
        app.global::<Od>().on_open_folder(move || {
            if let Some(dir) = rfd::FileDialog::new().set_title(t("gui.open_folder")).pick_folder() {
                if let Some(app) = weak.upgrade() {
                    select_project(&app, &cl, &pm, &dm, &pnm, &cur, &sf, dir);
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
        let cl = od_cl.clone();
        let pm = od_proj_model.clone();
        let dm = od_dev_model.clone();
        let pnm = od_pin_model.clone();
        let cur = od_current.clone();
        let gl = graph_loaded.clone();
        app.global::<Od>().on_reload(move || {
            refresh_proj_models(&pm, &dm, &pnm);
            if let Some(app) = weak.upgrade() {
                let p = cur.borrow().clone();
                load_overdev_into(&app, &cl, p.as_deref());
                graph_mark_dirty(&gl); // grafo carrega só quando a aba Grafo abrir
                app.global::<Od>().invoke_refresh_history();
            }
        });
    }
    // Governador de concorrência: "Rechecar" recomputa o teto (CPU/RAM/load − claudes-da-máquina).
    {
        let weak = app.as_weak();
        app.global::<Od>().on_refresh_agents(move || {
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
        app.global::<Od>().on_run_skill_action(move |command| {
            let Some(app) = weak.upgrade() else { return };
            let Some(project) = cur.borrow().clone() else {
                app.global::<Od>().set_run_status(tor("gui.od_no_project", "Escolha um projeto primeiro.").into());
                return;
            };
            match schematize::agentrun::launch_prompt_in_terminal(&project, command.as_str()) {
                Ok(_) => app.global::<Od>().set_run_status(format!("Rodando {command} num terminal externo…").into()),
                Err(e) => app.global::<Od>().set_run_status(format!("falhou ao rodar {command}: {e}").into()),
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
                app.global::<Od>().set_run_status(tor("gui.od_no_project", "Escolha um projeto primeiro.").into());
                return;
            };
            match schematize::agentrun::abrir_terminal_no_projeto(&project) {
                Ok(term) => app.global::<Od>()
                    .set_run_status(format!("terminal `{term}` aberto em {}", project.display()).into()),
                Err(e) => app.global::<Od>().set_run_status(format!("não consegui abrir o terminal: {e}").into()),
            }
        });
    }
    // Split multiagent: divide o checklist em K parts (checklist/part-NN.md) respeitando o teto do
    // governador; com dispatch, lança K claudes (um por fatia), cada um limitado a subagents_each.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_split(move |k, dispatch| {
            let Some(app) = weak.upgrade() else { return };
            let Some(project) = cur.borrow().clone() else {
                app.global::<Od>().set_split_status("Selecione um projeto primeiro.".into());
                return;
            };
            let k = (k.max(2)) as usize;
            let b = schematize::agents::budget();
            if k > b.total_cap {
                app.global::<Od>().set_split_status(
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
                    app.global::<Od>().set_split_status(msg.into());
                    apply_agent_budget(&app); // atualiza o "rodando/livre" após lançar
                }
                Err(e) => app.global::<Od>().set_split_status(format!("split falhou: {e}").into()),
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
                    app.global::<Od>().set_run_status(tor("gui.od_no_project", "Escolha um projeto primeiro.").into());
                }
                return;
            };
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_run_status(
                    tor("gui.od_gen_running", "Gerando afazeres do archive num terminal externo…").into(),
                );
            }
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = agentrun::launch_prompt_in_terminal(&root, &agentrun::archive_todos_prompt());
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        match res {
                            Ok(term) => app.global::<Od>().set_run_status(
                                format!("{} {}", tor("gui.od_gen_ok", "gerando no terminal"), term).into(),
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
        app.global::<Od>().on_unpin(move |path| {
            config::unpin_project(path.as_ref());
            refresh_proj_models(&pm, &dm, &pnm);
        });
    }
    // abrir o painel HTML do projeto atual no navegador.
    {
        let cur = od_current.clone();
        app.global::<Od>().on_open_browser(move || {
            if let Some(p) = cur.borrow().as_ref() {
                let _ = panel::open_in_browser(p);
            }
        });
    }
    // ---- Fase 3: marcar item HUMANO aberto como feito (- [H ]→- [H x]) ----
    // Edita o CHECKLIST.md do projeto e recarrega a view (contagem + itens).
    {
        let weak = app.as_weak();
        let cl = od_cl.clone();
        let cur = od_current.clone();
        app.global::<Od>().on_mark_human(move |idx| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                let _ = mark_human_done_at(&p, idx);
                load_overdev_into(&app, &cl, Some(&p));
            }
        });
    }
    // ---- Checklist fatiado: trocar de PÁGINA ----
    // A UI nunca segura o checklist inteiro; aqui só movemos a janela e
    // republicamos <= checklist::PER_PAGE itens. Custo de render constante.
    {
        let weak = app.as_weak();
        let cl = od_cl.clone();
        app.global::<Od>().on_cl_set_page(move |page| {
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_cl_page(page);
                cl.apply(&app);
            }
        });
    }
    // ---- Checklist fatiado: trocar de FILTRO (todos/abertos/feitos/on-hold/humanos) ----
    // Trocar o filtro volta pra 1ª página — a posição no conjunto antigo não
    // corresponde a nada no novo.
    {
        let weak = app.as_weak();
        let cl = od_cl.clone();
        app.global::<Od>().on_cl_set_filter(move |filter| {
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_cl_filter(filter);
                app.global::<Od>().set_cl_page(0);
                cl.apply(&app);
            }
        });
    }
    // ---- Editor acoplado: abrir/fechar SOB DEMANDA ----
    // Fechado, o `TextEdit` nem existe na árvore do Slint (o `if` no .slint) e o
    // conteúdo nem é lido do disco. Só ao abrir é que pagamos o layout do texto.
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_editor_toggle(move || {
            let root = cur.borrow().clone();
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_editor_open(!app.global::<Od>().get_editor_open());
                if let Some(p) = root {
                    load_editor_content(&app, &p);
                }
            }
        });
    }
    // ---- Editor acoplado: arquivo grande demais → abre no editor externo ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_editor_open_external(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                let target = app.global::<Od>().get_editor_target().to_string();
                open_in_vscode(&overdev_file_path(&p, &target));
            }
        });
    }
    // ---- Fase 3: trocar o arquivo do editor (PLAN.md/CHECKLIST.md) ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_editor_pick(move |target| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                app.global::<Od>().set_editor_target(target);
                load_editor_content(&app, &p);
            }
        });
    }
    // ---- Fase 3: salvar o arquivo do editor (regrava no .overdev/) ----
    // Reflete no checklist/itens se o arquivo salvo for o CHECKLIST.md.
    {
        let weak = app.as_weak();
        let cl = od_cl.clone();
        let cur = od_current.clone();
        app.global::<Od>().on_editor_save(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                // Guarda-corpo: com o editor FECHADO ou com o arquivo acima de
                // EDITOR_MAX_BYTES o `od-editor-content` está VAZIO de propósito.
                // Gravar aqui truncaria o PLAN/CHECKLIST do projeto — recusa.
                if !app.global::<Od>().get_editor_open() || app.global::<Od>().get_editor_too_big() {
                    app.global::<Od>().set_editor_error(true);
                    app.global::<Od>().set_editor_status(
                        tor("gui.od_editor_readonly", "Arquivo grande demais para editar aqui — abra no editor externo.").into(),
                    );
                    return;
                }
                let target = app.global::<Od>().get_editor_target().to_string();
                let content = app.global::<Od>().get_editor_content().to_string();
                let path = overdev_file_path(&p, &target);
                match std::fs::write(&path, content) {
                    Ok(()) => {
                        app.global::<Od>().set_editor_error(false);
                        app.global::<Od>().set_editor_status(tor("gui.saved", "Salvo").into());
                        // salvar o CHECKLIST.md muda o estado 2-níveis: recarrega.
                        if target == "CHECKLIST.md" {
                            load_overdev_into(&app, &cl, Some(&p));
                            app.global::<Od>().set_editor_error(false);
                            app.global::<Od>().set_editor_status(tor("gui.saved", "Salvo").into());
                        }
                    }
                    Err(e) => {
                        app.global::<Od>().set_editor_error(true);
                        app.global::<Od>().set_editor_status(e.to_string().into());
                    }
                }
            }
        });
    }
    // ---- Fase 3: adicionar ponto/nota por task (add_note kind="task") ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_add_note(move |texto| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if !texto.trim().is_empty() {
                    let _ = overdev::add_note(&p, "task", &texto);
                    app.global::<Od>().set_notes(overdev::read_notes(&p).into());
                }
            }
        });
    }
    // ---- Fase 3: prompt de correção do overdev (add_note kind="correcao") ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_add_correction(move |texto| {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if !texto.trim().is_empty() {
                    let _ = overdev::add_note(&p, "correcao", &texto);
                    app.global::<Od>().set_notes(overdev::read_notes(&p).into());
                }
            }
        });
    }
    // ---- Fase 4: "Executar overdev" — passo 1: GUARDRAIL (mostra o comando) ----
    // Não dispara nada; só mostra o comando que abrirá no TERMINAL EXTERNO e abre o
    // mini-modal de confirmação. O disparo real (launch_in_terminal) é no `od-run-confirm`.
    {
        let weak = app.as_weak();
        app.global::<Od>().on_run_request(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_agent_cmdline(
                    tor(
                        "gui.od_agent_cmdline",
                        "claude --dangerously-skip-permissions \"<prompt do overdev>\"  (terminal externo, processo próprio)",
                    )
                    .into(),
                );
                app.global::<Od>().set_confirm_open(true);
            }
        });
    }
    // ---- Fase 4: guardrail — CANCELAR (fecha sem disparar) ----
    {
        let weak = app.as_weak();
        app.global::<Od>().on_run_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_confirm_open(false);
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
        app.global::<Od>().on_run_confirm(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if app.global::<Od>().get_session_running() {
                    return; // já monitorando — não dispara outro
                }
                app.global::<Od>().set_confirm_open(false);
                app.global::<Od>().set_run_status(SharedString::new());
                // Os CONTADORES não são zerados: agora são a fonte única, compartilhada
                // com o bloco de progresso do projeto. Zerá-los pintaria 0 na tela até o
                // 1º tick do monitor (~3s) — mentir pra baixo em vez de pra cima. Eles já
                // carregam a verdade lida do disco; o monitor só os atualiza.
                app.global::<Od>().set_mon_iter(0);
                app.global::<Od>().set_mon_max(0);
                app.global::<Od>().set_mon_mode(SharedString::new());
                app.global::<Od>().set_mon_items(ModelRc::from(Rc::new(VecModel::<SharedString>::from(Vec::new()))));
                stop.store(false, Ordering::SeqCst);
                let objetivo = overdev::objetivo_at(&p).unwrap_or_default();
                let w = weak.clone();
                let stop2 = stop.clone();
                std::thread::spawn(move || match agentrun::launch_in_terminal(&p, &objetivo) {
                    Ok(term) => {
                        let wu = w.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = wu.upgrade() {
                                app.global::<Od>().set_session_running(true);
                                let msg = format!(
                                    "{}{}{}",
                                    tor("gui.od_launched_pre", "claude aberto no terminal "),
                                    term,
                                    tor(
                                        "gui.od_launched_post",
                                        " — o overdev roda fora do app; acompanhe abaixo.",
                                    ),
                                );
                                app.global::<Od>().set_run_status(msg.into());
                            }
                        });
                        run_monitor(w, p, stop2, false);
                    }
                    Err(e) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = w.upgrade() {
                                app.global::<Od>().set_session_running(false);
                                app.global::<Od>().set_run_status(e.into());
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
        app.global::<Od>().on_stop(move || {
            stop.store(true, Ordering::SeqCst);
            if let Some(app) = weak.upgrade() {
                app.global::<Od>().set_run_status(tor("gui.od_stop", "Parar").into());
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
        app.global::<Od>().on_attach(move || {
            let root = cur.borrow().clone();
            let Some(app) = weak.upgrade() else { return };
            let Some(p) = root else {
                app.global::<Od>().set_run_status(tor("gui.od_pick_first", "Selecione um projeto primeiro.").into());
                return;
            };
            if !overdev_dir(&p).is_dir() {
                app.global::<Od>().set_run_status(tor("gui.od_no_overdev_here", "nenhum overdev neste projeto").into());
                return;
            }
            // Já monitorando: não dispara outra thread (evita duplicata na mesma
            // `stop`); só reforça os tokens agora.
            if app.global::<Od>().get_session_running() {
                spawn_usage(weak.clone(), p.clone());
                app.global::<Od>().set_run_status(tor("gui.od_attached", "acompanhando o overdev deste projeto…").into());
                return;
            }
            // Zera o painel e liga o monitor anexado ao run externo.
            app.global::<Od>().set_run_status(tor("gui.od_attached", "acompanhando o overdev deste projeto…").into());
            // Idem: contadores preservados (fonte única — ver a nota no `on_run`).
            app.global::<Od>().set_mon_iter(0);
            app.global::<Od>().set_mon_max(0);
            app.global::<Od>().set_mon_mode(SharedString::new());
            app.global::<Od>().set_mon_items(ModelRc::from(Rc::new(VecModel::<SharedString>::from(Vec::new()))));
            app.global::<Od>().set_session_running(true);
            stop.store(false, Ordering::SeqCst);
            run_monitor(weak.clone(), p, stop.clone(), true);
        });
    }
    // ---- "Atualizar tokens": relê `agent_usage` (PESADO) sob demanda, em thread ----
    {
        let weak = app.as_weak();
        let cur = od_current.clone();
        app.global::<Od>().on_refresh_tokens(move || {
            let root = cur.borrow().clone();
            if let (Some(app), Some(p)) = (weak.upgrade(), root) {
                if overdev_dir(&p).is_dir() {
                    spawn_usage(weak.clone(), p);
                } else {
                    app.global::<Od>().set_run_status(tor("gui.od_no_overdev_here", "nenhum overdev neste projeto").into());
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
