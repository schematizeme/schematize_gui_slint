//! Formatação de valores pra UI (hoje: a linha de upstream dos commits).
//! Funções puras — sem I/O e sem tocar em propriedade. Testáveis em unidade.

/// Linha de upstream (branch → remote · ↑ahead ↓behind); vazia se sem tracking.
pub(crate) fn fmt_upstream(up: Option<crate::gitlog::Upstream>) -> String {
    match up {
        Some(u) => {
            let remote = u.remote.unwrap_or_else(|| "—".into());
            format!("{} → {} · ↑{} ↓{}", u.branch, remote, u.ahead, u.behind)
        }
        None => String::new(),
    }
}
