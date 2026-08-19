//! Ligação do checklist fatiado com a UI: estado completo no Rust, PÁGINA no Slint.
//!
//! O quê: guarda o `Vec<OverItem>` inteiro do projeto e mantém o `VecModel` que o
//! Slint renderiza sincronizado com a página/filtro escolhidos. Onde: a aba
//! Overdev (`load_overdev_into` e os callbacks `od-cl-*` do `main.rs`).
//! Efeitos: escreve as propriedades `od-cl-*` e `od-done/open/hold/human-open`
//! do `AppWindow`; nunca lê disco (quem lê é o `parse_checklist_items`).
//!
//! O ponto: `apply()` é a ÚNICA porta que empurra itens pro Slint, e ela empurra
//! no máximo [`checklist::PER_PAGE`] — é o que garante que a UI não cresce com o
//! projeto. Ver o porquê em `checklist.rs`.

use crate::checklist;
use crate::prelude::*;

/// Dono do checklist do projeto atual: tudo no Rust, fatia no Slint.
pub struct ChecklistView {
    /// Checklist COMPLETO do projeto (ordem do arquivo). Nunca vai inteiro pra UI.
    all: RefCell<Vec<OverItem>>,
    /// Modelo que o repeater do Slint lê — só a página corrente.
    model: Rc<VecModel<OverItem>>,
}

impl ChecklistView {
    /// Cria a view em cima do `VecModel` já ligado ao `od-items` do Slint.
    pub fn new(model: Rc<VecModel<OverItem>>) -> Self {
        Self { all: RefCell::new(Vec::new()), model }
    }

    /// Troca o checklist inteiro (recarga do mesmo projeto) e republica a fatia.
    /// MANTÉM a página corrente (presa ao novo total pelo `apply`) — recarregar
    /// depois de marcar um item não pode jogar o usuário de volta pro topo.
    /// Troca de PROJETO chama `reset_view` antes. As contagens das 4 categorias
    /// saem do mesmo `Vec`, numa passada só, sem reler arquivo.
    pub fn set_all(&self, app: &AppWindow, items: Vec<OverItem>) {
        *self.all.borrow_mut() = items;
        self.publish_counts(app);
        self.apply(app);
    }

    /// Zera filtro e página — usado ao TROCAR de projeto (a posição do checklist
    /// anterior não significa nada no novo).
    pub fn reset_view(&self, app: &AppWindow) {
        app.global::<Od>().set_cl_filter(checklist::FILTER_ALL);
        app.global::<Od>().set_cl_page(0);
    }

    /// Esvazia (nenhum projeto / projeto sem overdev).
    pub fn clear(&self, app: &AppWindow) {
        self.all.borrow_mut().clear();
        app.global::<Od>().set_cl_page(0);
        self.publish_counts(app);
        self.apply(app);
    }

    /// Republica a fatia visível a partir do filtro/página atuais das props.
    /// Prende a página ao intervalo válido (filtro pode ter encolhido o total).
    pub fn apply(&self, app: &AppWindow) {
        let all = self.all.borrow();
        let filter = app.global::<Od>().get_cl_filter();
        let total = checklist::filtered_len(&all, filter);
        let page = checklist::clamp_page(app.global::<Od>().get_cl_page(), total);
        let (from, to) = checklist::range_of(total, page);
        app.global::<Od>().set_cl_page(page);
        app.global::<Od>().set_cl_pages(checklist::page_count(total));
        app.global::<Od>().set_cl_total(total as i32);
        app.global::<Od>().set_cl_from(from);
        app.global::<Od>().set_cl_to(to);
        self.model.set_vec(checklist::page_rows(&all, filter, page));
    }

    /// Contagem das 4 categorias do checklist 2-níveis (feitos = máquina + humano),
    /// como o `Counts::done()` do engine do lib. Independe de filtro/página.
    fn publish_counts(&self, app: &AppWindow) {
        let (mut done, mut open, mut hold, mut human) = (0i32, 0i32, 0i32, 0i32);
        for it in self.all.borrow().iter() {
            match it.kind.as_str() {
                "done" | "hdone" => done += 1,
                "open" => open += 1,
                "hold" => hold += 1,
                "hopen" => human += 1,
                _ => {}
            }
        }
        app.global::<Od>().set_done(done);
        app.global::<Od>().set_open(open);
        app.global::<Od>().set_hold(hold);
        app.global::<Od>().set_human_open(human);
    }

    /// `true` quando não há item nenhum — usado pra decidir "projeto sem run".
    pub fn is_empty(&self) -> bool {
        self.all.borrow().is_empty()
    }
}
