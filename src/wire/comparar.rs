//! Fiação do COMPARAR fork vs oficial.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI.
//!
//! **Saiu do `appversion.rs` pela mesma razão do sininho** — ver o cabeçalho de
//! [`crate::wire::notificacoes`]. Este recorte é de SKILLS: ele compara a versão que a pessoa
//! editou com a oficial nova, e sai na fase E5, não na do market.
//!
//! **Comparar não sobrescreve nada**, e a tela diz isso: ela mostra as diferenças, e quem
//! decide o que fazer com elas é quem editou.

use crate::prelude::*;
use crate::wire::Ctx;

/// Resultado achatado de `skills::compare_update`, pronto pra cruzar o event loop:
/// (slug, versão, mudanças `nome→descrição`, resumo) ou o erro. Existe como alias porque
/// o tipo cru é ilegível na assinatura e o lint reclamava com razão.
type ComparacaoDeSkill = Result<(String, String, Vec<(String, String)>, String), String>;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // ==================== Comparar fork vs oficial ====================
    // "Comparar com oficial" → compare_update(slug) em thread; abre o painel com
    // base→nova, arquivos (status) e o diff. NÃO sobrescreve nada.
    app.global::<Cmp>().set_files(ModelRc::from(Rc::new(VecModel::<CmpFile>::from(Vec::new()))));
    {
        let weak = app.as_weak();
        app.global::<Cmp>().on_request(move |slug| {
            let Some(app) = weak.upgrade() else { return };
            let slug = slug.to_string();
            if slug.is_empty() {
                return;
            }
            app.global::<Cmp>().set_open(true);
            app.global::<Cmp>().set_loading(true);
            app.global::<Cmp>().set_error(SharedString::new());
            app.global::<Cmp>().set_diff(SharedString::new());
            app.global::<Cmp>().set_versions(SharedString::new());
            app.global::<Cmp>().set_slug(slug.clone().into());
            app.global::<Cmp>()
                .set_title(format!("{} {slug}", tor("gui.compare_title", "Comparar:")).into());
            app.global::<Cmp>()
                .set_files(ModelRc::from(Rc::new(VecModel::<CmpFile>::from(Vec::new()))));
            let weak = weak.clone();
            std::thread::spawn(move || {
                let res = skills::compare_update(&slug);
                // extrai os campos (String/bool) antes de cruzar pro event loop.
                let out: ComparacaoDeSkill = res.map(|c| {
                    (
                        c.base_version,
                        c.new_version,
                        c.files.into_iter().map(|f| (f.path, f.status)).collect(),
                        c.diff_text,
                    )
                });
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.global::<Cmp>().set_loading(false);
                        match out {
                            Ok((base, new, files, diff)) => {
                                app.global::<Cmp>()
                                    .set_versions(format!("v{base} → v{new}").into());
                                app.global::<Cmp>().set_diff(if diff.trim().is_empty() {
                                    tor("gui.compare_identical", "(sem diferenças de conteúdo)")
                                        .into()
                                } else {
                                    diff.into()
                                });
                                app.global::<Cmp>().set_files(ModelRc::from(Rc::new(
                                    VecModel::from(
                                        files
                                            .into_iter()
                                            .map(|(path, status)| CmpFile {
                                                path: path.into(),
                                                status: status.into(),
                                            })
                                            .collect::<Vec<CmpFile>>(),
                                    ),
                                )));
                            }
                            Err(e) => app.global::<Cmp>().set_error(e.into()),
                        }
                    }
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Cmp>().on_close(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Cmp>().set_open(false);
            }
        });
    }
}
