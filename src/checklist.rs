//! Fatiamento (paginação + filtro) do CHECKLIST do overdev.
//!
//! O quê: o estado COMPLETO do checklist vive no Rust (`Vec<OverItem>`); o
//! `VecModel` que o Slint renderiza recebe só uma PÁGINA. Onde: consumido pela
//! aba Overdev (`load_overdev_into` / `apply_checklist_page` no `main.rs`).
//!
//! Por quê: o repeater do Slint (`for it in root.od-items`) NÃO é virtualizado —
//! ele instancia um sub-árvore de ~8 elementos por item, com `Text` em
//! `word-wrap` (layout caro). Com 500+ itens isso vira ~4.000 elementos criados
//! e re-medidos a cada layout, e o event loop trava. Fatiar em páginas mantém o
//! número de elementos CONSTANTE (<= `PER_PAGE`), independente do tamanho do
//! projeto — que é o piso de "software de massa" da casa: o app não pode ficar
//! mais lento porque o projeto cresceu.
//!
//! Tudo aqui é PURO (sem I/O, sem tocar na UI) — por isso é testável em unidade.

use crate::OverItem;

/// Itens por página. Escolhido pra caber com folga no orçamento de layout do
/// Slint (~800 elementos) mesmo na página mais densa, e ainda dar uma leitura
/// longa o suficiente pra rolar sem paginar toda hora.
pub const PER_PAGE: usize = 100;

/// Filtro ativo do checklist. O `i32` é o que cruza a fronteira pro Slint
/// (não há enum na fronteira), então a conversão mora aqui e em lugar nenhum mais.
/// 0 = todos · 1 = abertos (máquina) · 2 = feitos · 3 = on-hold · 4 = humanos abertos.
pub const FILTER_ALL: i32 = 0;
pub const FILTER_OPEN: i32 = 1;
pub const FILTER_DONE: i32 = 2;
pub const FILTER_HOLD: i32 = 3;
pub const FILTER_HUMAN: i32 = 4;

/// Um item do checklist casa com o filtro?
/// `kind` é o mesmo vocabulário do parser: `open`/`done`/`hold`/`hopen`/`hdone`.
/// Filtro desconhecido cai em "todos" (nunca esconde item por engano).
pub fn matches(kind: &str, filter: i32) -> bool {
    match filter {
        FILTER_OPEN => kind == "open",
        // "Feitos" inclui respondido: a pendência humana acabou. NÃO inclui recusado —
        // recusado é resolvido e não feito, e misturar mentiria no contador da tela.
        FILTER_DONE => kind == "done" || kind == "hdone" || kind == "hresp",
        // On-hold é o que está TRAVADO esperando decisão. A nota da resposta viaja
        // junto do item pai em qualquer filtro, senão "respondido" apareceria sem
        // dizer o que foi respondido.
        FILTER_HOLD => kind == "hold",
        FILTER_HUMAN => kind == "hopen",
        _ => true,
    }
}

/// Quantos itens sobram sob o filtro (o total da paginação).
pub fn filtered_len(all: &[OverItem], filter: i32) -> usize {
    if filter == FILTER_ALL {
        return all.len();
    }
    all.iter().filter(|it| matches(it.kind.as_str(), filter)).count()
}

/// Número de páginas de `total` itens (sempre >= 1, pra a UI nunca mostrar "0 de 0").
pub fn page_count(total: usize) -> i32 {
    if total == 0 {
        1
    } else {
        total.div_ceil(PER_PAGE) as i32
    }
}

/// Prende `page` ao intervalo válido `[0, page_count-1]` — a UI pode pedir uma
/// página que sumiu depois de trocar o filtro/recarregar o projeto.
pub fn clamp_page(page: i32, total: usize) -> i32 {
    page.clamp(0, page_count(total) - 1)
}

/// A FATIA que vai pro `VecModel`: os itens que casam com `filter`, só os da
/// página `page` (0-based). Clona só o que vai pra tela — nunca o checklist todo.
pub fn page_rows(all: &[OverItem], filter: i32, page: i32) -> Vec<OverItem> {
    let skip = (clamp_page(page, filtered_len(all, filter)) as usize) * PER_PAGE;
    all.iter()
        .filter(|it| matches(it.kind.as_str(), filter))
        .skip(skip)
        .take(PER_PAGE)
        .cloned()
        .collect()
}

/// Intervalo 1-based exibido no rodapé ("`from`–`to` de `total`").
/// Total zero → `(0, 0)`, pra a UI escrever "0–0 de 0" sem caso especial.
pub fn range_of(total: usize, page: i32) -> (i32, i32) {
    if total == 0 {
        return (0, 0);
    }
    let from = (clamp_page(page, total) as usize) * PER_PAGE;
    let to = (from + PER_PAGE).min(total);
    (from as i32 + 1, to as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constrói `n` itens alternando os 5 estados do checklist 2-níveis.
    fn sample(n: usize) -> Vec<OverItem> {
        let kinds = ["done", "open", "hold", "hopen", "hdone"];
        (0..n)
            .map(|i| OverItem {
                kind: kinds[i % 5].into(),
                text: format!("item {i}").into(),
                machine: i % 5 < 3,
                hindex: -1,
                sub: false,
            })
            .collect()
    }

    /// Respondido conta como feito; recusado NÃO entra em nenhum filtro de trabalho —
    /// é resolvido sem ter sido feito, e somá-lo em "feitos" inflaria o progresso.
    #[test]
    fn respondido_conta_como_feito_recusado_nao() {
        assert!(matches("hresp", FILTER_DONE));
        assert!(matches("hdone", FILTER_DONE));
        assert!(!matches("hrec", FILTER_DONE));
        assert!(!matches("cancel", FILTER_DONE));
        assert!(!matches("hrec", FILTER_OPEN));
        assert!(!matches("cancel", FILTER_OPEN));
        assert!(!matches("hrec", FILTER_HUMAN), "recusado não é pendência humana");
        // Mas aparecem no "tudo" — sumir com eles esconderia o que foi decidido.
        assert!(matches("hrec", FILTER_ALL));
        assert!(matches("cancel", FILTER_ALL));
    }

    #[test]
    fn pagina_limita_o_que_vai_pra_tela() {
        let all = sample(700);
        assert_eq!(page_rows(&all, FILTER_ALL, 0).len(), PER_PAGE);
        assert_eq!(page_count(700), 7);
        // última página: 700 = 7 * 100, cheia.
        assert_eq!(page_rows(&all, FILTER_ALL, 6).len(), PER_PAGE);
        // página além do fim é presa na última (nunca vazia por engano).
        assert_eq!(page_rows(&all, FILTER_ALL, 99).len(), PER_PAGE);
    }

    #[test]
    fn filtro_reduz_total_e_paginas() {
        let all = sample(700); // 140 de cada estado
        assert_eq!(filtered_len(&all, FILTER_OPEN), 140);
        assert_eq!(page_count(filtered_len(&all, FILTER_OPEN)), 2);
        let p0 = page_rows(&all, FILTER_OPEN, 0);
        assert_eq!(p0.len(), 100);
        assert!(p0.iter().all(|it| it.kind == "open"));
        assert_eq!(page_rows(&all, FILTER_OPEN, 1).len(), 40);
    }

    #[test]
    fn feitos_juntam_maquina_e_humano() {
        let all = sample(10); // 2 done + 2 hdone
        assert_eq!(filtered_len(&all, FILTER_DONE), 4);
    }

    #[test]
    fn intervalo_1_based_e_vazio_sem_caso_especial() {
        assert_eq!(range_of(0, 0), (0, 0));
        assert_eq!(range_of(700, 0), (1, 100));
        assert_eq!(range_of(700, 6), (601, 700));
        assert_eq!(range_of(140, 1), (101, 140));
    }

    #[test]
    fn lista_curta_cabe_numa_pagina_so() {
        let all = sample(12);
        assert_eq!(page_count(12), 1);
        assert_eq!(page_rows(&all, FILTER_ALL, 0).len(), 12);
        assert_eq!(range_of(12, 0), (1, 12));
    }
}
