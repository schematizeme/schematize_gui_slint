//! Posição inicial dos nós do grafo (espiral de ângulo áureo).
//!
//! O quê: dado o índice do nó, devolve `(x, y)` de partida da simulação. Onde:
//! consumido pelas três cargas de grafo da GUI (global, drill de serviço e o
//! grafo do schema do Database builder). Puro e testável.
//!
//! Por quê o passo radial é grande: a repulsão agora é cortada por raio
//! (`repulsion::CUTOFF`); se todos os nós nascem empilhados num punhado de
//! células, a grade não separa nada no 1º quadro e o custo volta a ser O(n²)
//! justo no momento mais pesado (carga do projeto). Nascer espalhado na ordem
//! do raio de corte é o que mantém o primeiro quadro barato.

use crate::repulsion::CUTOFF;

/// Ângulo áureo em radianos — o passo que evita nós alinhados em raios.
const GOLDEN_ANGLE: f32 = 2.399_963;

/// Raio do 1º nó (dá um miolo respirável em grafos pequenos).
const R0: f32 = 40.0;

/// Passo radial da espiral, como fração do raio de corte da repulsão.
///
/// Com raio `R0 + K·sqrt(n)` a densidade inicial é `(CUTOFF/K)²/π` nós por célula
/// da grade — INDEPENDENTE de `n`, então uma constante resolve pra qualquer
/// tamanho de grafo. Em 0.36 dá ~2,5 nós por célula; era 0.04 (o `9.0` original),
/// que empilhava ~13 por célula e fazia a grade não separar nada no 1º quadro.
const K: f32 = CUTOFF * 0.36;

/// Posição inicial do `i`-ésimo nó (0-based) em coordenadas de mundo.
/// O raio cresce com `sqrt(i)` (área por nó constante), com o passo [`K`].
pub fn position(i: usize) -> (f32, f32) {
    let a = i as f32 * GOLDEN_ANGLE;
    let r = R0 + K * (i as f32).sqrt();
    (a.cos() * r, a.sin() * r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O 1º nó fica no raio de partida, e os seguintes só se afastam.
    #[test]
    fn raio_cresce_monotonicamente() {
        let raio = |i: usize| {
            let (x, y) = position(i);
            (x * x + y * y).sqrt()
        };
        assert!((raio(0) - R0).abs() < 1e-3);
        for i in 1..500 {
            assert!(raio(i) > raio(i - 1), "raio caiu em i={i}");
        }
    }

    /// A densidade inicial fica na ordem de 1 nó por célula da grade da repulsão —
    /// é essa propriedade que segura o custo do 1º quadro.
    #[test]
    fn densidade_inicial_perto_de_um_no_por_celula() {
        let n = 700;
        let raio_max = {
            let (x, y) = position(n - 1);
            (x * x + y * y).sqrt()
        };
        let celulas = (std::f32::consts::PI * raio_max * raio_max) / (CUTOFF * CUTOFF);
        let por_celula = n as f32 / celulas;
        assert!(por_celula < 3.0, "{por_celula:.2} nós por célula — grade não separa");
    }
}
