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
    app.global::<Cfg>().set_langs(ModelRc::from(cfg_lang_model.clone()));
    app.global::<Cfg>().set_lang_code(cur_lang.clone().into());
    app.global::<Cfg>().set_lang_name(i18n::name_of(&cur_lang).unwrap_or("").into());
    app.global::<Cfg>().set_autostart_on(autostart::is_active());
    mostra_hooks(app, &hooks_dir());
    // trocar idioma AO VIVO: persiste + recarrega TODOS os rótulos estáticos (L).
    {
        let weak = app.as_weak();
        let lm = cfg_lang_model.clone();
        app.global::<Cfg>().on_set_lang(move |code| {
            let Some(app) = weak.upgrade() else { return };
            let c = code.to_string();
            if i18n::set_lang(&c).is_ok() {
                install_i18n(&app);
                app.global::<Cfg>().set_lang_code(c.clone().into());
                app.global::<Cfg>().set_lang_name(i18n::name_of(&c).unwrap_or("").into());
                lm.set_vec(build_lang_items(&c));
            }
        });
    }
    // autostart do agente (systemd --user + XDG). exe = binário do CLI schematize.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_toggle_autostart(move || {
            let Some(app) = weak.upgrade() else { return };
            let on = app.global::<Cfg>().get_autostart_on();
            let res = if on { autostart::disable() } else { autostart::enable(&schematize_bin()) };
            app.global::<Cfg>().set_autostart_on(if res.is_ok() {
                !on
            } else {
                autostart::is_active()
            });
        });
    }
    // hooks do overdev no settings.json do Claude Code — DELEGADO ao app `schematize-overdev`
    // (`enable`/`disable`/`hooks --json`). Sem o app, a chave não finge: mostra a dica de instalar.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_toggle_hooks(move || {
            let Some(app) = weak.upgrade() else { return };
            let dir = hooks_dir();
            let ligar = !app.global::<Cfg>().get_hooks_on();
            if let Err(e) = crate::overdevcli::definir_hooks(&dir, ligar) {
                app.global::<Cfg>().set_hooks_msg(e.into());
            }
            // O estado mostrado é SEMPRE o relido do app, não o que o clique esperava.
            mostra_hooks(&app, &dir);
        });
    }
    // atalho: reusa o modal de diretórios de dev / projetos fixados.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_manage_dirs(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Cfg>().set_modal_open(true);
            }
        });
    }
    // Diagnóstico: alterna o diagnóstico de rede (online) — mais lento quando ligado.
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_debug_toggle_online(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Cfg>().set_debug_online(!app.global::<Cfg>().get_debug_online());
            }
        });
    }
    // Diagnóstico: gera o relatório de debug numa THREAD (não trava a UI). Só dados
    // Send cruzam a fronteira — o caminho volta como String via invoke_from_event_loop.
    // Offline por default (rápido); com o toggle marcado passa online=true (mais lento).
    {
        let weak = app.as_weak();
        app.global::<Cfg>().on_debug_generate(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<Cfg>().get_debug_running() {
                return;
            }
            let online = app.global::<Cfg>().get_debug_online();
            // marca em andamento + limpa o resultado anterior.
            app.global::<Cfg>().set_debug_running(true);
            app.global::<Cfg>().set_debug_path(SharedString::new());
            app.global::<Cfg>().set_debug_summary(SharedString::new());
            app.global::<Cfg>().set_debug_error(SharedString::new());
            let weak = app.as_weak();
            std::thread::spawn(move || {
                let res = debugreport::write_report(None, online); // Result<PathBuf,String> (Send)
                let summary = debugreport::short_summary(); // String (Send)
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Cfg>().set_debug_running(false);
                        match res {
                            Ok(path) => {
                                app.global::<Cfg>()
                                    .set_debug_path(path.to_string_lossy().into_owned().into());
                                app.global::<Cfg>().set_debug_summary(summary.into());
                                app.global::<Cfg>().set_debug_error(SharedString::new());
                            }
                            Err(e) => {
                                app.global::<Cfg>()
                                    .set_debug_error(tf("err.prefix", &[("e", &e)]).into());
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
        app.global::<Cfg>().on_debug_open_folder(move || {
            let Some(app) = weak.upgrade() else { return };
            let path = app.global::<Cfg>().get_debug_path().to_string();
            if path.is_empty() {
                return;
            }
            let p = Path::new(&path);
            let dir = p.parent().unwrap_or(p);
            open_path_in_files(dir);
        });
    }
}

/// **O quê:** o diretório onde rodar os comandos de hooks. **Onde:** a chave de hooks.
/// Os hooks são GLOBAIS (`~/.claude/settings.json`); o app só precisa de um cwd existente.
fn hooks_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
}

/// **O quê:** relê os hooks do app e põe o estado na chave; sem o app (ou com falha) mostra o
/// motivo e, no caso de ausência, a dica de instalar — a MESMA da aba Overdev.
///
/// **Onde:** o arranque da tela e depois de cada clique. `Err` desliga a chave sem afirmar nada
/// além de «não sei»: o texto de `hooks_msg` diz por quê.
pub(crate) fn mostra_hooks(app: &AppWindow, dir: &Path) {
    let cfg = app.global::<Cfg>();
    match crate::overdevcli::hooks_ligados(dir) {
        Ok(on) => {
            cfg.set_hooks_on(on);
            cfg.set_hooks_msg(SharedString::new());
        }
        Err(e) => {
            cfg.set_hooks_on(false);
            let dica = if crate::overdevcli::presente() {
                e
            } else {
                format!("{e} — instale com: {}", crate::wire::envs::COMANDO_INSTALAR_OVERDEV)
            };
            cfg.set_hooks_msg(dica.into());
        }
    }
}
