//! Fiação da tela de chaves SSH: listar, gerar, importar, copiar e remover.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Cada função
//! recebe o `AppWindow` e o [`Ctx`] (estado compartilhado) e só liga os callbacks —
//! a lógica de verdade mora nos módulos irmãos.

use crate::prelude::*;
use crate::wire::Ctx;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // ==================== Tela SSH (chaves) ====================
    let ssh_model = Rc::new(VecModel::<SshRow>::from(build_ssh_rows()));
    app.global::<Ssh>().set_rows(ModelRc::from(ssh_model.clone()));
    // re-sonda ~/.ssh.
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.global::<Ssh>().on_refresh(move || {
            m.set_vec(build_ssh_rows());
            if let Some(app) = weak.upgrade() {
                app.global::<Ssh>().set_gen_status(SharedString::new());
                app.global::<Ssh>().set_gen_proof(SharedString::new());
                app.global::<Ssh>().set_bw_result(SharedString::new());
            }
        });
    }
    // exportar uma chave p/ o Bitwarden (cofre destravado OU arquivo de import 600).
    // Roda em THREAD (bw/subprocesso pode bloquear); só o resultado (String, Send)
    // volta pela event loop. A chave PRIVADA nunca chega à UI (o lib a esconde).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.global::<Ssh>().on_export_bw(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let i = idx as usize;
            let Some(mut r) = m.row_data(i) else { return };
            let name = r.name.to_string();
            // marca a linha como ocupada e limpa o banner anterior.
            r.op_label = tor("gui.ssh_bw_exporting", "exportando…").into();
            r.op_error = false;
            m.set_row_data(i, r);
            app.global::<Ssh>().set_bw_result(SharedString::new());
            let weak2 = app.as_weak();
            std::thread::spawn(move || {
                let res = sshkeys::export_bitwarden(&name, None);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak2.upgrade() {
                        // solta o "ocupado" da linha (o modelo é o mesmo VecModel).
                        let rows = app.global::<Ssh>().get_rows();
                        if let Some(mut r) = rows.row_data(i) {
                            r.op_label = SharedString::new();
                            r.op_error = false;
                            rows.set_row_data(i, r);
                        }
                        match res {
                            Ok(msg) => {
                                app.global::<Ssh>().set_bw_result(msg.into());
                                app.global::<Ssh>().set_bw_error(false);
                            }
                            Err(e) => {
                                app.global::<Ssh>().set_bw_result(e.into());
                                app.global::<Ssh>().set_bw_error(true);
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
        app.global::<Ssh>().on_generate(move || {
            let Some(app) = weak.upgrade() else { return };
            let name = app.global::<Ssh>().get_gen_name().to_string();
            let kind_s = app.global::<Ssh>().get_gen_kind().to_string();
            let comment = app.global::<Ssh>().get_gen_comment().to_string();
            let pass = app.global::<Ssh>().get_gen_passphrase().to_string();
            if let Err(e) = sshkeys::valid_name(&name) {
                app.global::<Ssh>().set_gen_error(true);
                app.global::<Ssh>().set_gen_status(e.into());
                return;
            }
            let kind = match sshkeys::KeyKind::parse(&kind_s) {
                Ok(k) => k,
                Err(e) => {
                    app.global::<Ssh>().set_gen_error(true);
                    app.global::<Ssh>().set_gen_status(e.into());
                    return;
                }
            };
            let comment_opt = if comment.trim().is_empty() { None } else { Some(comment.as_str()) };
            let pass_opt = if pass.is_empty() { None } else { Some(pass.as_str()) };
            match sshkeys::generate(&name, kind, comment_opt, pass_opt, false) {
                Ok(info) => {
                    app.global::<Ssh>().set_gen_error(false);
                    app.global::<Ssh>()
                        .set_gen_status(format!("{} · {}", info.name, info.fingerprint).into());
                    // PROVA da chave: bits · fingerprint · tipo (ssh-keygen -l). Confere a força.
                    let proof = sshkeys::proof_line(&info.name).unwrap_or_default();
                    app.global::<Ssh>().set_gen_proof(proof.into());
                    app.global::<Ssh>().set_gen_name(SharedString::new());
                    app.global::<Ssh>().set_gen_comment(SharedString::new());
                    app.global::<Ssh>().set_gen_passphrase(SharedString::new());
                    m.set_vec(build_ssh_rows());
                }
                Err(e) => {
                    app.global::<Ssh>().set_gen_error(true);
                    app.global::<Ssh>().set_gen_status(e.into());
                    app.global::<Ssh>().set_gen_proof(SharedString::new());
                }
            }
        });
    }
    // IMPORTAR uma chave que já existe. Contrapartida do `generate`: um cria, o outro adota.
    // NUNCA sobrescreve (force=false) — na GUI não há como confirmar sem virar modal, e
    // apagar a chave errada em silêncio é irreversível. Quem precisa disso usa a CLI.
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.global::<Ssh>().on_import(move || {
            let Some(app) = weak.upgrade() else { return };
            let file = app.global::<Ssh>().get_imp_file().to_string();
            let name = app.global::<Ssh>().get_imp_name().to_string();
            let pass = app.global::<Ssh>().get_imp_passphrase().to_string();
            let comment = app.global::<Ssh>().get_imp_comment().to_string();

            // `~` não é expandido por ninguém quando o caminho vem de um campo de texto:
            // quem cola "~/backup/chave" está falando do próprio home, e receber
            // "não achei o arquivo" por causa de um til é o tipo de aspereza que o piso
            // "prever macacos" proíbe.
            let expandido = match file.strip_prefix("~/") {
                Some(resto) => schematize::util::home().join(resto),
                None => std::path::PathBuf::from(&file),
            };

            // Sem nome explícito, herda o do arquivo — mesma regra da CLI.
            let nome = if name.trim().is_empty() {
                match expandido.file_stem().and_then(|s| s.to_str()) {
                    Some(n) => n.to_string(),
                    None => {
                        app.global::<Ssh>().set_imp_error(true);
                        app.global::<Ssh>().set_imp_status(
                            "não consegui deduzir o nome — preencha o campo".into(),
                        );
                        return;
                    }
                }
            } else {
                name.trim().to_string()
            };

            let pass_opt = if pass.is_empty() { None } else { Some(pass.as_str()) };
            let comment_opt = if comment.trim().is_empty() { None } else { Some(comment.as_str()) };
            match sshkeys::import(&expandido, &nome, pass_opt, comment_opt, false) {
                Ok(info) => {
                    app.global::<Ssh>().set_imp_error(false);
                    app.global::<Ssh>()
                        .set_imp_status(format!("{} · {}", info.name, info.fingerprint).into());
                    // Limpa o formulário — sobretudo a passphrase, que não fica na tela.
                    app.global::<Ssh>().set_imp_file(SharedString::new());
                    app.global::<Ssh>().set_imp_name(SharedString::new());
                    app.global::<Ssh>().set_imp_passphrase(SharedString::new());
                    app.global::<Ssh>().set_imp_comment(SharedString::new());
                    m.set_vec(build_ssh_rows());
                }
                Err(e) => {
                    app.global::<Ssh>().set_imp_error(true);
                    app.global::<Ssh>().set_imp_status(e.into());
                    // A passphrase FICA: o erro mais comum é ter esquecido de preenchê-la, e
                    // apagar o que a pessoa digitou certo pra ela redigitar é hostil.
                }
            }
        });
    }
    // copiar a PÚBLICA (export_public + clipboard). NUNCA toca a privada.
    {
        let m = ssh_model.clone();
        app.global::<Ssh>().on_copy(move |idx| {
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
        app.global::<Ssh>().on_remove_request(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            if let Some(r) = m.row_data(idx as usize) {
                let name = r.name.to_string();
                app.global::<Ssh>().set_confirm_name(name.clone().into());
                app.global::<Ssh>().set_confirm_msg(
                    format!(
                        "{} '{}'? {}",
                        tor("gui.ssh_remove_confirm", "Remover a chave"),
                        name,
                        tor("gui.ssh_remove_note", "Isto apaga o par (privada + pública).")
                    )
                    .into(),
                );
                app.global::<Ssh>().set_confirm_open(true);
            }
        });
    }
    // confirmar remoção (remove o par).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.global::<Ssh>().on_remove_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let name = app.global::<Ssh>().get_confirm_name().to_string();
            app.global::<Ssh>().set_confirm_open(false);
            if !name.is_empty() {
                match sshkeys::remove(&name) {
                    Ok(()) => m.set_vec(build_ssh_rows()),
                    Err(e) => {
                        app.global::<Ssh>().set_gen_error(true);
                        app.global::<Ssh>().set_gen_status(e.into());
                    }
                }
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Ssh>().on_remove_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Ssh>().set_confirm_open(false);
            }
        });
    }
}
