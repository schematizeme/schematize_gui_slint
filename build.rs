//! Compila o `ui/app.slint` para código Rust (via `slint::include_modules!()` no main).
//! Onde: build-time. Por quê separado do `slint!` inline: mantém o .slint como
//! arquivo de verdade (destaque de sintaxe, LSP do Slint, diff limpo).

fn main() {
    slint_build::compile("ui/app.slint").expect("falha ao compilar ui/app.slint");
}
