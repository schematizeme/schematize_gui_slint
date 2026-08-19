//! Fiação da aba Gerenciar: criar skill do zero, forkar/editar uma instalada,
//! salvar arquivos e publicar.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
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

}
