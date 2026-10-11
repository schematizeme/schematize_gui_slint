//! MONITOR leve do `.schematize/overdev/`: uma thread relê o progresso a cada ~3 s
//! e espelha na UI. Não segura o processo do agente (ele roda em terminal externo).

use crate::prelude::*;

/// Intervalo do monitor leve do `.overdev/` (só relê arquivos de progresso).
pub(crate) const OD_MONITOR_EVERY: Duration = Duration::from_secs(3);

/// Teto de itens abertos listados no monitor (o `claude` roda fora; isto é só espelho).
pub(crate) const OD_MONITOR_ITEMS: usize = 10;

/// Intervalo MÍNIMO entre leituras de `tokens --json` dentro do monitor.
/// CUIDADO PERF: o app parseia os `.jsonl` do Claude (100MB+) — jamais a cada ciclo. Relemos os tokens no máx. a cada 30s (e sempre em thread própria).
pub(crate) const OD_USAGE_EVERY: Duration = Duration::from_secs(30);

/// thread→UI: espelha o log de conclusões (linhas já formatadas `HH:MM:SS texto`).
pub(crate) fn post_completions(weak: &Weak<AppWindow>, lines: Vec<String>) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            let rows: Vec<SharedString> = lines.into_iter().map(SharedString::from).collect();
            app.global::<Od>().set_completions(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

/// thread→UI: escreve a linha de tokens/modelo já formatada.
pub(crate) fn post_usage(weak: &Weak<AppWindow>, line: String) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.global::<Od>().set_usage_line(line.into());
        }
    });
}

/// Lê os tokens (`tokens --json`; PESADO: o app parseia .jsonl de 100MB+) numa thread PRÓPRIA e
/// posta a linha formatada. Nunca no event loop, nunca no ritmo de 3s do monitor. Falha do app
/// aparece NA linha de tokens (o motivo), e não como tokens zerados.
pub(crate) fn spawn_usage(weak: Weak<AppWindow>, project: PathBuf) {
    std::thread::spawn(move || {
        let linha = match crate::overdevcli::tokens(&project) {
            Ok(u) => fmt_usage(&u),
            Err(e) => e,
        };
        post_usage(&weak, linha);
    });
}

/// thread→UI: o monitor não conseguiu perguntar ao app — mostra o MOTIVO no status da execução.
pub(crate) fn post_erro(weak: &Weak<AppWindow>, motivo: String) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.global::<Od>().set_run_status(motivo.into());
        }
    });
}

/// thread→UI: espelha o snapshot do `.overdev/` (estado + contadores + iterações +
/// lista de itens abertos). Cria um `VecModel` novo pra a lista (roda na UI thread).
pub(crate) fn post_monitor(
    weak: &Weak<AppWindow>,
    prog: crate::overdevcli::Progresso,
    items: Vec<String>,
) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            // FONTE ÚNICA dos contadores. Estes mesmos globais são os que o bloco de
            // cima (objetivo + progresso do projeto) exibe, e que a `ChecklistView`
            // escreve no load/reload. Antes o monitor tinha um par próprio
            // (`run-done`/`run-open`/`mon-human`/`mon-hold`): como o de cima só era
            // reescrito no load, durante um run ele CONGELAVA na contagem de quando o
            // projeto foi aberto e a tela mostrava dois números discordantes pro mesmo
            // checklist. Escrever aqui mantém os dois blocos vivos e iguais.
            app.global::<Od>().set_done(prog.feitos as i32);
            app.global::<Od>().set_open(prog.abertos as i32);
            app.global::<Od>().set_human_open(prog.humanos as i32);
            app.global::<Od>().set_hold(prog.on_hold as i32);
            // O selo de estado também era só do load — ficava "stopped" com o run vivo.
            app.global::<Od>().set_mode(prog.modo.clone().into());
            app.global::<Od>().set_mon_iter(prog.iteracoes as i32);
            app.global::<Od>().set_mon_max(prog.teto as i32);
            app.global::<Od>().set_mon_mode(prog.modo.into());
            let rows: Vec<SharedString> = items.into_iter().map(SharedString::from).collect();
            app.global::<Od>().set_mon_items(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

/// thread→UI: FIM do monitor — larga o flag de "monitorando", fixa o modo final e
/// re-sonda o projeto (`od-reload`) pra o checklist/contagem refletirem o disco.
pub(crate) fn post_monitor_end(weak: &Weak<AppWindow>, mode: String) {
    let w = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = w.upgrade() {
            app.global::<Od>().set_session_running(false);
            app.global::<Od>().set_mon_mode(mode.into());
            app.global::<Od>().invoke_reload();
        }
    });
}

/// MONITOR leve: a cada ~3s pergunta ao app `progresso --json` + `monitor --json` (itens abertos
/// e log de conclusões; tudo BARATO) e espelha na UI; os tokens (`tokens --json`,
/// PESADO) só no arranque e a cada ~30s, sempre em thread própria. Se o app não responde (ausente
/// ou falhou), o monitor MOSTRA o motivo e encerra — não segue desenhando zeros. NÃO segura o
/// processo do `claude` (ele roda no terminal externo). Para quando o botão Parar
/// levanta a `stop`, ou quando o run some/termina (`mode == "stopped"` /
/// `Progress::finished()`), MAS só depois de ter visto o run ficar `active` uma vez —
/// assim um `state.json` velho ("stopped") não encerra o monitor antes de o overdev
/// sequer arrancar no terminal.
///
/// `attach`: quando `true` (botão "Reload / Acompanhar", anexando a um run que já
/// roda POR FORA), começamos com `seen_active = true` — assim um run já em
/// andamento (mode "active") é seguido de imediato e um run já FINALIZADO
/// ("stopped") posta o snapshot final uma vez e encerra, em vez de exigir que o
/// monitor testemunhe a transição pra active (que já aconteceu antes de anexarmos).
pub(crate) fn run_monitor(
    weak: Weak<AppWindow>,
    project: PathBuf,
    stop: Arc<AtomicBool>,
    attach: bool,
) {
    std::thread::spawn(move || {
        let mut seen_active = attach;
        // Última lista de conclusões ESPELHADA na UI (pra não republicar igual).
        let mut ultimas_conclusoes: Vec<String> = Vec::new();
        // Arranque: tokens uma vez (thread própria) — o resto é relido a cada ciclo.
        spawn_usage(weak.clone(), project.clone());
        let mut last_usage = Instant::now();
        loop {
            if stop.load(Ordering::SeqCst) {
                post_monitor_end(&weak, "stopped".into());
                return;
            }
            let (prog, mon) = match crate::overdevcli::progresso(&project)
                .and_then(|p| crate::overdevcli::monitor(&project).map(|m| (p, m)))
            {
                Ok(par) => par,
                Err(e) => {
                    post_erro(&weak, e);
                    post_monitor_end(&weak, "stopped".into());
                    return;
                }
            };
            let items: Vec<String> = mon.abertos.into_iter().take(OD_MONITOR_ITEMS).collect();
            // Só reconstrói o log na UI quando ele MUDOU. Reenviar a lista igual a
            // cada 3 s destruía e recriava o repeater inteiro à toa — trabalho de
            // layout no event loop enquanto o usuário lê/rola a tela.
            let comps = fmt_completions(mon.conclusoes);
            if comps != ultimas_conclusoes {
                ultimas_conclusoes.clone_from(&comps);
                post_completions(&weak, comps);
            }
            // Tokens: no MÁX. a cada 30s, sempre fora do event loop.
            if last_usage.elapsed() >= OD_USAGE_EVERY {
                spawn_usage(weak.clone(), project.clone());
                last_usage = Instant::now();
            }
            let mode = prog.modo.clone();
            let finished = prog.terminou();
            if mode == "active" {
                seen_active = true;
            }
            post_monitor(&weak, prog, items);
            if seen_active && (mode == "stopped" || finished) {
                post_monitor_end(&weak, if mode.is_empty() { "stopped".into() } else { mode });
                return;
            }
            // Dorme em fatias curtas pra responder rápido ao botão Parar.
            let mut slept = Duration::ZERO;
            while slept < OD_MONITOR_EVERY {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
                slept += Duration::from_millis(200);
            }
        }
    });
}
