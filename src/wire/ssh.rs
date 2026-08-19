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
    // Roda em THREAD (bw/subprocesso pode bloquear); só o resultado (String, Send)
    // volta pela event loop. A chave PRIVADA nunca chega à UI (o lib a esconde).
    {
        let weak = app.as_weak();
        let m = ssh_model.clone();
        app.on_ssh_export_bw(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let i = idx as usize;
            let Some(mut r) = m.row_data(i) else { return };
            let name = r.name.to_string();
            // marca a linha como ocupada e limpa o banner anterior.
            r.op_label = tor("gui.ssh_bw_exporting", "exportando…").into();
            r.op_error = false;
            m.set_row_data(i, r);
            app.set_ssh_bw_result(SharedString::new());
            let weak2 = app.as_weak();
            std::thread::spawn(move || {
                let res = sshkeys::export_bitwarden(&name, None);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak2.upgrade() {
                        // solta o "ocupado" da linha (o modelo é o mesmo VecModel).
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
                    app.set_ssh_gen_status(format!("{} · {}", info.name, info.fingerprint).into());
                    // PROVA da chave: bits · fingerprint · tipo (ssh-keygen -l). Confere a força.
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
    // copiar a PÚBLICA (export_public + clipboard). NUNCA toca a privada.
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
    // pedir remoção → abre o modal de confirmação.
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
                        tor("gui.ssh_remove_note", "Isto apaga o par (privada + pública).")
                    )
                    .into(),
                );
                app.set_ssh_confirm_open(true);
            }
        });
    }
    // confirmar remoção (remove o par).
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

}
