//! Linhas da tela Disco — agregação e PAGINAÇÃO dos achados.
//!
//! O quê: transforma `disco::Achado`/`docker::Categoria` nas structs da fronteira
//! (`DiscoTotal`, `DiscoRow`, `DockerRow`) e fatia a lista. Onde: usado por
//! `wire::disco`. Funções puras (menos `docker_rows`, que fala com o docker).
//!
//! Por que paginar: repeater do Slint NÃO é virtualizado — mandar 400 achados pra
//! a UI custa 400 sub-árvores de layout por quadro. É o mesmo defeito que travava
//! o checklist do overdev, e a correção é a mesma: o Rust é dono da lista inteira
//! e a UI só recebe a página. Ver `checklist.rs`.

use crate::prelude::*;
use schematize::disco::{docker, tamanho::legivel, Achado};

/// Achados por página. Cabem folgados numa tela e o custo de render fica constante.
pub(crate) const PER_PAGE: usize = 40;

/// Corte de ruído da varredura: abaixo disto não vale nem listar (o mesmo do CLI —
/// um `__pycache__` de 40 KB não é o que enche o disco).
pub(crate) const MINIMO: u64 = 50 * 1024 * 1024;

/// Índices (na lista COMPLETA) dos achados que passam no filtro de tempo parado.
///
/// Devolve índices, não cópias: são eles que viajam pra a UI como `DiscoRow.idx` e
/// voltam como argumento de "apagar este". O índice do repeater não serviria — ele
/// é relativo à página.
pub(crate) fn filtrados(achados: &[Achado], min_dias: u64) -> Vec<usize> {
    achados.iter().enumerate().filter(|(_, a)| a.dias_parado >= min_dias).map(|(i, _)| i).collect()
}

/// A página `page` dos achados filtrados, já como linhas da UI.
pub(crate) fn page_rows(achados: &[Achado], idxs: &[usize], page: i32) -> Vec<DiscoRow> {
    let start = (page.max(0) as usize) * PER_PAGE;
    idxs.iter()
        .skip(start)
        .take(PER_PAGE)
        .filter_map(|&i| achados.get(i).map(|a| (i, a)))
        .map(|(i, a)| DiscoRow {
            idx: i as i32,
            path: a.caminho.display().to_string().into(),
            kind: a.tipo.rotulo().into(),
            size: legivel(a.bytes).into(),
            days: dias_label(a.dias_parado).into(),
            mount: a.montagem.display().to_string().into(),
            refaz: a.refaz.into(),
            net: a.tipo.custa_rede(),
            op_label: SharedString::new(),
            op_error: false,
        })
        .collect()
}

/// "hoje" / "1 dia parado" / "49 dias parado". É o que separa build de ontem de lixo.
pub(crate) fn dias_label(d: u64) -> String {
    match d {
        0 => tor("gui.disk_today", "mexido hoje"),
        1 => tor("gui.disk_one_day", "1 dia parado"),
        n => format!("{n} {}", tor("gui.disk_days", "dias parado")),
    }
}

/// Totais POR DISCO — a visão que responde "o que está enchendo ESTE disco".
pub(crate) fn totais_por_montagem(achados: &[Achado]) -> Vec<DiscoTotal> {
    let v = schematize::disco::por_montagem(achados);
    let maior = v.first().map(|(_, b)| *b).unwrap_or(0);
    v.into_iter()
        .map(|(m, b)| DiscoTotal {
            label: m.display().to_string().into(),
            size: legivel(b).into(),
            note: SharedString::new(),
            frac: fracao(b, maior),
        })
        .collect()
}

/// Totais POR TIPO — quem é o culpado, e o que custa refazer (CPU vs rede).
pub(crate) fn totais_por_tipo(achados: &[Achado]) -> Vec<DiscoTotal> {
    let v = schematize::disco::por_tipo(achados);
    let maior = v.first().map(|(_, b)| *b).unwrap_or(0);
    v.into_iter()
        .map(|(t, b)| DiscoTotal {
            label: t.rotulo().into(),
            size: legivel(b).into(),
            note: if t.custa_rede() {
                tor("gui.disk_redownload", "baixa de novo").into()
            } else {
                tor("gui.disk_recompile", "compila de novo").into()
            },
            frac: fracao(b, maior),
        })
        .collect()
}

/// Fração do maior total, pra a barra. Sem maior (lista vazia) → 0.
fn fracao(b: u64, maior: u64) -> f32 {
    if maior == 0 {
        0.0
    } else {
        (b as f64 / maior as f64) as f32
    }
}

/// Linhas do Docker: uma por categoria do `system df`, mais as podas oferecidas.
///
/// Casa cada poda com a categoria que ela libera, pra o botão ficar ao lado do
/// número que ele muda. A poda de VOLUMES entra marcada `destructive` — volume é
/// dado, não build, e a UI trata isso à parte.
pub(crate) fn docker_rows() -> Vec<DockerRow> {
    let uso = docker::uso();
    if uso.is_empty() {
        return Vec::new();
    }
    uso.into_iter()
        .map(|c| {
            let (prune, destr) = poda_de(&c.tipo);
            DockerRow {
                label: c.tipo.into(),
                size: legivel(c.bytes).into(),
                reclaim: legivel(c.recuperavel).into(),
                prune: prune.into(),
                destructive: destr,
                op_label: SharedString::new(),
                op_error: false,
            }
        })
        .collect()
}

/// Qual poda libera a categoria do `docker system df`? PURA — o casamento é por
/// nome e é aqui que ele fica visível (o docker traduz/renomeia essas linhas às
/// vezes; quando não casar, a linha vira só informativa, nunca um botão errado).
pub(crate) fn poda_de(tipo: &str) -> (&'static str, bool) {
    let t = tipo.to_ascii_lowercase();
    for (rotulo, _, destrutivo) in docker::podas() {
        let casa = match rotulo {
            "cache de build" => t.contains("build cache"),
            "containers parados" => t.contains("container"),
            "imagens sem uso" => t.contains("image"),
            "redes sem uso" => t.contains("network"),
            r if r.starts_with("volumes") => t.contains("volume"),
            _ => false,
        };
        if casa {
            return (rotulo, destrutivo);
        }
    }
    ("", false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use schematize::disco::Tipo;

    fn achado(dias: u64, bytes: u64) -> Achado {
        Achado {
            caminho: PathBuf::from(format!("/p/{dias}/target")),
            tipo: Tipo::RustTarget,
            bytes,
            dias_parado: dias,
            montagem: PathBuf::from("/"),
            refaz: "cargo build",
        }
    }

    /// O filtro devolve ÍNDICES da lista completa — é o que a ação de apagar usa.
    /// Se devolvesse posições da página, apagaríamos o item errado ao paginar.
    #[test]
    fn filtro_preserva_o_indice_original() {
        let v = vec![achado(0, 10), achado(40, 10), achado(90, 10)];
        let idxs = filtrados(&v, 30);
        assert_eq!(idxs, vec![1, 2]);
        let rows = page_rows(&v, &idxs, 0);
        assert_eq!(rows[0].idx, 1, "a 1ª linha da página aponta pro achado 1");
        assert_eq!(rows[1].idx, 2);
    }

    /// A UI nunca recebe mais que uma página, por mais achados que existam.
    #[test]
    fn pagina_nunca_estoura() {
        let v: Vec<Achado> = (0..500).map(|i| achado(i, 1024)).collect();
        let idxs = filtrados(&v, 0);
        assert_eq!(idxs.len(), 500);
        assert_eq!(page_rows(&v, &idxs, 0).len(), PER_PAGE);
        let ultima = page_rows(&v, &idxs, (500 / PER_PAGE) as i32);
        assert!(ultima.len() <= PER_PAGE);
        // página além do fim não estoura: devolve vazio.
        assert!(page_rows(&v, &idxs, 999).is_empty());
    }

    /// A barra é proporcional ao MAIOR total — o primeiro item sempre cheio.
    #[test]
    fn barra_e_proporcional() {
        let mut a = achado(0, 100);
        a.montagem = PathBuf::from("/");
        let mut b = achado(0, 50);
        b.montagem = PathBuf::from("/dados");
        let t = totais_por_montagem(&[a, b]);
        assert_eq!(t[0].frac, 1.0);
        assert!((t[1].frac - 0.5).abs() < 0.001);
    }

    /// O casamento poda↔categoria é por nome; volume vem marcado como destrutivo.
    #[test]
    fn poda_casa_e_marca_volume() {
        assert_eq!(poda_de("Build Cache").0, "cache de build");
        assert_eq!(poda_de("Images").0, "imagens sem uso");
        assert!(!poda_de("Images").1, "imagem se refaz com pull");
        let (rotulo, destr) = poda_de("Local Volumes");
        assert!(rotulo.starts_with("volumes"));
        assert!(destr, "volume é DADO — tem de vir marcado");
        assert_eq!(poda_de("Coisa Nova Do Docker").0, "", "sem casar = sem botão");
    }
}
