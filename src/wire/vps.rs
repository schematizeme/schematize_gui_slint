//! Fiação da tela do GESTOR DE VPS.
//!
//! "Fiação" = registrar os callbacks do `.slint` neste recorte da UI. Nenhuma REGRA mora
//! aqui: política, auditoria, sondagem e bootstrap são da lib `schematize::vps`. Se esta
//! camada decidisse qualquer coisa, seria mais um lugar por onde escapar da política.
//!
//! ## Toda I/O de rede vai pra THREAD
//! `probe`, `bootstrap`, `trust` e `exec` falam com um host que pode estar fora do ar. Rodar
//! isso na event loop congelaria a janela — que é o piso 10 (independência de runtime)
//! quebrado dentro da própria UI. O padrão é o mesmo de `wire/ssh.rs`: `thread::spawn` +
//! `slint::invoke_from_event_loop` pra voltar.
//!
//! ## O que a UI mostra é o que o host TEM
//! O badge de fronteira sai de `VpsProfile::fronteira`, gravado pela última sondagem — não de
//! uma expectativa nossa. Host nunca sondado aparece como "sem fronteira", que é a verdade
//! conhecida no momento (ADR-0005: falha fechada na leitura).

use crate::prelude::*;
use crate::wire::Ctx;
use schematize::vps;

/// Abre a conexão do banco de VPS, ou devolve a mensagem de erro pra UI.
///
/// **Onde:** todo callback deste módulo. Conexão por operação (barata) em vez de estado
/// global mutável compartilhado entre GUI e CLI.
fn conn() -> Result<vps::db::Conn, String> {
    vps::db::open()
}

/// Monta as linhas da lista a partir do registro.
///
/// **Onde:** `refresh` e toda ação que muda o registro. Cada linha já traz o rótulo e a
/// explicação do nível — a UI não recalcula nem interpreta nada.
fn build_rows() -> Vec<VpsRow> {
    let Ok(c) = conn() else { return Vec::new() };
    vps::listar(&c)
        .unwrap_or_default()
        .into_iter()
        .map(|h| {
            let verbos = vps::verbos::listar(&c, &h.alias).map(|v| v.len()).unwrap_or(0) as i32;
            VpsRow {
                destino: format!("{}@{}:{}", h.usuario, h.host, h.port).into(),
                ambiente: h.ambiente.as_str().into(),
                modo: h.modo.as_str().into(),
                confiado: vps::esta_confiado(&h),
                fronteira: h.fronteira.rotulo().into(),
                server_side: h.fronteira.e_server_side(),
                explicacao: h.fronteira.explicacao().into(),
                alias: h.alias.into(),
                verbos,
                op_label: SharedString::new(),
                op_error: false,
            }
        })
        .collect()
}

/// Formata um epoch em `AAAA-MM-DD HH:MM` UTC, sem trazer crate de data.
///
/// **Onde:** as linhas da trilha. Aritmética civil simples: o app já evita dependência de
/// data em outros pontos (ver `overdevdb::now_secs`), e uma data legível não justifica uma.
fn quando(ts: i64) -> String {
    // TETO E PISO ANTES DO LAÇO.
    //
    // O laço avança ano a ano a partir de 1970. Com um `ts` corrompido no banco (ou um relógio
    // maluco), `i64::MAX` daria ~2,9e11 iterações — a janela CONGELA. E um `ts` negativo
    // produzia lixo tipo `1970-01--18249`. Achado no teste destrutivo.
    //
    // Timestamp fora da faixa plausível não é data: é dado corrompido, e a UI diz isso em vez
    // de fingir uma data ou travar.
    const MIN: i64 = 0;                 // 1970-01-01
    const MAX: i64 = 253_402_300_799;   // 9999-12-31 23:59:59
    if !(MIN..=MAX).contains(&ts) {
        return format!("(data inválida: {ts})");
    }
    let dias = ts.div_euclid(86_400);
    let seg = ts.rem_euclid(86_400);
    let (mut a, mut d) = (1970i64, dias);
    loop {
        let bis = (a % 4 == 0 && a % 100 != 0) || a % 400 == 0;
        let n = if bis { 366 } else { 365 };
        if d < n {
            break;
        }
        d -= n;
        a += 1;
    }
    let bis = (a % 4 == 0 && a % 100 != 0) || a % 400 == 0;
    let meses = [31, if bis { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    while m < 12 && d >= meses[m] {
        d -= meses[m];
        m += 1;
    }
    format!("{a:04}-{:02}-{:02} {:02}:{:02}", m + 1, d + 1, seg / 3600, (seg % 3600) / 60)
}

/// Trilha de auditoria de um host (ou de todos, com alias vazio).
fn build_log(alias: &str) -> Vec<VpsLogRow> {
    let Ok(c) = conn() else { return Vec::new() };
    vps::listar_comandos(&c, alias, 100)
        .unwrap_or_default()
        .into_iter()
        .map(|l| VpsLogRow {
            quando: quando(l.ts).into(),
            negado: l.veredito == "deny",
            veredito: l.veredito.into(),
            exit_code: l.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()).into(),
            duracao: format!("{} ms", l.duracao_ms).into(),
            comando: l.comando.into(),
            alias: l.alias.into(),
        })
        .collect()
}

/// Catálogo de verbos de um host.
fn build_verbos(alias: &str) -> Vec<VpsVerbRow> {
    let Ok(c) = conn() else { return Vec::new() };
    vps::verbos::listar(&c, alias)
        .unwrap_or_default()
        .into_iter()
        .map(|v| VpsVerbRow { nome: v.nome.into(), comando: v.comando.into() })
        .collect()
}

/// Alias da linha `i`, se existir. Índice vindo da UI é entrada como outra qualquer.
fn alias_de(app: &AppWindow, i: i32) -> Option<String> {
    let rows = app.global::<Vps>().get_rows();
    (i >= 0).then(|| rows.row_data(i as usize)).flatten().map(|r| r.alias.to_string())
}

/// Escreve o banner de resultado.
fn banner(app: &AppWindow, msg: impl Into<SharedString>, erro: bool) {
    let g = app.global::<Vps>();
    g.set_banner(msg.into());
    g.set_banner_error(erro);
}

/// Roda `trabalho` numa thread e devolve o resultado pela event loop.
///
/// **Onde:** todo callback que fala com a rede. Centraliza o `busy`, o banner e o refresh —
/// sem isto, cada callback repetiria as três coisas e uma delas seria esquecida.
fn em_thread<F>(app: &AppWindow, trabalho: F)
where
    F: FnOnce() -> Result<String, String> + Send + 'static,
{
    app.global::<Vps>().set_busy(true);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let r = trabalho();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                let g = app.global::<Vps>();
                g.set_busy(false);
                match r {
                    Ok(msg) => banner(&app, msg, false),
                    Err(e) => banner(&app, e, true),
                }
                // O registro pode ter mudado (sondagem, bootstrap, política): recarrega.
                let m = Rc::new(VecModel::<VpsRow>::from(build_rows()));
                app.global::<Vps>().set_rows(ModelRc::from(m));
                let sel = app.global::<Vps>().get_sel();
                if let Some(a) = alias_de(&app, sel) {
                    let lm = Rc::new(VecModel::<VpsLogRow>::from(build_log(&a)));
                    app.global::<Vps>().set_log(ModelRc::from(lm));
                    let vm = Rc::new(VecModel::<VpsVerbRow>::from(build_verbos(&a)));
                    app.global::<Vps>().set_verbos(ModelRc::from(vm));
                }
            }
        });
    });
}

/// Liga os callbacks deste recorte da UI.
pub(crate) fn wire(app: &AppWindow, _cx: &Ctx) {
    let g = app.global::<Vps>();
    g.set_rows(ModelRc::from(Rc::new(VecModel::<VpsRow>::from(build_rows()))));

    // ---- refresh ----------------------------------------------------------
    {
        let weak = app.as_weak();
        g.on_refresh(move || {
            let Some(app) = weak.upgrade() else { return };
            let m = Rc::new(VecModel::<VpsRow>::from(build_rows()));
            app.global::<Vps>().set_rows(ModelRc::from(m));
            banner(&app, "", false);
        });
    }

    // ---- selecionar um host (carrega trilha + catálogo) --------------------
    {
        let weak = app.as_weak();
        g.on_pick(move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.global::<Vps>().set_sel(i);
            let Some(alias) = alias_de(&app, i) else { return };
            let lm = Rc::new(VecModel::<VpsLogRow>::from(build_log(&alias)));
            app.global::<Vps>().set_log(ModelRc::from(lm));
            let vm = Rc::new(VecModel::<VpsVerbRow>::from(build_verbos(&alias)));
            app.global::<Vps>().set_verbos(ModelRc::from(vm));
            app.global::<Vps>().set_saida(SharedString::new());
        });
    }

    // ---- registrar host ----------------------------------------------------
    {
        let weak = app.as_weak();
        g.on_add(move || {
            let Some(app) = weak.upgrade() else { return };
            let g = app.global::<Vps>();
            let (alias, host, user, key) = (
                g.get_f_alias().to_string(),
                g.get_f_host().to_string(),
                g.get_f_user().to_string(),
                g.get_f_key().to_string(),
            );
            // Porta inválida não vira panic nem 0: cai no default 22 (falha fechada útil).
            let port: u16 = g.get_f_port().to_string().trim().parse().unwrap_or(22);
            let env = g.get_f_env().to_string();
            let r = (|| -> Result<String, String> {
                let c = conn()?;
                let mut p = vps::VpsProfile::novo(&alias, &host, &user, &key);
                p.port = port;
                p.ambiente = vps::Ambiente::from_raw(&env);
                vps::salvar(&c, &p)?;
                Ok(format!(
                    "host {alias:?} registrado ({}). Próximo passo: confiar na host key.",
                    p.ambiente.as_str()
                ))
            })();
            match r {
                Ok(msg) => {
                    banner(&app, msg, false);
                    // Limpa o formulário pra o próximo host (o alias já foi usado).
                    let g = app.global::<Vps>();
                    g.set_f_alias(SharedString::new());
                    g.set_f_host(SharedString::new());
                    g.set_f_user(SharedString::new());
                    g.set_f_key(SharedString::new());
                }
                Err(e) => banner(&app, e, true),
            }
            let m = Rc::new(VecModel::<VpsRow>::from(build_rows()));
            app.global::<Vps>().set_rows(ModelRc::from(m));
        });
    }

    // ---- remover host ------------------------------------------------------
    {
        let weak = app.as_weak();
        g.on_remove(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(alias) = alias_de(&app, i) else { return };
            let r = conn().and_then(|c| vps::remover(&c, &alias));
            match r {
                Ok(_) => banner(&app, format!("host {alias:?} removido — a trilha permanece"), false),
                Err(e) => banner(&app, e, true),
            }
            app.global::<Vps>().set_sel(-1);
            let m = Rc::new(VecModel::<VpsRow>::from(build_rows()));
            app.global::<Vps>().set_rows(ModelRc::from(m));
        });
    }

    // ---- sondar (rede) -----------------------------------------------------
    {
        let weak = app.as_weak();
        g.on_probe(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(alias) = alias_de(&app, i) else { return };
            em_thread(&app, move || {
                let c = conn()?;
                let mut p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
                let s = vps::sondar(&c, &p)?;
                p.fronteira = s.instalada;
                p.sondado_em = vps::db::agora_secs();
                vps::salvar(&c, &p)?;
                let mut msg = format!(
                    "{alias}: instalada={} · possível={}",
                    s.instalada.rotulo(),
                    s.possivel.rotulo()
                );
                if s.pode_melhorar() {
                    msg.push_str(" — dá pra subir de nível com \"Instalar fronteira\"");
                }
                Ok(msg)
            });
        });
    }

    // ---- bootstrap (rede, escreve no host) ---------------------------------
    {
        let weak = app.as_weak();
        g.on_bootstrap(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(alias) = alias_de(&app, i) else { return };
            em_thread(&app, move || {
                let c = conn()?;
                let mut p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
                let r = vps::bootstrap::instalar(&c, &mut p)?;
                Ok(if r.melhorou() {
                    format!("{alias}: {} -> {} · {} verbo(s) sincronizado(s)", r.antes.rotulo(), r.depois.rotulo(), r.verbos)
                } else {
                    format!("{alias}: {} (sem mudança). {}", r.depois.rotulo(), r.depois.explicacao())
                })
            });
        });
    }

    // ---- confiar na host key: busca a fingerprint e ABRE o modal -----------
    {
        let weak = app.as_weak();
        g.on_trust_request(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(alias) = alias_de(&app, i) else { return };
            app.global::<Vps>().set_busy(true);
            let weak2 = app.as_weak();
            std::thread::spawn(move || {
                let r = (|| -> Result<(String, bool), String> {
                    let c = conn()?;
                    let p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
                    let cand = vps::descobrir_host_key(&p)?;
                    let mudou = p
                        .fingerprint
                        .as_ref()
                        .is_some_and(|a| a.trim() != cand.fingerprint.trim());
                    Ok((cand.fingerprint, mudou))
                })();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak2.upgrade() {
                        let g = app.global::<Vps>();
                        g.set_busy(false);
                        match r {
                            Ok((fp, mudou)) => {
                                g.set_trust_fingerprint(fp.into());
                                g.set_trust_alias(alias.into());
                                g.set_trust_mudou(mudou);
                                g.set_trust_open(true);
                            }
                            Err(e) => banner(&app, e, true),
                        }
                    }
                });
            });
        });
    }
    {
        let weak = app.as_weak();
        g.on_trust_confirm(move || {
            let Some(app) = weak.upgrade() else { return };
            let alias = app.global::<Vps>().get_trust_alias().to_string();
            app.global::<Vps>().set_trust_open(false);
            em_thread(&app, move || {
                let c = conn()?;
                let mut p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
                // Re-colhe a chave no momento de confiar: a que o humano viu tem que ser a
                // que vai pro disco, e um intervalo grande entre ver e confiar não pode
                // pinar uma chave diferente sem ninguém notar.
                let cand = vps::descobrir_host_key(&p)?;
                vps::confiar(&c, &mut p, &cand)?;
                Ok(format!("host key de {alias} pinada — as execuções já funcionam"))
            });
        });
    }
    {
        let weak = app.as_weak();
        g.on_trust_cancel(move || {
            if let Some(app) = weak.upgrade() {
                app.global::<Vps>().set_trust_open(false);
            }
        });
    }

    // ---- executar ----------------------------------------------------------
    {
        let weak = app.as_weak();
        g.on_exec(move || {
            let Some(app) = weak.upgrade() else { return };
            let sel = app.global::<Vps>().get_sel();
            let Some(alias) = alias_de(&app, sel) else { return };
            let cmd = app.global::<Vps>().get_cmd().to_string();
            if cmd.trim().is_empty() {
                return;
            }
            executar(&app, alias, cmd, vps::Confirmacao::Ausente);
        });
    }

    // ---- o gate humano: confirmar / cancelar -------------------------------
    {
        let weak = app.as_weak();
        g.on_confirm_yes(move || {
            let Some(app) = weak.upgrade() else { return };
            let g = app.global::<Vps>();
            g.set_confirm_open(false);
            let cmd = g.get_confirm_cmd().to_string();
            let sel = g.get_sel();
            let Some(alias) = alias_de(&app, sel) else { return };
            // Só AQUI a confirmação humana existe — e só porque uma pessoa clicou no modal.
            executar(&app, alias, cmd, vps::Confirmacao::HumanoConfirmou);
        });
    }
    {
        let weak = app.as_weak();
        g.on_confirm_no(move || {
            if let Some(app) = weak.upgrade() {
                let g = app.global::<Vps>();
                g.set_confirm_open(false);
                banner(&app, "execução cancelada", false);
            }
        });
    }

    // ---- política: modo e ambiente -----------------------------------------
    {
        let weak = app.as_weak();
        g.on_set_modo(move |i, modo| {
            let Some(app) = weak.upgrade() else { return };
            trocar(&app, i, Some(modo.to_string()), None);
        });
    }
    {
        let weak = app.as_weak();
        g.on_set_env(move |i, env| {
            let Some(app) = weak.upgrade() else { return };
            trocar(&app, i, None, Some(env.to_string()));
        });
    }

    // ---- semear o catálogo -------------------------------------------------
    {
        let weak = app.as_weak();
        g.on_seed_verbs(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(alias) = alias_de(&app, i) else { return };
            let r = conn().and_then(|c| vps::verbos::semear(&c, &alias));
            match r {
                Ok(n) => banner(&app, format!("{n} verbo(s) criado(s) — revise antes de instalar a fronteira"), false),
                Err(e) => banner(&app, e, true),
            }
            let vm = Rc::new(VecModel::<VpsVerbRow>::from(build_verbos(&alias)));
            app.global::<Vps>().set_verbos(ModelRc::from(vm));
            let m = Rc::new(VecModel::<VpsRow>::from(build_rows()));
            app.global::<Vps>().set_rows(ModelRc::from(m));
        });
    }

    // ---- abrir no terminal do sistema (o caminho do HUMANO) ----------------
    {
        let weak = app.as_weak();
        g.on_open_terminal(move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some(alias) = alias_de(&app, i) else { return };
            let r = (|| -> Result<String, String> {
                let c = conn()?;
                let p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
                // A montagem da linha de ssh é da LIB — a GUI não constrói comando remoto.
                vps::abrir_no_terminal(&p)
                    .map(|_| format!("terminal aberto em {alias}"))
                    .map_err(|e| format!("não consegui abrir um terminal: {e}"))
            })();
            match r {
                Ok(msg) => banner(&app, msg, false),
                Err(e) => banner(&app, e, true),
            }
        });
    }
}

/// Executa um comando em thread, tratando o veredito `Confirm` como ABERTURA DE MODAL.
///
/// **Onde:** `on_exec` e `on_confirm_yes`. É a peça que transforma a recusa "precisa de
/// confirmação humana" da lib numa pergunta de verdade na tela — em vez de num erro que o
/// usuário não sabe resolver (§37.48).
fn executar(app: &AppWindow, alias: String, cmd: String, conf: vps::Confirmacao) {
    app.global::<Vps>().set_busy(true);
    let weak = app.as_weak();
    let cmd_eco = cmd.clone();
    std::thread::spawn(move || {
        let r = (|| -> Result<String, String> {
            let c = conn()?;
            let p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
            let out = vps::executar(&c, &p, &cmd, "gui", conf)?;
            let mut s = format!(
                "exit={} · {} ms\n",
                out.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "sinal".into()),
                out.duracao_ms
            );
            s.push_str(out.stdout.trim_end());
            if !out.stderr.trim().is_empty() {
                s.push_str(&format!("\n--- erros ---\n{}", out.stderr.trim_end()));
            }
            Ok(s)
        })();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                let g = app.global::<Vps>();
                g.set_busy(false);
                match r {
                    Ok(saida) => {
                        g.set_saida(saida.into());
                        banner(&app, "executado e auditado", false);
                    }
                    Err(e) => {
                        // A recusa por falta de confirmação vira PERGUNTA, não erro.
                        if e.contains("confirmação humana") {
                            g.set_confirm_msg(e.into());
                            g.set_confirm_cmd(cmd_eco.into());
                            g.set_confirm_open(true);
                        } else {
                            banner(&app, e, true);
                        }
                    }
                }
                let sel = app.global::<Vps>().get_sel();
                if let Some(a) = alias_de(&app, sel) {
                    let lm = Rc::new(VecModel::<VpsLogRow>::from(build_log(&a)));
                    app.global::<Vps>().set_log(ModelRc::from(lm));
                }
            }
        });
    });
}

/// Troca modo e/ou ambiente de um host e recarrega a lista.
fn trocar(app: &AppWindow, i: i32, modo: Option<String>, env: Option<String>) {
    let Some(alias) = alias_de(app, i) else { return };
    let r = (|| -> Result<String, String> {
        let c = conn()?;
        let mut p = vps::buscar(&c, &alias)?.ok_or("host sumiu do registro")?;
        if let Some(m) = &modo {
            p.modo = vps::ModoPolitica::from_raw(m);
        }
        if let Some(e) = &env {
            p.ambiente = vps::Ambiente::from_raw(e);
        }
        vps::salvar(&c, &p)?;
        Ok(format!("{alias}: modo={} · ambiente={}", p.modo.as_str(), p.ambiente.as_str()))
    })();
    match r {
        Ok(msg) => banner(app, msg, false),
        Err(e) => banner(app, e, true),
    }
    let m = Rc::new(VecModel::<VpsRow>::from(build_rows()));
    app.global::<Vps>().set_rows(ModelRc::from(m));
}

#[cfg(test)]
mod tests {
    use super::quando;

    #[test]
    fn timestamp_absurdo_nao_congela_a_janela() {
        // O laço avançava ano a ano: `i64::MAX` são ~2,9e11 voltas. Se este teste demorar,
        // é porque a guarda saiu.
        let inicio = std::time::Instant::now();
        for ts in [i64::MAX, i64::MIN, -1, -86_400 * 365 * 50, 253_402_300_800, 1 << 40] {
            let s = quando(ts);
            assert!(s.contains("inválida"), "ts={ts} devia ser recusado, veio {s:?}");
        }
        assert!(inicio.elapsed().as_millis() < 100, "a formatação está iterando demais");
    }

    #[test]
    fn formata_epoch_em_data_legivel() {
        assert_eq!(quando(0), "1970-01-01 00:00");
        assert_eq!(quando(1_000_000_000), "2001-09-09 01:46");
        // 2024 é bissexto: 29 de fevereiro tem que existir.
        assert_eq!(quando(1_709_164_800), "2024-02-29 00:00");
        assert_eq!(quando(1_735_689_600), "2025-01-01 00:00");
    }
}
