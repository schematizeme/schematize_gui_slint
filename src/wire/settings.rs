//! Fiação da tela de Configurações: idioma, tema, autostart, hooks do overdev e
//! diretórios de desenvolvimento.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
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
    // Diagnóstico: alterna o diagnóstico de rede (online) — mais lento quando ligado.
    {
        let weak = app.as_weak();
        app.on_cfg_debug_toggle_online(move || {
            if let Some(app) = weak.upgrade() {
                app.set_cfg_debug_online(!app.get_cfg_debug_online());
            }
        });
    }
    // Diagnóstico: gera o relatório de debug numa THREAD (não trava a UI). Só dados
    // Send cruzam a fronteira — o caminho volta como String via invoke_from_event_loop.
    // Offline por default (rápido); com o toggle marcado passa online=true (mais lento).
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
    // Diagnóstico: abre a PASTA do relatório no gerenciador de arquivos (reusa o
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

}
