# Orçamento de memória (RSS) — schematize-gui (Slint)

Piso: `skill_engineering_src/references/eficiencia-runtime.md`. Métrica: `VmHWM` de `/proc/<pid>/status` (pico). Medido em 2026-11-21, Wayland/KDE, femtovg (GL).

| Operação | RSS pico alvo (teto) | Medido antes | Medido depois | Como medir |
|---|---|---|---|---|
| Janela aberta na Home, ociosa (release instalado) | 130 MiB (teto máx. GUI: 400 MiB) | 96,7 MiB | 96,7 MiB (sem mudança) | `schematize-gui --tela home`, ler `VmHWM` após ~8 s |
| Aba Grafo com projeto real (release) | 130 MiB | 98,0 MiB | 98,0 MiB | `--tela grafo --project <dir>` |
| Aba Overdev com projeto real (release) | 130 MiB | 100,0 MiB | 100,0 MiB | `--project <dir>` |
| Janela na Home (debug, perfil de teste) | 176 MiB | 124,6 MiB | 124,6 MiB | `cargo test --test orcamento_rss` (falha acima do teto) |

Notas
- O release medido é o binário instalado (0.9.4), não compilado deste checkout (o lto exige build caro); o debug é deste checkout. Teto do teste = debug medido (~129 MiB no pior caso) + ~30%.
- Sem display ou sem `/proc`, o teste pula com `eprintln` explícito.
- Causa do piso (~97 MiB): runtime GL + fontes + winit/accesskit (zbus). Não é dado nem código da app; reduzir exigiria trocar renderer/desligar a11y (mudança de comportamento, fora de escopo).
- Plano se o teto for violado: medir por tela (`--tela`), checar `cargo tree -d`, e só então subdividir a operação; nunca subir o teto sem medição.

Anúncios (ADR-0028), medido 2026-11-21 (debug, Home): pico 128636 KiB (125,6 MiB) vs 124,6 MiB antes, teto 176 MiB. A faixa é texto puro (sem imagem, sem rede), no máximo 1 por sessão e some por
completo com opt-out; não altera o teto da Home. Medição: ver `cargo test --test orcamento_rss`
(o teste falha acima do teto). BLOQUEIO: a dep `schematize_connectors` é por path local até o
repo `schematize_connectors_rs` ter remoto (ver README).
