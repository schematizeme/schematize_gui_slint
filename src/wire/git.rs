//! Fiação da tela Git: contas, o que ainda não saiu da máquina, e os repositórios.
//!
//! O que orienta este recorte:
//!
//!  - **A pergunta é "o que ainda não saiu daqui".** Git não guarda histórico de
//!    push — esse log não existe. O que existe é `@{u}..HEAD`, e é ele que a tela
//!    põe em destaque: commit que só existe nesta máquina some com a máquina.
//!  - **Trocar de conta escreve config LOCAL do repo.** A global é justamente a
//!    que faz o commit sair com a identidade errada ao pular de projeto.
//!  - **Thread pro que fala com o mundo.** Varrer os repositórios roda `git` uma
//!    dúzia de vezes por projeto; `gh repo list` é rede. Nada disso no event loop.
//!  - **Segredo não passa por aqui.** O cadastro guarda o NOME do arquivo de
//!    chave; autenticar segue com o `gh`/agente SSH.

use crate::prelude::*;
use crate::wire::{set_rows, trava, Ctx};
use schematize::gitcontas::{
    aplicar,
    contas::{self, Auth, Conta},
    repos::{self, EstadoLocal},
};

/// Commits mostrados do projeto selecionado.
const COMMITS: usize = 30;

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    // Estado completo do lado Rust; a UI recebe as linhas já formatadas.
    // `Arc<Mutex<…>>` e não `Rc<RefCell<…>>` porque a varredura roda em thread —
    // ver a nota em `wire::set_rows` sobre por que os modelos voltam do global.
    let projetos: Arc<Mutex<Vec<EstadoLocal>>> = Arc::new(Mutex::new(Vec::new()));

    let g = app.global::<Gh>();
    g.set_accounts(ModelRc::from(Rc::new(VecModel::<GitAccountRow>::from(Vec::new()))));
    g.set_projects(ModelRc::from(Rc::new(VecModel::<GitProjRow>::from(Vec::new()))));
    g.set_repos(ModelRc::from(Rc::new(VecModel::<GitRepoRow>::from(Vec::new()))));
    g.set_commits(ModelRc::from(Rc::new(VecModel::<CommitRow>::from(Vec::new()))));
    set_rows(&g.get_accounts(), gitrows::account_rows(&contas::listar()));

    // ---- reler os repositórios (thread) -----------------------------------
    {
        let weak = app.as_weak();
        let ps = projetos.clone();
        app.global::<Gh>().on_refresh(move || {
            let Some(app) = weak.upgrade() else { return };
            if app.global::<Gh>().get_loading_projects() {
                return;
            }
            app.global::<Gh>().set_loading_projects(true);
            // As contas podem ter mudado por fora (CLI) — relê junto, é barato.
            set_rows(&app.global::<Gh>().get_accounts(), gitrows::account_rows(&contas::listar()));
            let devs = config::dev_dirs();
            let weak2 = weak.clone();
            let ps = ps.clone();
            std::thread::spawn(move || {
                let v = repos::estado_dos_projetos(&devs);
                let linhas = gitrows::proj_rows(&v);
                let risco = gitrows::em_risco(&v);
                let n = v.len();
                *trava(&ps) = v;
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak2.upgrade() else { return };
                    set_rows(&app.global::<Gh>().get_projects(), linhas);
                    app.global::<Gh>().set_at_risk(risco);
                    app.global::<Gh>().set_loading_projects(false);
                    app.global::<Gh>().set_scanned(true);
                    app.global::<Gh>().set_status_error(false);
                    app.global::<Gh>().set_status(
                        format!("{n} {}", tor("gui.git_repo_count", "repositórios")).into(),
                    );
                });
            });
        });
    }

    // ---- cadastrar conta ---------------------------------------------------
    {
        let weak = app.as_weak();
        app.global::<Gh>().on_add_account(move || {
            let Some(app) = weak.upgrade() else { return };
            let g = app.global::<Gh>();
            let chave = g.get_new_chave().to_string();
            let servico = g.get_new_servico().to_string();
            let c = Conta {
                rotulo: g.get_new_rotulo().trim().to_string(),
                usuario: g.get_new_usuario().trim().to_string(),
                email: g.get_new_email().trim().to_string(),
                servico: if servico.trim().is_empty() { "github.com".into() } else { servico },
                // Sem chave = conta `gh` (token guardado por ele). Com chave, o NOME
                // do arquivo em ~/.ssh — nunca o conteúdo.
                auth: if chave.trim().is_empty() {
                    Auth::Gh
                } else {
                    Auth::Ssh { chave: chave.trim().to_string() }
                },
            };
            if c.usuario.is_empty() || c.email.is_empty() {
                g.set_form_error(true);
                g.set_form_status(
                    tor("gui.git_need_user_email", "usuário e e-mail são obrigatórios.").into(),
                );
                return;
            }
            match contas::adicionar(c.clone()) {
                Ok(()) => {
                    set_rows(&g.get_accounts(), gitrows::account_rows(&contas::listar()));
                    g.set_new_rotulo(SharedString::new());
                    g.set_new_usuario(SharedString::new());
                    g.set_new_email(SharedString::new());
                    g.set_new_chave(SharedString::new());
                    g.set_form_error(false);
                    // Conta SSH sem alias no ~/.ssh/config ainda não empurra pela chave
                    // certa — dizer isso agora evita o push que falha depois.
                    g.set_form_status(
                        if matches!(c.auth, Auth::Ssh { .. }) && !aplicar::alias_configurado(&c) {
                            tor(
                                "gui.git_saved_need_alias",
                                "conta salva — falta escrever o alias SSH.",
                            )
                        } else {
                            tor("gui.git_saved", "conta salva.")
                        }
                        .into(),
                    );
                }
                Err(e) => {
                    g.set_form_error(true);
                    g.set_form_status(e.into());
                }
            }
        });
    }

    // ---- remover conta ------------------------------------------------------
    {
        let weak = app.as_weak();
        app.global::<Gh>().on_remove_account(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(r) = app.global::<Gh>().get_accounts().row_data(i as usize) else { return };
            let rotulo = r.rotulo.to_string();
            match contas::remover(&rotulo) {
                Ok(_) => {
                    set_rows(
                        &app.global::<Gh>().get_accounts(),
                        gitrows::account_rows(&contas::listar()),
                    );
                    app.global::<Gh>().set_form_error(false);
                    // Remover o cadastro NÃO desfaz o que já foi aplicado nos repos nem
                    // apaga a chave — dizer o que ficou é mais honesto que "removida".
                    app.global::<Gh>().set_form_status(
                        format!(
                            "{} '{rotulo}' — {}",
                            tor("gui.git_removed", "conta removida:"),
                            tor(
                                "gui.git_removed_note",
                                "a chave e a config dos repositórios ficam como estão."
                            )
                        )
                        .into(),
                    );
                }
                Err(e) => {
                    app.global::<Gh>().set_form_error(true);
                    app.global::<Gh>().set_form_status(e.into());
                }
            }
        });
    }

    // ---- escrever o alias no ~/.ssh/config ----------------------------------
    {
        let weak = app.as_weak();
        app.global::<Gh>().on_write_alias(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(r) = app.global::<Gh>().get_accounts().row_data(i as usize) else { return };
            let Some(c) = contas::por_rotulo(&r.rotulo) else { return };
            let (msg, erro) = match aplicar::escreve_alias(&c) {
                Ok(true) => {
                    (tor("gui.git_alias_written", "alias adicionado ao ~/.ssh/config."), false)
                }
                Ok(false) => (
                    tor("gui.git_alias_noop", "nada a fazer (conta gh ou alias já existe)."),
                    false,
                ),
                Err(e) => (e, true),
            };
            set_rows(&app.global::<Gh>().get_accounts(), gitrows::account_rows(&contas::listar()));
            app.global::<Gh>().set_form_status(msg.into());
            app.global::<Gh>().set_form_error(erro);
        });
    }

    // ---- selecionar conta / projeto -----------------------------------------
    {
        let weak = app.as_weak();
        app.global::<Gh>().on_pick_account(move |r| {
            if let Some(app) = weak.upgrade() {
                app.global::<Gh>().set_sel_account(r);
            }
        });
    }
    {
        let weak = app.as_weak();
        let ps = projetos.clone();
        app.global::<Gh>().on_pick_project(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let lista = trava(&ps);
            let Some(e) = lista.get(idx as usize) else { return };
            app.global::<Gh>().set_sel_project(idx);
            app.global::<Gh>().set_commits_of(e.nome.clone().into());
            // Se o repo já usa uma conta cadastrada, ela vem pré-escolhida — trocar de
            // conta é a exceção, não a regra.
            if let Some(r) = &e.conta {
                app.global::<Gh>().set_sel_account(r.clone().into());
            }
            set_rows(
                &app.global::<Gh>().get_commits(),
                gitrows::commit_rows(&githist::commits(&e.raiz, COMMITS)),
            );
        });
    }
    {
        let ps = projetos.clone();
        app.global::<Gh>().on_open_project(move |idx| {
            if let Some(e) = trava(&ps).get(idx as usize) {
                open_path_in_files(&e.raiz);
            }
        });
    }

    // ---- aplicar a conta ao repositório (com confirmação) -------------------
    {
        let weak = app.as_weak();
        let ps = projetos.clone();
        app.global::<Gh>().on_apply_request(move || {
            let Some(app) = weak.upgrade() else { return };
            let g = app.global::<Gh>();
            let rotulo = g.get_sel_account().to_string();
            let idx = g.get_sel_project();
            let lista = trava(&ps);
            let (Some(e), Some(c)) = (lista.get(idx.max(0) as usize), contas::por_rotulo(&rotulo))
            else {
                return;
            };
            // Diz exatamente o que muda: identidade local e (se der pra saber qual repo
            // é) a URL do remoto. Nada de "aplicar conta?" sem dizer o efeito.
            g.set_confirm_msg(
                format!(
                    "{} '{}' {} {}?\n\n{}: {} <{}>\n{}: {}",
                    tor("gui.git_confirm_use", "Usar a conta"),
                    rotulo,
                    tor("gui.git_confirm_in", "no repositório"),
                    e.nome,
                    tor("gui.git_confirm_identity", "identidade local"),
                    c.usuario,
                    c.email,
                    tor("gui.git_confirm_remote", "host do remoto"),
                    c.host_do_remoto(),
                )
                .into(),
            );
            g.set_confirm_open(true);
        });
    }
    {
        let weak = app.as_weak();
        let ps = projetos.clone();
        app.global::<Gh>().on_confirm_yes(move || {
            let Some(app) = weak.upgrade() else { return };
            let g = app.global::<Gh>();
            g.set_confirm_open(false);
            let rotulo = g.get_sel_account().to_string();
            let idx = g.get_sel_project().max(0) as usize;
            let raiz = { trava(&ps).get(idx).map(|e| e.raiz.clone()) };
            let (Some(raiz), Some(c)) = (raiz, contas::por_rotulo(&rotulo)) else { return };
            match aplicar::aplicar(&raiz, &c, "origin") {
                Ok(feitos) => {
                    g.set_status_error(false);
                    g.set_status(feitos.join(" · ").into());
                    // Reflete o novo estado do repo mexido — sem re-varrer os outros.
                    let nome = trava(&ps)[idx].nome.clone();
                    let novo = repos::estado_de(&raiz, &nome);
                    let linhas = {
                        let mut lista = trava(&ps);
                        lista[idx] = novo;
                        gitrows::proj_rows(&lista)
                    };
                    set_rows(&g.get_projects(), linhas);
                }
                Err(e) => {
                    g.set_status_error(true);
                    g.set_status(e.into());
                }
            }
        });
    }
    {
        let weak = app.as_weak();
        app.global::<Gh>().on_confirm_no(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Gh>().set_confirm_open(false);
            }
        });
    }

    // ---- repositórios do serviço (gh, thread) --------------------------------
    {
        let weak = app.as_weak();
        app.global::<Gh>().on_list_repos(move |rotulo| {
            let Some(app) = weak.upgrade() else { return };
            let Some(c) = contas::por_rotulo(&rotulo) else { return };
            app.global::<Gh>().set_loading_repos(true);
            set_rows(&app.global::<Gh>().get_repos(), Vec::new());
            let weak2 = weak.clone();
            std::thread::spawn(move || {
                let res = repos::listar(&c, 200).map(|v| (v.len(), gitrows::repo_rows(&v)));
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak2.upgrade() else { return };
                    app.global::<Gh>().set_loading_repos(false);
                    match res {
                        Ok((n, linhas)) => {
                            set_rows(&app.global::<Gh>().get_repos(), linhas);
                            app.global::<Gh>().set_status_error(false);
                            app.global::<Gh>().set_status(
                                format!("{n} {}", tor("gui.git_repo_count", "repositórios")).into(),
                            );
                        }
                        // Erro do `gh` vai INTEIRO pra tela: "não está logado" e "não
                        // está instalado" pedem ações diferentes, e uma lista vazia
                        // pareceria "você não tem repositório".
                        Err(e) => {
                            app.global::<Gh>().set_status_error(true);
                            app.global::<Gh>().set_status(e.into());
                        }
                    }
                });
            });
        });
    }
}
