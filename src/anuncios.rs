//! Anúncios da casa na GUI (ADR-0028): liga o catálogo do crate `schematize_connectors` à tela.
//!
//! O quê: escolhe o anúncio por slot (tela inicial e estado vazio do Overdev), no idioma da
//! GUI, e o entrega à UI já pronto (rótulo, texto, link, botão "ocultar"). Toda a decisão é do
//! `connectors::escolher` (PURA e determinística); aqui só se monta o [`Contexto`] e se faz a
//! ponte com o Slint. Onde: `main()` chama [`wire`] uma vez; a troca de idioma chama [`aplicar`].
//!
//! Regras do ADR respeitadas:
//! - rótulo "Anúncio/Ad" obrigatório (vem de `rotulo()`; a UI o desenha sempre);
//! - nunca anúncio de conector que o usuário já usa (ver [`conectores_ativos`]);
//! - respeita o opt-out (env `SCHEMATIZE_SEM_ANUNCIOS` OU arquivo `sem-anuncios`);
//! - 1 anúncio por sessão: o slot do estado vazio reusa a MESMA escolha da tela inicial;
//! - nunca vai a prompt/contexto de agente: este módulo só escreve em propriedades da janela;
//! - sem imagem (o campo `imagem` do anúncio é ignorado) e sem rede: o link só abre quando
//!   a PESSOA clica, pelo mecanismo existente da GUI (`util::open_url`), após revalidar.

use crate::prelude::*;
use schematize_connectors::cli::amb::{hoje_utc, optout_de_config};
use schematize_connectors::{
    anuncios, buscar, escolher, optout_de_env, rotulo, Anuncio, Auth, Contexto, Data, Idioma, Slot,
};
use std::path::PathBuf as Caminho;

/// Leitor de ambiente injetável (testes passam um falso).
type LeEnv<'a> = &'a dyn Fn(&str) -> Option<String>;

/// O quê: `Idioma` do crate a partir do código de locale da GUI (`pt`, `pt-BR`, `en`, ...);
/// tudo que não é português cai em inglês (o catálogo só tem pt/en).
/// Onde: [`aplicar`], com `i18n::current_code()`.
pub(crate) fn idioma_de(code: &str) -> Idioma {
    if code.trim().to_lowercase().starts_with("pt") {
        Idioma::Pt
    } else {
        Idioma::En
    }
}

/// O quê: texto do botão "não mostrar mais" no idioma. Onde: [`item_de`].
fn texto_ocultar(idioma: Idioma) -> &'static str {
    match idioma {
        Idioma::Pt => "Ocultar · não mostrar mais",
        Idioma::En => "Hide · don't show again",
    }
}

/// O quê: caminho do arquivo de opt-out `<config>/schematize/connectors/sem-anuncios`
/// (config = `XDG_CONFIG_HOME` ou `$HOME/.config`) — o MESMO que a CLI do connectors lê.
/// Onde: [`gravar_optout`].
pub(crate) fn caminho_optout(get: LeEnv<'_>) -> Option<Caminho> {
    let base = match get("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(x) => Caminho::from(x),
        None => Caminho::from(get("HOME").filter(|v| !v.is_empty())?).join(".config"),
    };
    Some(base.join("schematize").join("connectors").join("sem-anuncios"))
}

/// O quê: o usuário desligou anúncios? (env OU arquivo). Onde: [`decidir`].
pub(crate) fn optout_ativo(get: LeEnv<'_>) -> bool {
    optout_de_env(|k| get(k)) || optout_de_config(get)
}

/// O quê: grava o opt-out de arquivo (cria os diretórios; arquivo vazio basta).
/// Onde: botão "ocultar" ([`wire`]). Erro volta ao chamador — nunca é engolido.
pub(crate) fn gravar_optout(get: LeEnv<'_>) -> Result<Caminho, String> {
    let alvo = caminho_optout(get).ok_or("sem HOME/XDG_CONFIG_HOME para gravar o opt-out")?;
    if let Some(dir) = alvo.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&alvo, b"").map_err(|e| format!("{}: {e}", alvo.display()))?;
    Ok(alvo)
}

/// O quê: ids dos conectores que o usuário JÁ usa. Heurística: o conector com auth por chave
/// cuja variável de ambiente está definida e não vazia (ex.: `ASENDER_API_KEY`).
/// Onde: [`decidir`], para a regra "nunca anunciar conector ativo".
/// Limite conhecido: não lê a config do Claude Code (ver relatório; pendência do dono).
pub(crate) fn conectores_ativos(get: LeEnv<'_>) -> Vec<&'static str> {
    schematize_connectors::catalogo()
        .iter()
        .filter(|c| match c.auth {
            Auth::BearerChave { env_var, .. } => get(env_var).is_some_and(|v| !v.trim().is_empty()),
            _ => false,
        })
        .map(|c| c.id)
        .collect()
}

/// Semente de rotação: dias desde 1970 (muda por dia, sem estado persistido). Onde: [`aplicar`].
fn sessao_do_dia() -> u32 {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (s / 86_400) as u32
}

/// O quê: decide o anúncio de um slot. PURA: tudo é injetado (idioma, env, hoje, semente).
/// A GUI é interativa e humana: `tty = true`, `modo_json = false`.
/// Onde: [`aplicar`] (uma chamada por slot) e os testes.
pub(crate) fn decidir(
    slot: Slot,
    idioma: Idioma,
    get: LeEnv<'_>,
    hoje: Data,
    sessao_n: u32,
) -> Option<&'static Anuncio> {
    let ativos = conectores_ativos(get);
    escolher(&Contexto {
        slot,
        idioma,
        tty: true,
        modo_json: false,
        optout: optout_ativo(get),
        conectores_ativos: &ativos,
        sessao_n,
        exibidos_na_sessao: 0,
        agora: hoje,
    })
}

/// O quê: monta o item da UI (texto sem caracteres de controle, rótulo obrigatório); `None`
/// vira um item invisível. Onde: [`aplicar`].
fn item_de(a: Option<&'static Anuncio>, idioma: Idioma) -> AdItem {
    let Some(a) = a else { return AdItem::default() };
    let texto = match idioma {
        Idioma::Pt => a.texto_pt,
        Idioma::En => a.texto_en,
    };
    let limpo = |s: &str| -> SharedString {
        s.chars().filter(|c| !c.is_control()).collect::<String>().into()
    };
    AdItem {
        visible: true,
        id: a.id.into(),
        label: limpo(rotulo(idioma)),
        text: limpo(texto),
        link: limpo(a.link),
        hide: texto_ocultar(idioma).into(),
    }
}

/// O quê: (re)calcula os dois slots e escreve no global `Ad` da janela. A escolha de cada slot
/// usa a mesma semente, então o estado vazio mostra o MESMO anúncio da tela inicial (1 por sessão).
/// Onde: [`wire`] no arranque e a troca de idioma (`wire::settings`).
pub(crate) fn aplicar(app: &AppWindow) {
    let idioma = idioma_de(&i18n::current_code());
    let get = |k: &str| std::env::var(k).ok();
    let (hoje, n) = (hoje_utc(), sessao_do_dia());
    let ad = app.global::<Ad>();
    ad.set_home(item_de(decidir(Slot::TelaInicial, idioma, &get, hoje, n), idioma));
    ad.set_vazio(item_de(decidir(Slot::EstadoVazio, idioma, &get, hoje, n), idioma));
}

/// O quê: URL revalidada de um anúncio pelo id (existe e `validar()` ok = https do domínio do
/// conector). Onde: clique no link ([`wire`]); a UI nunca passa URL, só o id.
pub(crate) fn link_do_anuncio(id: &str) -> Option<&'static str> {
    let a = anuncios().iter().find(|a| a.id == id)?;
    (a.validar().is_ok() && buscar(a.conector_id).is_some()).then_some(a.link)
}

/// O quê: liga os callbacks `Ad.abrir`/`Ad.ocultar` e mostra os anúncios iniciais.
/// Onde: `main()`, depois de criar a janela (nenhuma rede é feita aqui).
pub(crate) fn wire(app: &AppWindow) {
    aplicar(app);
    app.global::<Ad>().on_abrir(|id| match link_do_anuncio(&id) {
        Some(url) => util::open_url(url),
        None => eprintln!("anúncio: id/link inválido, nada aberto"),
    });
    let weak = app.as_weak();
    app.global::<Ad>().on_ocultar(move || {
        if let Err(e) = gravar_optout(&|k| std::env::var(k).ok()) {
            // Não grava => some só nesta sessão; o erro NÃO é engolido.
            eprintln!("anúncio: não consegui gravar o opt-out: {e}");
        }
        if let Some(app) = weak.upgrade() {
            app.global::<Ad>().set_home(AdItem::default());
            app.global::<Ad>().set_vazio(AdItem::default());
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    const HOJE: Data = Data { ano: 2026, mes: 11, dia: 21 };

    /// Ambiente falso: só as chaves dadas.
    fn env(pares: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let v: Vec<(String, String)> =
            pares.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| v.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone())
    }

    /// Diretório temporário único (sem crate extra). `std::env::temp_dir()` respeita TMPDIR.
    fn tmp(nome: &str) -> Caminho {
        let d = std::env::temp_dir().join(format!("gui-anuncios-{}-{nome}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn idioma_segue_o_codigo_da_gui() {
        assert_eq!(idioma_de("pt"), Idioma::Pt);
        assert_eq!(idioma_de("pt-BR"), Idioma::Pt);
        assert_eq!(idioma_de("en"), Idioma::En);
        assert_eq!(idioma_de("ja"), Idioma::En);
    }

    #[test]
    fn escolhe_algo_nos_dois_slots_e_o_mesmo_anuncio() {
        let g = env(&[]);
        for n in 0..6 {
            let h = decidir(Slot::TelaInicial, Idioma::Pt, &g, HOJE, n).expect("home");
            let v = decidir(Slot::EstadoVazio, Idioma::Pt, &g, HOJE, n).expect("vazio");
            assert_eq!(h.id, v.id, "1 por sessão: mesmo anúncio nos dois slots");
        }
    }

    #[test]
    fn item_tem_rotulo_obrigatorio_e_idioma() {
        let g = env(&[]);
        let a = decidir(Slot::TelaInicial, Idioma::En, &g, HOJE, 1);
        let i = item_de(a, Idioma::En);
        assert!(i.visible && i.label.starts_with("Ad"), "rótulo en: {}", i.label);
        let p = item_de(a, Idioma::Pt);
        assert!(p.label.starts_with("Anúncio"));
        assert!(!item_de(None, Idioma::Pt).visible, "sem anúncio = invisível (colapsa)");
    }

    #[test]
    fn copy_modelo_aparece_como_texto_sem_imagem() {
        let g = env(&[]);
        // sessão 0 = conector principal (nebularws), cuja copy é "[modelo ...]".
        let a = decidir(Slot::TelaInicial, Idioma::Pt, &g, HOJE, 0).unwrap();
        assert!(item_de(Some(a), Idioma::Pt).text.contains("[modelo — aguardando copy do dono]"));
    }

    #[test]
    fn conector_ativo_nunca_e_anunciado() {
        assert!(conectores_ativos(&env(&[])).is_empty());
        let g = env(&[("ASENDER_API_KEY", "sk_x")]);
        assert_eq!(conectores_ativos(&g), vec!["asender"]);
        for n in 0..40 {
            for s in [Slot::TelaInicial, Slot::EstadoVazio] {
                let a = decidir(s, Idioma::Pt, &g, HOJE, n).unwrap();
                assert_ne!(a.conector_id, "asender");
            }
        }
        // chave vazia não conta como ativo
        assert!(conectores_ativos(&env(&[("ASENDER_API_KEY", " ")])).is_empty());
    }

    #[test]
    fn optout_de_env_some_o_anuncio() {
        let g = env(&[("SCHEMATIZE_SEM_ANUNCIOS", "1")]);
        assert!(optout_ativo(&g));
        assert!(decidir(Slot::TelaInicial, Idioma::Pt, &g, HOJE, 0).is_none());
    }

    /// Prova ponta a ponta: com anúncio, grava o opt-out de arquivo, e o anúncio some.
    #[test]
    fn optout_de_arquivo_some_o_anuncio() {
        let dir = tmp("optout");
        let cfg = dir.to_string_lossy().to_string();
        let g = env(&[("XDG_CONFIG_HOME", &cfg)]);
        assert!(decidir(Slot::TelaInicial, Idioma::Pt, &g, HOJE, 0).is_some(), "antes: aparece");
        let alvo = gravar_optout(&g).expect("grava");
        assert!(alvo.ends_with("schematize/connectors/sem-anuncios") && alvo.exists());
        assert!(optout_ativo(&g));
        for s in [Slot::TelaInicial, Slot::EstadoVazio] {
            assert!(decidir(s, Idioma::Pt, &g, HOJE, 0).is_none(), "depois: some em {s:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn caminho_do_optout_cai_no_home_e_falha_sem_nada() {
        let g = env(&[("HOME", "/h")]);
        assert_eq!(
            caminho_optout(&g).unwrap(),
            Caminho::from("/h/.config/schematize/connectors/sem-anuncios")
        );
        assert!(caminho_optout(&env(&[])).is_none());
        assert!(gravar_optout(&env(&[])).is_err());
    }

    #[test]
    fn link_so_abre_se_o_anuncio_existe_e_e_valido() {
        assert_eq!(link_do_anuncio("owiew-modelo"), Some("https://owiew.com"));
        assert_eq!(link_do_anuncio("https://evil.example"), None);
        assert_eq!(link_do_anuncio(""), None);
    }
}
