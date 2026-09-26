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

// O `env_terminal_inner` e o `run_env_action` saíram com o modal de instalação (E5).
//
// Eles montavam e disparavam `schematize-market env install <lang> --method <m>` — o passo
// opcional que o modal oferecia junto com a skill. Sem o modal, ninguém os chama: o
// environment se instala pela janela do market, que é de quem ele é desde o ADR-0012.
//
// O que fica neste arquivo é o `build_lang_items`, que é do seletor de IDIOMA — mesmo nome de
// "lang", outro assunto.
