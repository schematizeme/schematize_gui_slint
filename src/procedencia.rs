//! De qual commit este binário foi construído — só a COLA com o `build.rs` desta janela.
//!
//! **O quê:** a decisão (rótulo, SHA válido, sujo) mora em `schematize_plataforma::procedencia`;
//! aqui ficam só os NOMES das variáveis que o `build.rs` deste repo emite.
//!
//! **Onde:** [`rotulo_versao`] alimenta o `--version` desta janela (`main.rs`).

/// **O quê:** o rótulo de versão deste binário (`"0.9.2 (b9fcc65abcde)"`, `-sujo` ou
/// `fonte sem git`), `&'static str`.
///
/// **Onde:** o `--version` do `main.rs`.
pub fn rotulo_versao() -> &'static str {
    schematize_plataforma::rotulo_versao!("SCHEMATIZE_GUI_SHA", "SCHEMATIZE_GUI_DIRTY")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O valor real deste build começa pela versão e traz a procedência entre parênteses.
    #[test]
    fn o_build_de_verdade_traz_versao_e_procedencia() {
        let r = rotulo_versao();
        assert!(r.starts_with(env!("CARGO_PKG_VERSION")), "{r}");
        assert!(r.ends_with(')') && r.contains(" ("), "{r}");
    }
}
