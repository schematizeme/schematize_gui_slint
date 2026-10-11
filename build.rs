//! Compila o `ui/app.slint` para código Rust, e junta os fontes para o guard de i18n.
//!
//! **O `.slint` separado do `slint!` inline** mantém o arquivo de verdade: destaque de
//! sintaxe, LSP do Slint, diff limpo.

use std::io::Write;

fn main() {
    procedencia();
    slint_build::compile("ui/app.slint").expect("falha ao compilar ui/app.slint");
    juntar_fontes();
}

/// **O quê:** colhe o SHA do commit e se a árvore estava suja, para o `--version` dizer de QUAL
/// build ele é.
///
/// **Onde:** `src/procedencia.rs` lê as duas variáveis (`SCHEMATIZE_GUI_SHA` e
/// `SCHEMATIZE_GUI_DIRTY`). Copiado da janela do optimizer (`e0ea6bd`).
///
/// Sem o SHA o número sozinho não distingue dois binários com o mesmo `Cargo.toml` — e foi assim
/// que um binário de 15 dias atrás passou por novo e gravou o `.desktop` errado.
fn procedencia() {
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=SCHEMATIZE_GUI_SHA={sha}");

    // **Árvore suja.** Binário construído com mudança NÃO commitada não é o HEAD: sem esta
    // marca o `--version` mentia o SHA do commit anterior. Só arquivos RASTREADOS contam
    // (`--untracked-files=no`): um rascunho solto na pasta não entra no binário e não deve
    // marcá-lo. Falha do git => "limpo" (sem git já cai em "fonte sem git" pelo SHA vazio).
    let sujo = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| !o.stdout.iter().all(u8::is_ascii_whitespace));
    println!("cargo:rustc-env=SCHEMATIZE_GUI_DIRTY={}", if sujo { "1" } else { "" });
    // Sem isto o valor congela no primeiro build e a versão passa a MENTIR.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    // O veredito "sujo" depende do CONTEÚDO da árvore, que não passa por `.git/HEAD`: editar um
    // arquivo rastreado não mexe em HEAD nem em refs. Por isso vigiamos o que vira binário —
    // `src`, `ui`, `Cargo.*`, este `build.rs` (diretório = mtime recursivo) — e `.git/index`
    // (`git add`/`reset`/`stash`). NÃO vigiamos a árvore inteira (`.`): `target/` muda a cada
    // build e o script rodaria sempre. Caminho inexistente em `rerun-if-changed` conta como
    // "mudou" e re-roda TODO build, então só se emite o que existe.
    for caminho in ["src", "ui", "Cargo.toml", "Cargo.lock", "build.rs", ".git/index"] {
        if std::path::Path::new(caminho).exists() {
            println!("cargo:rerun-if-changed={caminho}");
        }
    }
}

/// **O quê:** concatena todo `src/**/*.rs` num arquivo no `OUT_DIR`.
///
/// **Onde:** o teste `nenhuma_chave_chega_crua_na_tela`, que o lê com `include_str!`.
///
/// ## Por que passa pelo build, e não por uma lista de arquivos no teste
///
/// O guard precisa ver TODO uso de `t(…)`/`tor(…)` do crate. O `include_str!` do Rust não
/// aceita glob, e uma lista escrita à mão apodreceria no primeiro arquivo novo — que é
/// exatamente a falha que o guard existe para pegar.
///
/// **Foi medido:** enquanto ele lia só o `i18nbind.rs`, havia **71 usos em outros arquivos**,
/// e **24 chaves** deles não estavam no catálogo. O guard reprovava sobre o arquivo que já
/// estava certo e era cego para o resto.
///
/// **Por que não um script `.py`, como o de ligações mortas:** a checagem chama `t()`, que só
/// existe compilando contra o crate irmão. Um script teria de reimplementar a leitura do
/// catálogo — segunda lei para o mesmo fato.
fn juntar_fontes() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let arq = std::fs::File::create(out.join("fontes_i18n.txt")).expect("criar fontes_i18n.txt");
    let mut w = std::io::BufWriter::new(arq);
    let mut n = 0usize;
    visitar(std::path::Path::new("src"), &mut |p| {
        let t = std::fs::read_to_string(p).unwrap_or_default();
        // Só o código de PRODUÇÃO de cada arquivo: os testes NOMEIAM chaves para procurá-las,
        // e contá-las faria o guard perseguir a própria sombra.
        let producao = t.split("#[cfg(test)]").next().unwrap_or("").to_string();
        let _ = writeln!(w, "// === {} ===\n{producao}", p.display());
        n += 1;
    });
    assert!(n > 10, "só {n} arquivos varridos — o build está cego");
    println!("cargo:rerun-if-changed=src");
}

/// **O quê:** chama `f` para cada `.rs` sob `dir`, recursivamente.
fn visitar(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entradas: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    // Ordem ESTÁVEL: sem isto o arquivo gerado muda de ordem entre builds, e o `include_str!`
    // faria o teste recompilar sem motivo — e um diff de build vira ruído.
    entradas.sort();
    for p in entradas {
        if p.is_dir() {
            visitar(&p, f);
        } else if p.extension().is_some_and(|e| e == "rs") {
            f(&p);
        }
    }
}
