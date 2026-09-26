//! Compila o `ui/app.slint` para código Rust, e junta os fontes para o guard de i18n.
//!
//! **O `.slint` separado do `slint!` inline** mantém o arquivo de verdade: destaque de
//! sintaxe, LSP do Slint, diff limpo.

use std::io::Write;

fn main() {
    slint_build::compile("ui/app.slint").expect("falha ao compilar ui/app.slint");
    juntar_fontes();
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
