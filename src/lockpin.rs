//! Guarda do `Cargo.lock`: a entrada do CLI tem que PINAR um commit do git.
//!
//! **O quê:** um teste, só. Nenhum código de produção mora aqui.
//!
//! **Onde:** roda a cada `cargo test`, e portanto no CI.
//!
//! # O incidente que originou este arquivo
//!
//! Esta GUI consome o `schematize` (o CLI) por **git-dep** — `Cargo.toml` declara
//! `git = "…/schematize-cli", branch = "main"`. O `Cargo.lock` existe aqui por um motivo
//! único: **pinar o commit exato** daquele repo, pra que dois clones do mesmo SHA compilem
//! contra o mesmo CLI.
//!
//! Pra desenvolver os dois lados juntos há um `[patch]` em `.cargo/config.toml` (gitignored)
//! que redireciona a dependência pra pasta ao lado. A ferramenta é boa; a armadilha é que
//! **qualquer `cargo update` feito com o patch ligado reescreve o lock** — e a entrada
//! `schematize` sai como dependência por CAMINHO, **sem linha `source`**.
//!
//! Nessa forma o lock parou de fazer a única coisa que tinha pra fazer. Pior: num clone
//! limpo (o CI, ou a máquina de outra pessoa), onde o `.cargo/config.toml` não existe porque
//! é gitignored, ele descreve algo que o `Cargo.toml` **não declara**.
//!
//! Isso já foi commitado uma vez, e foi pego na revisão — por olho humano, na véspera de
//! publicar. A regra ficou registrada numa mensagem de commit, que é o lugar onde regra vai
//! morrer: ninguém lê `git log` antes de rodar `cargo update`.
//!
//! # Por que um teste e não uma nota
//!
//! Regra que depende de alguém lembrar não é regra, é sorte. Esta aqui é barata de
//! verificar — uma linha de texto no lock — e cara de errar. Vira teste.
//!
//! # Por que ele olha o lock COMMITADO, e não o do disco
//!
//! Descoberto ao escrever este próprio arquivo: **não é só o `cargo update`**. Com o
//! `[patch]` ligado, um `cargo test` comum já reescreve o lock e apaga a linha `source`. A
//! armadilha é bem mais larga do que "lembre-se depois de atualizar dependência" — ela
//! dispara no comando mais banal do dia.
//!
//! Consequência de projeto: um teste que olhasse o arquivo do disco ficaria **vermelho na
//! máquina de todo mundo que usa o override**, e um teste sempre vermelho é um teste que as
//! pessoas aprendem a ignorar. O invariante que importa é sobre o que **entra no
//! repositório**, então é isso que ele lê: `git show HEAD:Cargo.lock`.

#[cfg(test)]
mod tests {
    /// A entrada `schematize` do lock aponta pro git, com SHA, e não pra um caminho local.
    ///
    /// **Se este teste falhar**, quase certamente você rodou `cargo update` (ou qualquer
    /// comando que mexa no lock) com o `[patch]` de `.cargo/config.toml` ligado. O conserto:
    ///
    /// ```text
    /// mv .cargo/config.toml /tmp/     # desliga o override
    /// cargo update -p schematize      # regenera contra o main de verdade
    /// mv /tmp/config.toml .cargo/     # religa, pra seguir desenvolvendo os dois lados
    /// ```
    ///
    /// E rode os testes com o override DESLIGADO antes de commitar: é a configuração que o
    /// CI usa, e é a que decide se o commit compila pra qualquer outra pessoa.
    #[test]
    fn o_lock_pina_um_commit_do_cli() {
        // O lock COMMITADO, não o do disco: com o `[patch]` ligado o do disco está
        // legitimamente reescrito o tempo todo (ver o cabeçalho do módulo).
        let saida = std::process::Command::new("git")
            .args(["show", "HEAD:Cargo.lock"])
            .output()
            .expect("git precisa existir pra verificar o lock commitado");
        assert!(
            saida.status.success(),
            "não consegui ler o `Cargo.lock` commitado: {}",
            String::from_utf8_lossy(&saida.stderr).trim()
        );
        let lock = String::from_utf8_lossy(&saida.stdout).into_owned();

        // A entrada é o bloco `[[package]]` cujo `name = "schematize"`.
        let bloco = lock
            .split("[[package]]")
            .find(|b| b.contains("name = \"schematize\"\n"))
            .expect("o lock não tem entrada pro crate `schematize` — o Cargo.toml declara?");

        let source = bloco.lines().find(|l| l.starts_with("source = ")).unwrap_or("");
        assert!(
            source.contains("git+"),
            "o `Cargo.lock` NÃO pina commit nenhum do CLI.\n\n\
             A entrada `schematize` está {}, e o lock existe aqui só pra fixar o commit do \
             git-dep. Assim ele descreve algo que o `Cargo.toml` não declara, e num clone \
             limpo (o CI) não há `.cargo/config.toml` pra sustentar a ficção.\n\n\
             Conserto: desligue o `[patch]`, rode `cargo update -p schematize`, religue.\n\
             Veja o cabeçalho de `src/lockpin.rs`.",
            if source.is_empty() {
                "SEM linha `source` (dependência por CAMINHO — o `[patch]` local estava ligado)"
                    .to_string()
            } else {
                format!("com `{}`", source.trim())
            }
        );

        // `git+…#<sha>`: sem o fragmento, o "pin" é o branch, que anda sozinho.
        let sha = source.rsplit('#').next().unwrap_or("").trim_end_matches('"');
        assert!(
            sha.len() >= 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "a entrada aponta pro git mas sem SHA completo — `branch = main` anda sozinho, \
             e aí dois clones do mesmo commit desta GUI podem compilar contra CLIs \
             diferentes. Veio: {}",
            source.trim()
        );
    }
}
