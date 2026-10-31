//! A PORTA do hub para `schematize cost …` — isolada para o CLI poder ainda não existir.
//!
//! **O quê:** roda `schematize cost sync --projeto <dir>` e depois
//! `schematize cost report --json --projeto <dir>` (contrato `custos-v1.md`). O binário é
//! resolvido do mesmo jeito que o resto da GUI ([`crate::sysenv::schematize_bin`]: irmão do
//! executável, depois PATH). `--projeto` explícito evita depender do cwd do processo.
//!
//! **Erro é texto acionável, nunca pânico:** [`ErroCli`] distingue binário ausente, CLI antigo
//! (sem `cost`) e falha de execução (rc≠0, com a mensagem do CLI). A saída do `report` é
//! ignorada: o CLI grava o `custos.json` no archive e a GUI o relê de lá.

use std::path::Path;
use std::process::{Command, Stdio};

/// Por que o CLI não entregou os custos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ErroCli {
    /// O binário não existe/não executa (caminho tentado).
    Ausente(String),
    /// O binário existe mas não conhece `cost` (instalação antiga).
    Antigo,
    /// rc≠0: a etapa (`sync`/`report`) e o que o CLI disse.
    Falhou { etapa: &'static str, detalhe: String },
}

impl ErroCli {
    /// Texto para a tela, no idioma atual, dizendo o que fazer.
    pub(crate) fn texto(&self) -> String {
        use crate::custosi18n::tc;
        match self {
            ErroCli::Ausente(bin) => tc("cli_missing").replace("{bin}", bin),
            ErroCli::Antigo => tc("cli_old"),
            ErroCli::Falhou { etapa, detalhe } => {
                tc("cli_failed").replace("{etapa}", etapa).replace("{detalhe}", detalhe)
            }
        }
    }
}

/// Uma etapa do CLI (`cost <etapa> [--json] --projeto <dir>`).
fn etapa(bin: &str, dir: &Path, etapa: &'static str, json: bool) -> Result<(), ErroCli> {
    let mut cmd = Command::new(bin);
    cmd.arg("cost").arg(etapa);
    if json {
        cmd.arg("--json");
    }
    let out = cmd
        .arg("--projeto")
        .arg(dir)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| ErroCli::Ausente(bin.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.trim();
    if err.contains("unrecognized subcommand") {
        return Err(ErroCli::Antigo);
    }
    let detalhe = if err.is_empty() {
        format!("rc={}", out.status.code().map_or_else(|| "?".into(), |c| c.to_string()))
    } else {
        err.trim_start_matches("erro:").trim().lines().take(3).collect::<Vec<_>>().join(" ")
    };
    Err(ErroCli::Falhou { etapa, detalhe })
}

/// **O quê:** `sync` + `report --json` com `bin`. Separada de [`sincronizar`] para o teste
/// passar um binário falso.
pub(crate) fn rodar(bin: &str, dir: &Path) -> Result<(), ErroCli> {
    etapa(bin, dir, "sync", false)?;
    etapa(bin, dir, "report", true)
}

/// **O quê:** sincroniza os custos do projeto `dir`. **Onde:** o botão «Sincronizar» (em thread).
pub(crate) fn sincronizar(dir: &Path) -> Result<(), ErroCli> {
    rodar(&crate::sysenv::schematize_bin(), dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binario_inexistente_vira_ausente_e_nao_panico() {
        let e = rodar("/nao/existe/schematize-xyz", &std::env::temp_dir()).unwrap_err();
        assert_eq!(e, ErroCli::Ausente("/nao/existe/schematize-xyz".into()));
        assert!(e.texto().contains("/nao/existe/schematize-xyz"), "{}", e.texto());
    }

    #[cfg(unix)]
    #[test]
    fn rc_diferente_de_zero_traz_a_etapa_e_o_codigo() {
        // `false` sai com 1 sem stderr: o motivo cita a etapa e o rc.
        let e = rodar("false", &std::env::temp_dir()).unwrap_err();
        assert_eq!(e, ErroCli::Falhou { etapa: "sync", detalhe: "rc=1".into() });
        assert!(e.texto().contains("sync"), "{}", e.texto());
        // `true` aceita qualquer argumento e sai 0: o fluxo feliz passa pelas duas etapas.
        assert!(rodar("true", &std::env::temp_dir()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn cli_sem_cost_vira_antigo_e_stderr_do_cli_e_repassado() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("cli-fake-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mk = |nome: &str, corpo: &str| {
            let f = d.join(nome);
            std::fs::write(&f, format!("#!/bin/sh\n{corpo}\n")).unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            f.to_string_lossy().into_owned()
        };
        let antigo = mk("antigo", "echo \"error: unrecognized subcommand 'cost'\" >&2; exit 2");
        assert_eq!(rodar(&antigo, &d).unwrap_err(), ErroCli::Antigo);
        let ruim = mk("ruim", "echo 'erro: nenhum projeto com archive' >&2; exit 1");
        assert_eq!(
            rodar(&ruim, &d).unwrap_err(),
            ErroCli::Falhou { etapa: "sync", detalhe: "nenhum projeto com archive".into() }
        );
        // Os argumentos são os do contrato: `--projeto` explícito nas duas etapas.
        let eco = mk("eco", "echo \"$@\" >> \"$0.log\"");
        rodar(&eco, &d).unwrap();
        let log = std::fs::read_to_string(format!("{eco}.log")).unwrap();
        let dir = d.to_string_lossy();
        assert_eq!(log, format!("cost sync --projeto {dir}\ncost report --json --projeto {dir}\n"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
