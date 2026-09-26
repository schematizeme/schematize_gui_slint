//! A versão do app e a dos irmãos — lida do `schematize-market status --json`.
//!
//! **O quê:** roda o gestor uma vez e devolve o que ele sabe: versão instalada, última
//! publicada, e o PIN.
//!
//! **Onde:** [`crate::wire::appversion`], no rodapé do app.
//!
//! ## Por que o hub deixou de fazer essa conta (E4 M5 da extradição)
//!
//! O que havia aqui era `upgrade::app_update_available()`: uma chamada de rede própria ao
//! GitHub, comparando `app_version()` com a última tag. Duas coisas erradas com isso, e a
//! segunda é a que importa.
//!
//! **A primeira é duplicação:** o market já faz essa leitura, com cache e com a matriz de
//! plataformas. Duas leituras do mesmo fato divergem — foi assim que as duas cópias de
//! `environments/` acabaram discordando sobre instalar .NET no openSUSE.
//!
//! **A segunda é um DEFEITO, não um cheiro: a conta do hub ignorava o PIN.** Quem fixou uma
//! versão fixou por um motivo, e o rodapé anunciava "nova versão disponível" mesmo assim. O
//! botão ao lado chamava o self-update, que delega ao market — e o market, respeitando o pin,
//! não atualizava. O resultado era um app que oferece uma coisa e não a faz, sem dizer por quê.
//!
//! O `target_version` do market é `read_pin().or_else(latest)`. Perguntar a ele é perguntar a
//! quem tem a resposta inteira.

/// O que o gestor sabe sobre as versões nesta máquina.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Status {
    /// A versão do hub que está no disco, ou vazio se o gestor não a viu.
    pub(crate) instalada: String,
    /// A última publicada.
    pub(crate) ultima: String,
    /// A versão FIXADA, quando há uma. Vazio = seguir a última.
    pub(crate) pin: String,
}

impl Status {
    /// **O quê:** o alvo desta máquina — o pin quando há um, senão a última publicada.
    ///
    /// **Onde:** [`Status::ha_atualizacao`] e o texto do rodapé.
    ///
    /// **É a MESMA regra do `target_version` do market**, e ela mora aqui por leitura, não por
    /// cópia: o pin vem no documento porque o market o pôs lá.
    pub(crate) fn alvo(&self) -> &str {
        if self.pin.is_empty() {
            &self.ultima
        } else {
            &self.pin
        }
    }

    /// **O quê:** há o que atualizar? `None` quando não dá para saber.
    ///
    /// **Onde:** o rodapé do app.
    ///
    /// **`None` e `Some(false)` são coisas diferentes**, e o rodapé trata as duas de formas
    /// diferentes: sem o gestor instalado ou sem rede, a resposta é "não sei", e anunciar
    /// "está atualizado" sobre um "não sei" é a mesma confusão entre ausência e vazio que já
    /// custou uma versão a este projeto.
    pub(crate) fn ha_atualizacao(&self) -> Option<bool> {
        if self.instalada.is_empty() || self.alvo().is_empty() {
            return None;
        }
        Some(schematize::util::semver_lt(&self.instalada, self.alvo()))
    }
}

/// **O quê:** roda o gestor e lê o documento. `None` quando ele não está instalado ou falhou.
///
/// **Onde:** o rodapé, numa thread — é chamada de rede do outro lado.
pub(crate) fn ler() -> Option<Status> {
    let bin = crate::sysenv::market_bin_opcional()?;
    let o = std::process::Command::new(bin)
        .args(["status", "--json"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !o.status.success() {
        return None;
    }
    interpretar(&String::from_utf8_lossy(&o.stdout))
}

/// **O quê:** o mesmo que [`ler`], a partir do TEXTO. PURA, e é onde os testes entram.
///
/// **Onde:** [`ler`]. Separada porque o comportamento com documento truncado, de outra versão
/// do market ou hostil tem de ser afirmável sem ter o gestor instalado na máquina de quem roda
/// a suíte.
pub(crate) fn interpretar(texto: &str) -> Option<Status> {
    let v: serde_json::Value = serde_json::from_str(texto).ok()?;
    let s =
        |c: &str| -> String { v.get(c).and_then(|x| x.as_str()).unwrap_or_default().to_string() };
    let st = Status { instalada: s("app_installed"), ultima: s("app_latest"), pin: s("pin") };
    // Um documento que não tem NENHUM dos três campos não é o status do market — é outra
    // coisa que por acaso é JSON. Devolver um `Status` vazio faria o rodapé dizer "não sei"
    // em vez de "não consegui perguntar", e as duas coisas pedem mensagens diferentes.
    if st.instalada.is_empty() && st.ultima.is_empty() && st.pin.is_empty() {
        return None;
    }
    Some(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"market":"0.3.0","os":"linux","app_installed":"0.66.0",
      "app_latest":"0.67.0","pin":null,"apps":[]}"#;

    #[test]
    fn le_o_documento_do_market() {
        let s = interpretar(DOC).expect("lê");
        assert_eq!((s.instalada.as_str(), s.ultima.as_str()), ("0.66.0", "0.67.0"));
        assert_eq!(s.alvo(), "0.67.0", "sem pin, o alvo é a última");
        assert_eq!(s.ha_atualizacao(), Some(true));
    }

    /// **O PIN manda, e é o defeito que este módulo existe para consertar.**
    ///
    /// A conta antiga do hub comparava a versão instalada com a última tag do GitHub, sem
    /// olhar o pin. O rodapé anunciava "nova versão disponível" a quem fixou uma versão de
    /// propósito, e o botão ao lado não atualizava — porque o market, esse sim, respeita o
    /// pin. Um app que oferece uma coisa e não a faz, sem dizer por quê.
    #[test]
    fn com_pin_nao_ha_atualizacao_a_oferecer() {
        let doc = r#"{"app_installed":"0.66.0","app_latest":"0.99.0","pin":"0.66.0"}"#;
        let s = interpretar(doc).expect("lê");
        assert_eq!(s.alvo(), "0.66.0", "o alvo é o PIN, não a última publicada");
        assert_eq!(s.ha_atualizacao(), Some(false), "quem fixou não recebe oferta");
        // E um pin ADIANTE do instalado continua sendo uma atualização legítima: fixar não é
        // congelar para trás, é escolher um alvo.
        let doc = r#"{"app_installed":"0.60.0","app_latest":"0.99.0","pin":"0.66.0"}"#;
        assert_eq!(interpretar(doc).expect("lê").ha_atualizacao(), Some(true));
    }

    /// **"Não sei" e "está atualizado" são respostas DIFERENTES.**
    ///
    /// Sem o gestor ou sem rede, o documento vem incompleto. Anunciar "está atualizado" sobre
    /// isso é a mesma confusão entre ausência e vazio que já custou uma versão a este projeto.
    #[test]
    fn nao_sei_nao_vira_esta_atualizado() {
        let s =
            interpretar(r#"{"app_installed":"0.66.0","app_latest":null,"pin":null}"#).expect("lê");
        assert_eq!(s.ha_atualizacao(), None, "sem alvo, não dá para afirmar nada");
        let s = interpretar(r#"{"app_installed":null,"app_latest":"0.67.0"}"#).expect("lê");
        assert_eq!(s.ha_atualizacao(), None, "sem instalada, idem");
    }

    /// Documento que não é o status do market vira `None`, e não um `Status` vazio: "não
    /// consegui perguntar" e "perguntei e não sei" pedem mensagens diferentes.
    #[test]
    fn documento_de_outra_coisa_nao_vira_status_vazio() {
        assert!(interpretar(r#"{"algo":"outro"}"#).is_none());
        let fundo = format!("{}{}", "[".repeat(3000), "]".repeat(3000));
        for lixo in ["", "null", "[]", "0", "{ nao e json", "\u{0}", &fundo] {
            assert!(interpretar(lixo).is_none(), "{lixo:?}");
        }
    }
}
