//! Os botões que as skills declaram — lidos do BINÁRIO `schematize-skills`.
//!
//! **O quê:** roda `schematize-skills acoes --json` e devolve as ações que viram botões na aba
//! do projeto, ao lado de «Executar overdev».
//!
//! **Onde:** o arranque da janela.
//!
//! ## É o ÚNICO ponto em que software de TERCEIRO declara UI aqui dentro
//!
//! Uma skill publica um `gui.json` com `label`, `command` e `needs_project`, e espera um botão.
//! O contrato é da SKILL com o ecossistema — e por isso o formato tem de ter **um leitor só**.
//! Enquanto este hub lia os arquivos por conta própria E o app de skills também, uma mudança
//! no formato pedia dois consertos, e o segundo só apareceria quando alguém reclamasse do
//! botão que sumiu.
//!
//! ## O que se perde sem o app instalado, e por que é aceitável
//!
//! Sem `schematize-skills`, a lista vem VAZIA e nenhum botão de skill aparece. Não é silêncio:
//! a aba de Skills, ao lado, já mostra que o app não está instalado e oferece instalá-lo. E o
//! resto da janela continua inteiro (piso 10) — o overdev, o grafo, o git e o disco não
//! dependem disto.
//!
//! A alternativa seria o hub ler o `gui.json` por conta própria como reserva, e aí estariam de
//! volta os dois leitores que este módulo existe para eliminar.

/// Uma ação declarada por uma skill.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Acao {
    pub(crate) skill: String,
    pub(crate) rotulo: String,
    /// A linha de comando, CRUA. O hub não a interpreta — ela foi escrita por terceiro, e
    /// sanitizá-la aqui daria a falsa impressão de que foi validada. Ela roda num terminal,
    /// onde a pessoa a vê antes de confirmar.
    pub(crate) comando: String,
    pub(crate) precisa_projeto: bool,
}

/// **O quê:** as ações declaradas. Lista VAZIA quando o app não está instalado ou falhou.
///
/// **Onde:** o arranque da janela.
pub(crate) fn ler() -> Vec<Acao> {
    let Some(bin) = crate::sysenv::skills_bin() else { return Vec::new() };
    let saida = std::process::Command::new(bin)
        .args(["acoes", "--json"])
        .stdin(std::process::Stdio::null())
        .output();
    let Ok(o) = saida else { return Vec::new() };
    if !o.status.success() {
        return Vec::new();
    }
    interpretar(&String::from_utf8_lossy(&o.stdout))
}

/// **O quê:** o mesmo que [`ler`], a partir do TEXTO. PURA, e é onde os testes entram.
///
/// **Onde:** [`ler`]. Separada porque o comportamento com documento truncado, de outra versão
/// do app ou hostil tem de ser afirmável sem ter o app instalado na máquina de quem roda a
/// suíte — e porque o conteúdo vem de TERCEIRO, que é exatamente o caso em que a entrada
/// hostil não é hipótese.
pub(crate) fn interpretar(texto: &str) -> Vec<Acao> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(texto) else { return Vec::new() };
    let Some(itens) = v.get("acoes").and_then(|a| a.as_array()) else { return Vec::new() };
    itens
        .iter()
        .filter_map(|a| {
            let rotulo = a.get("rotulo")?.as_str()?.trim().to_string();
            let comando = a.get("comando")?.as_str()?.trim().to_string();
            // **Ação sem rótulo ou sem comando não vira botão.** Um botão em branco, ou um que
            // não faz nada ao ser clicado, é pior que a ausência dele: a pessoa clica, nada
            // acontece, e a culpa parece ser do app e não da skill que declarou errado.
            if rotulo.is_empty() || comando.is_empty() {
                return None;
            }
            Some(Acao {
                skill: a.get("skill").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
                rotulo,
                comando,
                precisa_projeto: a.get("precisa_projeto").and_then(|b| b.as_bool()) == Some(true),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"acoes":[
      {"skill":"qa","rotulo":"Q.A.","comando":"/qa-plan","precisa_projeto":true},
      {"skill":"archive","rotulo":"Archive","comando":"/archive-init","precisa_projeto":false}]}"#;

    #[test]
    fn le_as_acoes() {
        let a = interpretar(DOC);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].rotulo, "Q.A.");
        assert_eq!(a[0].comando, "/qa-plan");
        assert!(a[0].precisa_projeto);
        assert!(!a[1].precisa_projeto);
    }

    /// **Ação sem rótulo ou sem comando NÃO vira botão.**
    ///
    /// Um botão em branco, ou um que não faz nada ao ser clicado, é pior que a ausência dele:
    /// a pessoa clica, nada acontece, e a culpa parece ser do app — não da skill que declarou
    /// errado. E isto vem de TERCEIRO, então declarar errado é o caso normal, não a exceção.
    #[test]
    fn acao_incompleta_nao_vira_botao() {
        let a = interpretar(
            r#"{"acoes":[
              {"skill":"x","rotulo":"","comando":"/a"},
              {"skill":"y","rotulo":"B","comando":"   "},
              {"skill":"z","rotulo":"C"},
              {"skill":"w","comando":"/d"},
              {"skill":"ok","rotulo":"Vale","comando":"/vale"}]}"#,
        );
        assert_eq!(a.len(), 1, "só a completa vira botão");
        assert_eq!(a[0].rotulo, "Vale");
    }

    /// **`precisa_projeto` duvidoso conta como FALSE.**
    ///
    /// O lado seguro aqui é o permissivo: marcar `true` sem a skill ter pedido esconderia o
    /// botão de quem não tem projeto aberto, e o autor da skill não teria como descobrir por
    /// que o botão dele nunca aparece.
    #[test]
    fn precisa_projeto_duvidoso_e_falso() {
        let a = interpretar(r#"{"acoes":[{"rotulo":"A","comando":"/a","precisa_projeto":"sim"}]}"#);
        assert!(!a[0].precisa_projeto);
        let a = interpretar(r#"{"acoes":[{"rotulo":"A","comando":"/a"}]}"#);
        assert!(!a[0].precisa_projeto);
    }

    /// **Documento ruim vira lista vazia, e nunca pânico.** Sem o app instalado, nenhum botão
    /// de skill aparece — e o resto da janela continua inteiro (piso 10).
    #[test]
    fn documento_ruim_vira_lista_vazia() {
        let fundo = format!("{}{}", "[".repeat(3000), "]".repeat(3000));
        for lixo in ["", "null", "[]", "0", "{ nao e json", "\u{0}", &fundo, r#"{"acoes":7}"#] {
            assert!(interpretar(lixo).is_empty(), "{lixo:?}");
        }
    }

    /// **O comando sai CRU.** Ele foi escrito por terceiro; sanitizá-lo aqui daria a falsa
    /// impressão de que foi validado, e ele roda num terminal onde a pessoa o vê antes.
    #[test]
    fn o_comando_nao_e_mexido() {
        let a = interpretar(
            r#"{"acoes":[{"rotulo":"X","comando":"/cmd --flag \"com aspas\" && echo oi"}]}"#,
        );
        assert_eq!(a[0].comando, "/cmd --flag \"com aspas\" && echo oi");
    }
}
