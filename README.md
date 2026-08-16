# schematize_gui_slint — schematize em Slint (migração incremental)

Migração **incremental** da GUI do schematize de [egui](../schematize_cli_rs/src/gui.rs)
para o toolkit [**Slint**](https://slint.dev). Este crate é a base da nova GUI; começa pela
**aba _Skills_**, agora um **GESTOR funcional de verdade** (dados e ações reais).

> ⚠️ Enquanto o Slint não chega a paridade, **quem shippa é a GUI egui** em
> `schematize_cli_rs/` (abas Skills + Overdev + Grafo). Este crate **não toca** em nada
> fora de `schematize_gui_slint/`, exceto **adicionar chaves i18n** em
> `schematize_cli_rs/src/i18n/*.json` (única exceção; sem mexer em build/versão do CLI).

---

## O que já funciona (1º incremento: aba Skills)

Uma janela única com a **aba Skills** como gestor real:

- **Lista agrupada por categoria** — _Base & Arquitetura_ / _Linguagens_ /
  _Ferramentas externas_ (taxonomia `base|language|external` do catálogo).
- Por linha: **checkbox de seleção** (custom, temático), **nome da skill**, **selo
  `✓ Verificado`**, coluna **Autor** (`sponsor.name`, **clicável** → abre `sponsor.url`
  via `xdg-open`), **versão instalada** e **latest** REAIS, e uma **pill de estado**
  derivado (_Não instalada_ / _Atualizada_ / _Desatualizada (X→Y)_ / _…carregando_).
- **Ações reais, em massa e em PARALELO**: botões _Instalar selecionadas_, _Remover
  selecionadas_, _Atualizar tudo_, seleção rápida (_todas/pendentes/nenhuma_) e ações
  **por-linha** (instalar/atualizar/remover). Cada operação roda numa thread própria
  (`std::thread::scope`) chamando `skills::install`/`skills::remove`; a linha mostra
  _instalando…_/_removendo…_ → _✓_/_erro_ e há um **toast final** com o placar do lote.
- **Tema claro/escuro** caprichado, alternável em runtime (um clique repinta a janela
  inteira via a `global Theme`).
- **i18n de verdade**: NADA de texto hardcoded no `.slint`. Todos os rótulos vêm de
  `schematize::i18n` (11 locales) injetados no `global L`; o locale é detectado como na
  GUI egui (`config → $SCHEMATIZE_LANG → $LANG/$LC_*` → `en`).

### Dados e ações — reusam o crate `schematize`
Depende do crate irmão por **path**, **sem a feature `gui`** (não puxa egui/rfd):

```toml
schematize = { path = "../schematize_cli_rs", default-features = false }
```

- Catálogo: `schematize::registry::catalog()` (índice remoto com fallback embutido);
- Versão instalada: `skills::installed_version(&Item)` (lê `~/.claude/skills/<dir>/VERSION`);
- Latest: `skills::resolve_latest(&Item)` — **rede**, resolvido **assíncrono** (uma thread
  por skill; enquanto não volta, a coluna mostra `…`), entregue à UI por
  `slint::invoke_from_event_loop` + `Weak<AppWindow>::upgrade` (padrão thread→UI do Slint);
- Instalar/remover: `skills::install`/`skills::remove` (thread-safe no lib via `STATE_LOCK`).

### Abas Overdev e Grafo
Ficam na barra de abas com um selo **"em breve"** e um painel placeholder — a estrutura já
existe, mas a implementação é dos **próximos incrementos** (não neste).

---

## Como rodar

```bash
cd schematize_gui_slint
cargo run            # numa máquina com display (Wayland ou X11)
```

### Dependências de build (importante)
Slint 1.17 traz o `fontique` no núcleo (`i-slint-common`), que **linka a libfontconfig
do sistema**. Logo, numa máquina de desenvolvimento normal o build precisa de:

- **`pkg-config`** e **`libfontconfig1-dev`** (Debian/Ubuntu/Mint) — ou
  `fontconfig-devel` (Fedora/openSUSE);
- um toolchain C (`cc`/`gcc`) — já necessário pra qualquer build Rust com deps nativas.

Runtime (a máquina que **roda**): `libfontconfig.so.1`, `libGL`/EGL, `libxkbcommon`,
`libwayland-client` — todas presentes num desktop KDE/GNOME comum. O **winit** carrega
X11 (`x11-dl`) e Wayland por **dlopen** em runtime, então o build **não** precisa das
libs `-dev` de janela (só das de fontconfig).

> **Nota do sandbox onde este spike foi validado:** a máquina de CI/sandbox **não tinha
> `pkg-config` nem `libfontconfig.so` (dev)**, então a validação de compilação foi feita
> com um _shim_ de `pkg-config` apontando pra `libfontconfig.so.1` já presente. **Nada
> disso está commitado** — numa distro com `pkg-config` + `libfontconfig1-dev` o
> `cargo build` roda direto. Tentei o caminho `RUST_FONTCONFIG_DLOPEN=1` (que dispensaria
> o pkg-config), mas ele **quebra o `fontique`** (que importa os símbolos `Fc*` direto),
> então dlopen **não** é opção no Slint 1.17.

### Como o Slint escolhe backend/renderer
- **Backend de janela**: `backend-winit` (padrão em desktop). O winit fala **Wayland _e_
  X11** com o mesmo binário e escolhe em runtime (preferindo Wayland se `WAYLAND_DISPLAY`
  existir; senão X11 via `DISPLAY`). Dá pra forçar com `SLINT_BACKEND=winit-femtovg` e
  `WINIT_UNIX_BACKEND=wayland|x11`.
- **Renderer**: `renderer-femtovg` (OpenGL/GLES via `glutin`/`glow`). **Não** ativamos
  `renderer-skia` de propósito — skia exige **cmake** (ausente no sandbox) e infla muito o
  build. Também existe `renderer-software` (tiny-skia, sem GPU) como plano B pra ambientes
  sem GL.
- **Detecção de ambiente**: o `main.rs` loga Wayland-vs-X11 (`WAYLAND_DISPLAY` /
  `DISPLAY` / `XDG_SESSION_TYPE`) e o desktop (`XDG_CURRENT_DESKTOP`) na subida — só
  informativo; quem decide o transporte é o winit.

### Empacotamento (o que seria preciso)
- **.deb / .rpm**: mesmo esquema do crate real (`cargo-deb` / `cargo-generate-rpm`).
  Declarar como `depends`/`requires` as libs carregadas por dlopen (X11/Wayland/GL/
  xkbcommon) **+ `libfontconfig1`/`fontconfig`** (esta o Slint linka de fato, então o
  `$auto`/soname já a pega). O `.slint` é compilado **para dentro do binário** por
  `slint-build`, então não há asset de UI pra empacotar à parte.
- **Fontes**: como o femtovg usa fontconfig, o binário acha as fontes do sistema; num
  container mínimo, incluir `fontconfig` + um pacote de fontes (ex.: `fonts-dejavu`).

---

## Status de validação

| Item | Estado |
|---|---|
| `cargo build` (debug) | ✅ **compila limpo, sem warnings** (com `pkg-config` presente; no sandbox, via shim) |
| `cargo build --release` | ✅ compila (LTO + `opt-level="z"`) |
| Dep de path `schematize` sem feature `gui` | ✅ confirmado — `eframe`/`egui`/`rfd` **ausentes** do `Cargo.lock` |
| Sobe o event loop sem panic | ✅ rodou ~6s sob Wayland/KDE (SIGTERM ao fim, sem panic); catálogo lido com 18 skills via `registry::catalog()` |
| Abre a janela / renderiza | ⚠️ **não verificado visualmente** — o agente de validação é headless (sem captura de tela). A janela foi **criada** no compositor Wayland e o processo permaneceu vivo, mas o acabamento é o descrito no código, **não confirmado a olho**. |

Assíncrono/ações também **não foram exercidos a olho** (sem interação de UI possível no
headless); a lógica é a mesma do `run_batch` do egui, reusando `skills::install/remove` do
lib. O locale detectado no ambiente foi `pt`.

---

## Slint × egui — prós/contras observados neste spike

**A favor do Slint**
- **Separação limpa** UI/lógica: o `.slint` é declarativo (parecido com QML), com `global
  Theme`, componentes reutilizáveis (`Check`, `Btn`, `StatePill`) e bindings reativos —
  bem mais legível que montar a árvore imperativa a cada frame no egui.
- **Tema trocável de verdade**: uma `global` com props derivadas repinta tudo; no egui o
  claro/escuro é mais na mão.
- **Retido/reativo** (vs. imediato do egui): só repinta no que muda → ocioso barato.
- **i18n**: Slint tem `@tr("...")` nativo (gettext), enquanto no egui a casa rolou um
  `i18n::t()` próprio.
- **Renderers plugáveis** (femtovg/skia/software) — bom pra ambientes sem GPU.

**Contra o Slint**
- **Peso de build/deps**: puxa `fontique`/`parley`/`femtovg`/`glutin` e **linka
  fontconfig** → exige `pkg-config` + `libfontconfig1-dev` no build (o egui atual da casa
  compila com menos atrito e já roda em Wayland/X11/Windows com `glow`). O `renderer-skia`
  ainda exige `cmake`.
- **Binário maior** (ver tabela de tamanho no fim) — o egui da casa é `strip+opt-z+lto`
  e fica enxuto; o Slint agrega mais dependências nativas.
- **Curva do `.slint`**: é uma DSL própria (sintaxe, modelo de layout, `VecModel`,
  callbacks) — poder e limites diferentes do Rust puro do egui.
- **Grafo force-directed / canvas custom** (aba Grafo): no egui é `painter` imperativo
  trivial; no Slint daria mais trabalho (Path/Canvas ou renderizar textura por fora).

**Neutro**
- Ecossistema: egui tem mais crates de widget prontos; Slint tem tooling melhor (LSP,
  preview ao vivo `slint-viewer`, VS Code).

## Tamanho de binário (medido neste host, perfil release `strip`+`opt-z`+`lto`)

| Binário | Tamanho |
|---|---|
| `schematize-gui-slint` (este spike, Slint/femtovg) | **14 M** |
| `schematize-gui` (a GUI atual, egui/glow) | **9,2 M** |
| `schematize` (CLI, sem GUI) | 2,3 M |

Ou seja: o Slint ficou **~50% maior** que o egui pra a MESMA tela (só a aba Skills; o
egui já inclui as três abas). Com skia (não usado aqui) seria bem mais.

---

## Caminho para paridade com o hub egui

Pra virar substituto do hub atual, ainda falta portar:

1. **Abas Overdev e Grafo** — Overdev é lista/seções (fácil no Slint); o **Grafo
   force-directed** (física + pan/zoom + hit-test + labels) precisaria de `Path`/canvas ou
   uma textura renderizada por fora (o custo real da migração está aqui). _Placeholders
   "em breve" já na barra de abas._
2. **Picker de pasta nativo** — hoje `rfd` (portal XDG). `rfd` é agnóstico de toolkit,
   então integra com o event loop do Slint sem GTK. (Usado só nas abas Overdev/Grafo.)
3. **Self-update do CLI + notificações de desktop** (`notify-rust`, `pkexec`, relançar
   binário) — independentes do toolkit; portam quase 1:1. _Ainda não portado: a linha
   "schematize (CLI)" do gestor egui e o botão de self-update não estão nesta tela._
4. **Troca de idioma em runtime** — o locale é **detectado** na subida (como a GUI egui) e
   todas as strings vêm do `i18n` da casa via o `global L`, mas ainda **não há combo** pra
   trocar de idioma sem reabrir (re-injetar o `L` num callback é o passo que falta).
6. **Fontes CJK/árabe/etc.** — o egui injeta fallbacks à mão pra matar "tofu"; no Slint,
   via fontconfig, o sistema já resolve (com os pacotes de fonte instalados).

✅ **Feito neste incremento (5. Instalação real):** `skills::install/remove`,
`installed_version`, `resolve_latest`, execução **paralela** e o modelo de
progresso/estado por linha + toast — tudo ligado na aba Skills.

## Veredito (resumido)

O Slint **entrega o salto visual e um código de UI mais limpo/manutenível**, mas **cobra
em deps de build (fontconfig/pkg-config, cmake se skia) e tamanho de binário**, e a **aba
Grafo é o item caro** de portar. Recomendação: **não** reescrever tudo de uma vez — se
formos adiante, migrar **incrementalmente** começando pela aba Skills (esta tela), medindo
tamanho/atrito de empacotamento .deb/.rpm num alvo real antes de comprometer as abas
Overdev/Grafo. Ver o relato do spike para o veredito detalhado.
