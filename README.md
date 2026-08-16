# schematize_gui_slint — SPIKE de UI em Slint

**Prova de conceito isolada.** Reimplementa **só a aba _Skills_** da GUI atual do
schematize (hoje em [egui](../schematize_cli_rs/src/gui.rs)) usando o toolkit
[**Slint**](https://slint.dev), para avaliarmos um **salto visual**.

> ⚠️ Isto **NÃO é o binário que shippa.** A GUI que vai pro usuário continua sendo a
> de egui em `schematize_cli_rs/`. Este crate é descartável / avaliativo e **não toca**
> em nada fora de `schematize_gui_slint/`.

---

## O que o spike mostra

Uma janela única com a **aba Skills** bem acabada:

- **Lista agrupada por categoria** — _Base & Arquitetura_ / _Linguagens_ /
  _Ferramentas externas_ (mesma taxonomia `base|language|external` do `catalog.json`).
- Por linha: **checkbox de seleção** (custom, temático), **nome da skill**, **selo
  `✓ Verificado`**, coluna **Autor** (`sponsor.name`), **versão instalada** e **latest**
  (placeholders `—` no spike), e uma **pill de estado** (_Instalar_ / _Atualizar_ / _Em dia_).
- **Tema claro/escuro** caprichado, alternável em runtime (um clique repinta a janela
  inteira via a `global Theme`), com cor de acento, tipografia legível, respiro generoso,
  linhas zebradas e cabeçalho/toolbar próprios.
- Botões **"Instalar selecionadas"** e **"Atualizar tudo"** — **no-op** neste spike
  (logam no stderr e atualizam o texto de status, tipo _toast_). O foco é o **visual**;
  a mecânica real (download/instalação/versão) já existe no crate `schematize`.

### Dados
Lê o **`catalog.json` do crate irmão** direto por `serde` (sem depender do crate
`schematize` — evita ciclos de features e mantém o spike isolado). Ordem de resolução:

1. `../schematize_cli_rs/catalog.json` (e mais dois candidatos) em disco;
2. **fallback**: snapshot embutido em build-time via `include_str!` (read-only; não
   altera o outro crate).

As **versões instalada/latest são placeholders** (`—`) e o estado é sempre _Instalar_ —
não há acesso à instalação real (`skills::installed_version` / `resolve_latest` vivem no
crate `schematize`, deliberadamente não linkado aqui).

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
| `cargo build` (debug) | ✅ **compila limpo** (com `pkg-config` presente; no sandbox, via shim) |
| `cargo build --release` | ✅ compila (LTO + `opt-level="z"`) |
| Abre a janela / renderiza | ⚠️ **não verificado visualmente** — o ambiente de validação é headless-equivalente (sem tooling de captura confiável). O processo **subiu e rodou o event loop sem panic** e a detecção de ambiente logou Wayland/KDE corretamente. |

O `.slint` compila (checagem de tipos/layout do compilador do Slint passa no build), a
detecção de ambiente funciona e o `catalog.json` é lido (18 skills). **Não foi possível
ver a janela** — sem captura de tela, o acabamento visual é o descrito no código, não
confirmado a olho.

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

Pra virar substituto do hub atual, faltaria portar (fora do escopo do spike):

1. **Abas Overdev e Grafo** — Overdev é lista/seções (fácil no Slint); o **Grafo
   force-directed** (física + pan/zoom + hit-test + labels) precisaria de `Path`/canvas ou
   uma textura renderizada por fora (o custo real da migração está aqui).
2. **Picker de pasta nativo** — hoje `rfd` (portal XDG). `rfd` é agnóstico de toolkit,
   então integra com o event loop do Slint sem GTK.
3. **Self-update + notificações de desktop** (`notify-rust`, `pkexec`, relançar binário) —
   independentes do toolkit; portam quase 1:1.
4. **i18n 11 locales** — migrar do `i18n::t()` caseiro para `@tr()`/gettext do Slint (ou
   manter o `t()` e só injetar as strings via propriedades).
5. **Instalação real** — reconectar `skills::install/remove`, `installed_version`,
   `resolve_latest`, execução paralela e o modelo de progresso/estado.
6. **Fontes CJK/árabe/etc.** — o egui injeta fallbacks à mão pra matar "tofu"; no Slint,
   via fontconfig, o sistema já resolve (com os pacotes de fonte instalados).

## Veredito (resumido)

O Slint **entrega o salto visual e um código de UI mais limpo/manutenível**, mas **cobra
em deps de build (fontconfig/pkg-config, cmake se skia) e tamanho de binário**, e a **aba
Grafo é o item caro** de portar. Recomendação: **não** reescrever tudo de uma vez — se
formos adiante, migrar **incrementalmente** começando pela aba Skills (esta tela), medindo
tamanho/atrito de empacotamento .deb/.rpm num alvo real antes de comprometer as abas
Overdev/Grafo. Ver o relato do spike para o veredito detalhado.
