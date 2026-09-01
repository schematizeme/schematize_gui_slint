//! Imports comuns dos módulos da GUI.
//!
//! O quê: um único ponto que reexporta o que praticamente todo módulo desta GUI
//! usa — os tipos GERADOS pelo Slint a partir de `ui/app.slint` (`AppWindow`,
//! `SkillRow`, `OverItem`, …, que nascem no módulo raiz do crate), a API do lib
//! `schematize` e o punhado de tipos de `slint`/`std` que atravessam a fronteira
//! Rust↔UI. Onde: `use crate::prelude::*;` no topo de cada módulo.
//!
//! Por quê: o `slint::include_modules!()` só pode aparecer UMA vez, no raiz. Sem
//! um prelúdio, cada módulo repetiria o mesmo bloco de 15 `use` — ruído que
//! esconde o que o módulo realmente depende de DIFERENTE dos outros.

pub(crate) use crate::*;

pub(crate) use schematize::agentrun;
pub(crate) use schematize::i18n::{self, t, tf};
pub(crate) use schematize::registry::{self, Item};
pub(crate) use schematize::skillsproj;
pub(crate) use schematize::{
    account, autostart, config, database, debugreport, environments, githist, links, market,
    notifications, overdev, overdevdb, panel, projects, selfupdate, settings, skilledit, skills,
    sshkeys, upgrade, usage, util,
};
pub(crate) use slint::{Model, ModelRc, SharedString, TimerMode, VecModel, Weak};
pub(crate) use std::cell::RefCell;
pub(crate) use std::collections::{HashMap, HashSet};
// `process_group` só existe em Unix. Sem a guarda, o build de Windows quebra em
// `cannot find 'unix' in 'os'` — foi o que derrubou o job de release da v0.52.0 depois que o
// CLI passou a compilar. Os chamadores usam `desacopla_processo` (abaixo), não este trait.
#[cfg(unix)]
pub(crate) use std::os::unix::process::CommandExt;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::Stdio;
pub(crate) use std::rc::Rc;
pub(crate) use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use std::time::{Duration, Instant};
