//! A PORTA do hub para `schematize cost …` — isolada para o CLI poder ainda não existir.
//!
//! **O quê:** roda `schematize cost sync` e depois `schematize cost report --json` com o projeto
//! como diretório corrente. O binário é resolvido do mesmo jeito que o resto da GUI
//! ([`crate::sysenv::schematize_bin`]: irmão do executável, depois PATH).
//!
//! **Erro é texto acionável, nunca pânico:** CLI ausente, subcomando `cost` ainda inexistente
//! (versão antiga) ou falha de execução voltam como motivo para a tela. A saída do `report` é
//! ignorada: o CLI grava o `custos.json` no archive e a GUI o relê de lá.

use std::path::Path;
use std::process::{Command, Stdio};

/// Uma etapa do CLI. Devolve o stderr (ou o código de saída) se falhar.
fn etapa(bin: &str, dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new(bin)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("não consegui executar `{bin}`: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.trim();
    Err(if err.is_empty() {
        format!("`{bin} {}` falhou ({})", args.join(" "), out.status)
    } else {
        err.lines().take(4).collect::<Vec<_>>().join(" ")
    })
}

/// **O quê:** `sync` + `report --json` com `bin`. Separada de [`sincronizar`] para o teste
/// passar um binário falso.
pub(crate) fn rodar(bin: &str, dir: &Path) -> Result<(), String> {
    etapa(bin, dir, &["cost", "sync"])?;
    etapa(bin, dir, &["cost", "report", "--json"])
}

/// **O quê:** sincroniza os custos do projeto `dir`. **Onde:** o botão «Sincronizar» (em thread).
pub(crate) fn sincronizar(dir: &Path) -> Result<(), String> {
    rodar(&crate::sysenv::schematize_bin(), dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binario_inexistente_vira_motivo_e_nao_panico() {
        let e = rodar("/nao/existe/schematize-xyz", &std::env::temp_dir()).unwrap_err();
        assert!(e.contains("não consegui executar"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn falha_do_cli_traz_o_stderr() {
        // `false` sai com 1 sem stderr: o motivo cita o código de saída.
        let e = rodar("false", &std::env::temp_dir()).unwrap_err();
        assert!(e.contains("falhou"), "{e}");
        // `true` aceita qualquer argumento e sai 0: o fluxo feliz passa pelas duas etapas.
        assert!(rodar("true", &std::env::temp_dir()).is_ok());
    }
}
