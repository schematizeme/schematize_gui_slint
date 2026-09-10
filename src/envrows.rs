//! Linhas da aba Environments + chaves SSH + idiomas, e o disparo das ações de
//! environment num terminal externo.

use crate::prelude::*;

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

#[cfg(test)]
mod tests_gestor {
    use super::*;

    /// **O buraco que a resolução do binário fecha.** O comando era montado com o nome PURO.
    /// A janela aberta pelo lançador do desktop tem PATH mínimo — sem `~/.cargo/bin` —, e o
    /// terminal que ela abre herda esse PATH. O usuário clicava em instalar e recebia
    /// `comando não encontrado` sobre um binário que ESTAVA instalado. Reproduzido com
    /// `env -i PATH=/usr/bin:/bin bash -c 'schematize --version'`.
    ///
    /// O caminho de APP saiu daqui junto com a lista do mercado (ela é da janela do market
    /// agora), mas o de LINGUAGEM continua: é o que o modal do marketplace usa ao instalar o
    /// environment de uma skill.
    #[test]
    fn o_comando_de_environment_usa_o_caminho_resolvido() {
        let cmd = env_terminal_inner("/home/u/.cargo/bin/schematize", "install", "go", "mise");
        assert!(
            cmd.contains("/home/u/.cargo/bin/schematize env install go --method mise"),
            "{cmd}"
        );
        // Self-check: com o nome puro a asserção acima falharia.
        let ruim = env_terminal_inner("schematize", "install", "go", "mise");
        assert!(!ruim.contains("/home/u/.cargo/bin/"), "o self-check parou de valer");
    }

    /// Ferramenta não tem método, e o CLI ignora `--method` para elas: passar a flag sem valor
    /// seria pedir ao CLI que interpretasse um argumento vazio.
    #[test]
    fn sem_metodo_nao_ha_flag_pendurada() {
        let cmd = env_terminal_inner("/b/schematize", "install", "gh", "");
        assert!(!cmd.contains("--method"), "{cmd}");
    }

    /// O terminal espera uma tecla no fim — sem isso ele fecha junto com o processo e o erro
    /// que a pessoa precisa ler some com ele.
    #[test]
    fn o_terminal_nao_fecha_na_cara() {
        assert!(env_terminal_inner("/b/x", "install", "go", "mise").contains("read -n1"));
    }

    /// A JANELA do Mercado é resolvida por caminho, e a ausência dela é `None` — não um
    /// palpite. É o que permite a aba OFERECER instalá-la em vez de mostrar um botão que
    /// tenta abrir o que não existe e não diz nada.
    #[test]
    fn a_janela_do_mercado_e_option_e_nao_palpite() {
        // `None` é resposta, não erro: numa máquina sem a janela instalada é exatamente isso
        // que a aba precisa saber para OFERECER instalá-la.
        if let Some(p) = crate::sysenv::market_gui_bin() {
            assert!(
                std::path::Path::new(&p).is_file() || crate::sysenv::which_bin(&p),
                "só devolve o que existe: {p}"
            );
        }
    }
}
