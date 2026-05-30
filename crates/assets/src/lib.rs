//! Asset filesystem helpers + optional hot-reload via [`notify`].
//!
//! Asset paths are relative to a base directory (defaults to `assets/` next to the executable
//! or the current working directory). Reads return raw bytes; decoding is the caller's job —
//! [`toolkit_gfx`] decodes PNGs, [`toolkit_audio`] decodes ogg/wav, etc.
//!
//! Hot-reload is opt-in: call [`Assets::watch`] to start a `notify` watcher on the asset root,
//! then drain change events each frame with [`Assets::drain_changes`].

#![deny(unsafe_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub struct Assets {
    root: PathBuf,
    watcher: Option<RecommendedWatcher>,
    rx: Option<Receiver<PathBuf>>,
    pending: HashSet<PathBuf>,
}

impl Assets {
    /// Create with `root` as the asset directory. Path is canonicalized eagerly to make
    /// matching against `notify` events deterministic.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let canon = root
            .canonicalize()
            .with_context(|| format!("asset root {} not found", root.display()))?;
        Ok(Self {
            root: canon,
            watcher: None,
            rx: None,
            pending: HashSet::new(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, rel: impl AsRef<Path>) -> PathBuf {
        let rel = rel.as_ref();
        if rel.is_absolute() {
            rel.to_path_buf()
        } else {
            self.root.join(rel)
        }
    }

    pub fn read(&self, rel: impl AsRef<Path>) -> Result<Vec<u8>> {
        let p = self.resolve(rel.as_ref());
        std::fs::read(&p).with_context(|| format!("read {}", p.display()))
    }

    /// Start watching the asset root recursively. Idempotent.
    pub fn watch(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            return Ok(());
        }
        let (tx, rx) = channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| match res {
            Ok(event) => {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    for p in event.paths {
                        let _ = tx.send(p);
                    }
                }
            }
            Err(e) => log::warn!("notify error: {e}"),
        })?;
        watcher.watch(&self.root, RecursiveMode::Recursive)?;
        self.watcher = Some(watcher);
        self.rx = Some(rx);
        Ok(())
    }

    /// Drain all queued filesystem changes since last call. Paths are canonicalized when
    /// possible. Idempotent across re-saves within one frame (HashSet-deduped).
    pub fn drain_changes(&mut self) -> Vec<PathBuf> {
        if let Some(rx) = self.rx.as_ref() {
            while let Ok(p) = rx.try_recv() {
                let canon = p.canonicalize().unwrap_or(p);
                self.pending.insert(canon);
            }
        }
        self.pending.drain().collect()
    }
}
