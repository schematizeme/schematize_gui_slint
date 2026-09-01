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

/// Constrói o modelo inteiro da aba Environments a partir de `environments::status()`.
pub(crate) fn build_env_rows() -> Vec<EnvRow> {
    build_env_rows_from(&environments::status())
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
