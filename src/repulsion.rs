//! Repulsão do grafo em GRADE ESPACIAL — o passo que era O(n²) por tick.
//!
//! O quê: dada a posição de cada nó, devolve a força de repulsão que age sobre
//! ele. Onde: consumido pelo `GraphState::step` (aba Grafo e grafo do schema do
//! Database builder). Puro: sem I/O, sem UI — por isso é testável em unidade.
//!
//! Por quê a grade: a versão anterior comparava TODO par de nós a cada tick, a
//! ~60 fps, no event loop. Com o `GRAFO_GLOBAL.md` de um projeto de verdade
//! (centenas de nós) isso é meio milhão de pares por quadro — o app inteiro
//! congelava, inclusive nas abas que nem desenham grafo.
//!
//! A força é `REP/d²`: a 3 células de distância ela já é ~1% da força de contato,
//! ou seja, o corte é aproximação, não mudança de comportamento. Cada nó só olha
//! a própria célula e as 8 vizinhas, então o custo vira O(n · densidade) em vez
//! de O(n²). Nós MUITO distantes seguem sendo puxados pelo termo de gravidade do
//! `step` (que continua global e é O(n)), então o grafo não se desfaz.

use std::collections::HashMap;

/// Raio de corte da repulsão, em unidades de mundo (é também o lado da célula).
/// Casado com o comprimento de mola do layout (`LEN = 70`): ~3x a distância de
/// repouso de uma aresta, onde `REP/d²` já é desprezível.
pub const CUTOFF: f32 = 220.0;

/// Índice da célula de um ponto na grade uniforme de lado [`CUTOFF`].
fn cell_of(x: f32, y: f32) -> (i32, i32) {
    ((x / CUTOFF).floor() as i32, (y / CUTOFF).floor() as i32)
}

/// Força de repulsão sobre cada nó, na mesma ordem de `xs`/`ys`.
///
/// `strength` é a constante `REP` do modelo (força = `strength / d²`, na direção
/// que afasta). `xs` e `ys` têm de ter o mesmo tamanho; sobra é ignorada.
/// Devolve um vetor de `(fx, fy)` do tamanho de `xs`.
pub fn forces(xs: &[f32], ys: &[f32], strength: f32) -> Vec<(f32, f32)> {
    let n = xs.len().min(ys.len());
    let mut out = vec![(0.0f32, 0.0f32); n];
    if n < 2 {
        return out;
    }
    // 1) indexa os nós por célula (uma passada).
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::with_capacity(n);
    for i in 0..n {
        grid.entry(cell_of(xs[i], ys[i])).or_default().push(i);
    }
    // 2) cada nó só interage com a própria célula e as 8 vizinhas.
    for i in 0..n {
        let (cx, cy) = cell_of(xs[i], ys[i]);
        let (mut fx, mut fy) = (0.0f32, 0.0f32);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(bucket) = grid.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &j in bucket {
                    if i == j {
                        continue;
                    }
                    let ddx = xs[i] - xs[j];
                    let ddy = ys[i] - ys[j];
                    let d2 = ddx * ddx + ddy * ddy + 0.01;
                    if d2 > CUTOFF * CUTOFF {
                        continue; // dentro da célula vizinha, mas fora do raio
                    }
                    let d = d2.sqrt();
                    let f = strength / d2;
                    fx += f * ddx / d;
                    fy += f * ddy / d;
                }
            }
        }
        out[i] = (fx, fy);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dois nós próximos se empurram em sentidos opostos, com a mesma intensidade.
    #[test]
    fn dois_nos_proximos_se_repelem_simetricamente() {
        let f = forces(&[0.0, 10.0], &[0.0, 0.0], 1400.0);
        assert!(f[0].0 < 0.0, "o da esquerda é empurrado pra esquerda");
        assert!(f[1].0 > 0.0, "o da direita é empurrado pra direita");
        assert!((f[0].0 + f[1].0).abs() < 1e-3, "3ª lei: soma das forças ~ 0");
        assert!(f[0].1.abs() < 1e-6, "sem componente vertical");
    }

    /// Além do corte a força é exatamente zero (é o que dá a complexidade linear).
    #[test]
    fn alem_do_corte_nao_ha_forca() {
        let far = CUTOFF * 4.0;
        let f = forces(&[0.0, far], &[0.0, 0.0], 1400.0);
        assert_eq!(f[0], (0.0, 0.0));
        assert_eq!(f[1], (0.0, 0.0));
    }

    /// O corte é por RAIO, não por célula: um vizinho na célula ao lado mas fora
    /// do raio não conta (senão a força dependeria do alinhamento da grade).
    #[test]
    fn vizinho_de_celula_fora_do_raio_nao_conta() {
        let d = CUTOFF * 1.5; // cai na célula vizinha, mas além do raio
        let f = forces(&[0.0, d], &[0.0, 0.0], 1400.0);
        assert_eq!(f[0], (0.0, 0.0));
    }

    /// Bate com a fórmula direta `REP/d²` quando os nós estão dentro do raio —
    /// a grade é só indexação, não muda o modelo.
    #[test]
    fn dentro_do_raio_bate_com_a_formula_direta() {
        let d = 50.0f32;
        let f = forces(&[0.0, d], &[0.0, 0.0], 1400.0);
        let d2 = d * d + 0.01;
        let esperado = 1400.0 / d2;
        assert!((f[1].0 - esperado).abs() < 1e-3, "f={} esperado={}", f[1].0, esperado);
    }

    /// Casos degenerados não entram em pânico nem devolvem tamanho errado.
    #[test]
    fn zero_e_um_no_nao_quebram() {
        assert!(forces(&[], &[], 1.0).is_empty());
        assert_eq!(forces(&[1.0], &[2.0], 1.0), vec![(0.0, 0.0)]);
    }

    /// Muitos nós no MESMO ponto: a grade degenera pra uma célula só, mas o
    /// resultado tem de sair (sem NaN) — o `+0.01` no d² é quem garante isso.
    #[test]
    fn nos_coincidentes_nao_geram_nan() {
        let xs = vec![0.0f32; 50];
        let ys = vec![0.0f32; 50];
        let f = forces(&xs, &ys, 1400.0);
        assert!(f.iter().all(|(a, b)| a.is_finite() && b.is_finite()));
    }
}
