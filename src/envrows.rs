//! Linhas da aba Environments + chaves SSH + idiomas, e o disparo das ações de
//! environment num terminal externo.

use crate::prelude::*;

/// Rótulo de status de um environment — mesmas chaves i18n que o `list()` do CLI usa.
pub(crate) fn env_status_label(le: &environments::LangEnv) -> String {
    if let Some(m) = le.installed {
        tf("env.installed_via", &[("method", m.slug())])
    } else if le.runtime_present {
        t("env.installed")
    } else {
        t("env.not_installed")
    }
}

/// Constrói uma linha da aba Environments a partir do status do lib. O
/// `section_title` fica vazio aqui; quem monta a lista (build_env_rows_from) o
/// preenche na PRIMEIRA linha de cada seção (linguagens × ferramentas).
pub(crate) fn env_row(le: &environments::LangEnv) -> EnvRow {
    let methods: Vec<SharedString> = le.methods_available.iter().map(|m| m.slug().into()).collect();
    let method_sel = methods.first().cloned().unwrap_or_default();
    EnvRow {
        lang: le.lang.into(),
        display: le.display.into(),
        category: le.category.into(),
        install_hint: le.install_hint.as_str().into(),
        section_title: SharedString::new(),
        methods: ModelRc::from(Rc::new(VecModel::from(methods))),
        method_sel,
        installed: le.is_installed(),
        status_label: env_status_label(le).into(),
        op_label: SharedString::new(),
    }
}

/// Título traduzido da seção de uma categoria ("language" | "tool").
pub(crate) fn env_section_title(category: &str) -> String {
    match category {
        "app" => tor("gui.env_apps_title", "Apps da casa"),
        "tool" => tor("gui.env_tools_title", "Ferramentas de dev"),
        _ => tor("gui.env_langs_title", "Linguagens"),
    }
}

/// Monta as linhas a partir de um status já sondado, marcando o `section_title`
/// na primeira linha de cada categoria (o `status()` do lib já lista linguagens
/// primeiro e ferramentas depois). Assim a UI renderiza os dois blocos separados.
pub(crate) fn build_env_rows_from(status: &[environments::LangEnv]) -> Vec<EnvRow> {
    let mut last_cat = String::new();
    status
        .iter()
        .map(|le| {
            let mut row = env_row(le);
            if le.category != last_cat {
                last_cat = le.category.to_string();
                row.section_title = env_section_title(le.category).into();
            }
            row
        })
        .collect()
}

/// Constrói o modelo inteiro da aba Environments: runtimes, ferramentas e os APPS DA CASA.
///
/// **Por que os apps entram AQUI, e não num card novo na home (ADR-0014 D7):** esta aba já é
/// a tela do Mercado, e o card "Mercado" da home já a abre. Os três apps são exatamente o que
/// o market instala — o ADR-0012 chama isso de "uma lista só", e foi ele que reconheceu que
/// `env` (linguagens) e `apps` (apps da casa) eram o mesmo produto partido em dois comandos.
///
/// A alternativa era um 13º card na home. A grade de lá é **3×4 exatamente cheia**, e
/// `ui/screen_home.slint` registra por quê: *"linhas com contagens diferentes davam cards de
/// larguras diferentes entre si… era o que deixava a tela torta"*. Pôr os apps onde eles já
/// pertencem custa esta função; mexer na grade custaria a regra que existe porque já foi
/// quebrada uma vez.
pub(crate) fn build_env_rows() -> Vec<EnvRow> {
    let mut linhas = build_env_rows_from(&environments::status());
    linhas.extend(build_app_rows());
    linhas
}

/// **O quê:** uma linha por app da casa (deployer, optimizer, market), com estado e versão.
///
/// **Onde:** [`build_env_rows`], como terceira seção da aba.
///
/// **O terceiro estado importa:** um binário que está lá e não responde (`Quebrado`) é
/// problema diferente de um que não existe. Dizer "não instalado" sobre o primeiro mandaria a
/// pessoa reinstalar o que já tem, e esconderia a causa real (permissão, lib faltando,
/// arquitetura errada).
pub(crate) fn build_app_rows() -> Vec<EnvRow> {
    use deployerlink::Estado;
    deployerlink::EXTERNOS
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (installed, status_label) = match deployerlink::descobrir_app(a.bin) {
                Estado::Instalado { versao, .. } => {
                    (true, tf("env.installed_via", &[("method", &versao)]))
                }
                Estado::Quebrado { .. } => {
                    (false, tor("gui.app_broken", "quebrado — não responde"))
                }
                Estado::Ausente => (false, t("env.not_installed")),
            };
            EnvRow {
                lang: a.bin.into(),
                display: a.bin.into(),
                category: "app".into(),
                install_hint: a.sobre.into(),
                // A PRIMEIRA linha abre a seção; as outras não repetem o título.
                section_title: if i == 0 {
                    env_section_title("app").into()
                } else {
                    SharedString::new()
                },
                // App da casa não tem "método": ele compila do fonte, e só. A lista vazia é o
                // que faz a UI não desenhar chip de método nesta seção.
                methods: ModelRc::from(Rc::new(VecModel::<SharedString>::from(Vec::new()))),
                method_sel: SharedString::new(),
                installed,
                status_label: status_label.into(),
                op_label: SharedString::new(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// SSH — modelo da tela de chaves a partir de `sshkeys::list()` (só metadados
// PÚBLICOS; a privada nunca é lida/exposta). Igual ao padrão dos demais modelos.
// ---------------------------------------------------------------------------
pub(crate) fn build_ssh_rows() -> Vec<SshRow> {
    sshkeys::list()
        .into_iter()
        .map(|k| SshRow {
            name: k.name.into(),
            kind: k.kind.into(),
            comment: k.comment.into(),
            fingerprint: k.fingerprint.into(),
            public_path: k.public_path.into(),
            op_label: SharedString::new(),
            op_error: false,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Idiomas p/ o seletor de Configurações (código + nome nativo + marca do atual).
// ---------------------------------------------------------------------------
pub(crate) fn build_lang_items(current: &str) -> Vec<LangItem> {
    i18n::LANGS
        .iter()
        .map(|(code, name, _)| LangItem {
            code: (*code).into(),
            name: (*name).into(),
            current: *code == current,
        })
        .collect()
}

/// **O quê:** instala um app da casa abrindo um TERMINAL com `schematize-market install`.
/// Devolve o rótulo transitório para a linha.
///
/// **Onde:** o botão "instalar" da seção de apps na aba do Mercado.
///
/// ## Por que TERMINAL, e não uma barra de progresso dentro da janela
///
/// O ADR-0014 (D7) pediu "botão que instala", e instalar um app da casa **compila do fonte
/// por minutos** — pode pedir sudo para as libs de build, e o `cargo` fala o tempo todo.
/// Fazer isso dentro do event loop travaria a janela; fazer numa thread com barra de
/// progresso exigiria reimplementar, em Slint, o que um terminal já faz melhor: mostrar a
/// saída ao vivo, aceitar `Ctrl-C`, e deixar o erro na tela para ser lido e copiado.
///
/// É o mesmo caminho que esta aba já usa para instalar linguagem, e pela mesma razão. O
/// progresso, o cancelamento e a mensagem de erro acionável vêm de graça — e são de verdade,
/// não uma aproximação desenhada.
///
/// **Sem `-y`:** o market mostra o que vai fazer e PEDE confirmação ali dentro. Consentimento
/// honesto vale mais aqui do que um clique a menos, porque o que se consente é minutos de CPU
/// e, às vezes, a senha do sudo.
/// **O quê:** monta o comando de terminal que instala um app da casa. PURA (testável).
///
/// **Onde:** [`run_app_install`].
///
/// **Por que é função separada, como a `env_terminal_inner` ao lado:** enquanto este comando
/// era um `format!` embutido no meio do disparo do terminal, ele não tinha teste — e foi
/// justamente ali que o nome puro do gestor sobreviveu, enquanto o caminho de linguagem, que
/// JÁ era puro e testado, resolvia o binário direito. A assimetria entre os dois era a forma
/// do bug.
pub(crate) fn app_install_inner(gestor: &str, bin: &str) -> String {
    format!(
        "echo '── {gestor} install {bin} ──'; echo; \
         {gestor} install {bin}; \
         echo; read -n1 -s -r -p '…'",
    )
}

pub(crate) fn run_app_install(bin: &str) -> String {
    // O gestor é resolvido, NUNCA escrito com o nome puro. A janela aberta pelo lançador do
    // desktop tem PATH mínimo (sem `~/.cargo/bin`), e o terminal que ela abre herda esse PATH:
    // com o nome puro o usuário via `schematize-market: comando não encontrado` sobre um
    // gestor que estava instalado. Ver `sysenv::bin_irmao`.
    let gestor = market_bin();
    let inner = app_install_inner(&gestor, bin);
    if launch_terminal(&inner) {
        t("gui.env_terminal_opened")
    } else {
        // Sem terminal a janela não some com o problema: ela entrega o comando para a pessoa
        // rodar onde quiser. É o mesmo contrato do caminho de linguagem.
        tf("gui.env_no_terminal", &[("cmd", &format!("{gestor} install {bin}"))])
    }
}

/// Monta o comando do terminal p/ `schematize env <action> <lang> --method <m>`.
/// SEM `--yes`: o CLI mostra o plano e PEDE confirmação ali dentro (consentimento honesto).
pub(crate) fn env_terminal_inner(bin: &str, action: &str, lang: &str, method: &str) -> String {
    // Ferramentas não têm método (o CLI ignora `--method` pra elas) → omite o flag
    // quando `method` vem vazio, pra não passar um `--method ` sem valor.
    let (tag, method_arg) = if method.is_empty() {
        (String::new(), String::new())
    } else {
        (format!(" ({method})"), format!(" --method {method}"))
    };
    format!(
        "echo '── schematize env {action} {lang}{tag} ──'; echo; \
         {bin} env {action} {lang}{method_arg}; \
         echo; read -n1 -s -r -p '…'",
        action = action,
        lang = lang,
        tag = tag,
        method_arg = method_arg,
        bin = bin
    )
}

/// Dispara o terminal p/ uma ação de environment e devolve o rótulo transitório a exibir
/// na linha (terminal aberto, ou instrução manual quando nenhum terminal foi encontrado).
pub(crate) fn run_env_action(action: &str, lang: &str, method: &str) -> String {
    let bin = schematize_bin();
    let inner = env_terminal_inner(&bin, action, lang, method);
    if launch_terminal(&inner) {
        t("gui.env_terminal_opened")
    } else {
        let method_arg =
            if method.is_empty() { String::new() } else { format!(" --method {method}") };
        let cmd = format!("{bin} env {action} {lang}{method_arg}");
        tf("gui.env_no_terminal", &[("cmd", &cmd)])
    }
}

#[cfg(test)]
mod tests_gestor {
    use super::*;

    /// **O buraco que isto fecha.** O comando era montado com o nome PURO
    /// (`schematize-market install <app>`). A janela aberta pelo lançador do desktop tem PATH
    /// mínimo — sem `~/.cargo/bin` —, e o terminal que ela abre herda esse PATH. O usuário
    /// clicava em instalar e recebia:
    ///
    /// ```text
    /// /usr/bin/bash: linha 1: schematize-market: comando não encontrado
    /// ```
    ///
    /// …sobre um gestor que ESTAVA instalado. Reproduzido com
    /// `env -i PATH=/usr/bin:/bin bash -c 'schematize-market --version'`.
    #[test]
    fn o_comando_usa_o_caminho_resolvido_e_nao_o_nome_puro() {
        let cmd = app_install_inner("/home/u/.cargo/bin/schematize-market", "schematize-deployer");
        assert!(cmd.contains("/home/u/.cargo/bin/schematize-market install schematize-deployer"));
        // Self-check do próprio teste: com o nome puro, a asserção acima falharia.
        let ruim = app_install_inner("schematize-market", "schematize-deployer");
        assert!(!ruim.contains("/home/u/.cargo/bin/"), "o self-check parou de valer");
    }

    /// O eco que a pessoa lê no topo do terminal mostra o MESMO comando que vai rodar. Se ele
    /// dissesse o nome puro e executasse o caminho, a mensagem de erro que ela copiasse para
    /// pedir ajuda seria sobre um comando que ninguém rodou.
    #[test]
    fn o_eco_e_o_comando_executado_sao_o_mesmo() {
        let g = "/opt/x/schematize-market";
        let cmd = app_install_inner(g, "schematize-optimizer");
        assert_eq!(cmd.matches(&format!("{g} install schematize-optimizer")).count(), 2, "{cmd}");
    }

    /// `market_bin()` nunca devolve vazio — devolveria um comando que começa com um espaço e
    /// tentaria executar o argumento como programa.
    #[test]
    fn market_bin_nunca_e_vazio() {
        assert!(!market_bin().is_empty());
        assert!(market_bin().contains("schematize-market"));
    }
}
