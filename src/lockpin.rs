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
//! # O segundo defeito, achado depois — e causado pela correção do primeiro
//!
//! O conserto do lock que não pinava foi: bumpar a versão no `Cargo.toml` e rodar
//! `git checkout Cargo.lock` pra desfazer a sujeira do `[patch]`. Esse `checkout` reverte o
//! lock INTEIRO — inclusive a **própria versão do crate**, que o bump tinha atualizado
//! legitimamente.
//!
//! Resultado: `Cargo.toml` em `0.8.5` e `Cargo.lock` afirmando `schematize-gui-slint 0.8.3`.
//! O build normal não reclama (cargo conserta o lock em silêncio), mas **`cargo build
//! --locked` FALHA** — e `--locked` é justamente o que se usa pra build reproduzível, que é
//! o ponto inteiro de commitar um lock.
//!
//! Quem achou foi a camada 11 do Q.A., ao **executar** a sequência do release em vez de ler
//! o workflow: o `cargo update --precise` imprimiu `schematize-gui-slint v0.8.3 -> v0.8.5` de
//! passagem, e aquele "de passagem" era o bug.
//!
//! Por isso este arquivo passou a checar DUAS coisas. A primeira guarda contra o lock que
//! descreve a dependência errada; a segunda, contra o lock que descreve o PRÓPRIO crate
//! errado. As duas nascem do mesmo hábito — mexer no lock com o override ligado — e nenhuma
//! das duas aparece num `cargo build` comum.
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
    /// O lock commitado, como texto.
    fn lock_commitado() -> String {
        let saida = std::process::Command::new("git")
            .args(["show", "HEAD:Cargo.lock"])
            .output()
            .expect("git precisa existir pra verificar o lock commitado");
        assert!(
            saida.status.success(),
            "não consegui ler o `Cargo.lock` commitado: {}",
            String::from_utf8_lossy(&saida.stderr).trim()
        );
        String::from_utf8_lossy(&saida.stdout).into_owned()
    }

    /// O lock não pode descrever este crate com uma versão que o `Cargo.toml` não tem mais.
    ///
    /// **Por que é um teste separado do de cima:** são dois defeitos diferentes com a mesma
    /// origem (mexer no lock com o `[patch]` ligado). O de cima pega "o lock descreve a
    /// DEPENDÊNCIA errada"; este pega "o lock descreve O PRÓPRIO CRATE errado".
    ///
    /// **Por que importa, se o `cargo build` não reclama:** ele conserta o lock em silêncio.
    /// Quem reclama é **`cargo build --locked`** — exatamente o modo que existe pra build
    /// reproduzível, que é a única razão de commitar um lock. Um lock que quebra `--locked`
    /// é um lock que não faz o trabalho dele.
    #[test]
    fn o_lock_descreve_a_versao_atual_deste_crate() {
        let toml = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml");
        let versao = toml
            .lines()
            .find_map(|l| l.strip_prefix("version = "))
            .map(|v| v.trim().trim_matches('"').to_string())
            .expect("`version` no Cargo.toml");
        let nome = toml
            .lines()
            .find_map(|l| l.strip_prefix("name = "))
            .map(|v| v.trim().trim_matches('"').to_string())
            .expect("`name` no Cargo.toml");

        let lock = lock_commitado();
        let bloco = lock
            .split("[[package]]")
            .find(|b| b.contains(&format!("name = \"{nome}\"\n")))
            .expect("o lock não tem entrada para o próprio crate");
        let no_lock = bloco
            .lines()
            .find_map(|l| l.strip_prefix("version = "))
            .map(|v| v.trim().trim_matches('"'))
            .unwrap_or("");

        assert_eq!(
            no_lock, versao,
            "o `Cargo.lock` commitado diz que este crate é {no_lock:?}, mas o `Cargo.toml` \
             diz {versao:?}.\n\n\
             `cargo build` conserta isso em silêncio; `cargo build --locked` FALHA — e \
             `--locked` é o modo de build reproduzível, a razão inteira de commitar um lock.\n\n\
             Quase sempre a causa é ter bumpado a versão e depois rodado `git checkout \
             Cargo.lock` pra desfazer a sujeira do `[patch]`: o checkout reverte o lock \
             INTEIRO, inclusive o bump.\n\n\
             Conserto: desligue o `[patch]`, rode `cargo update -p {nome} --precise {versao}`, \
             religue."
        );
    }

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
        let lock = lock_commitado();

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
