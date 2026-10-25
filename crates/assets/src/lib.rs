//! Asset filesystem helpers + optional hot-reload via `notify`.
//!
//! Asset paths are relative to a base directory (defaults to `assets/` next to the executable
//! or the current working directory). Reads return raw bytes; decoding is the caller's job —
//! `game_toolkit_gfx` decodes PNGs, `game_toolkit_audio` decodes ogg/wav, etc.
//!
//! Hot-reload is opt-in: call [`Assets::watch`] to start a `notify` watcher on the asset root,
//! then drain change events each frame with [`Assets::drain_changes`].
//!
//! # Web
//!
//! The browser has no filesystem and can only fetch asynchronously, so on `wasm32` the asset
//! root is a URL prefix relative to the page and [`Assets::read`] serves bytes from memory. The
//! toolkit's core fills that store with `Assets::preload` before `Game::init` runs, from the
//! list in `manifest.txt` (one asset path per line) under the asset root. Hot-reload is not
//! available there: [`Assets::watch`] is a logged no-op.
//!
//! A file missing from the manifest only fails in the browser, since native builds read from
//! disk. To catch that during development, native debug builds load `manifest.txt` too (if
//! the asset root has one) and log a warning, once per file, when [`Assets::read`] serves a
//! file the manifest doesn't list.
//!
//! On every platform, bytes added with [`Assets::insert`] take precedence over the filesystem.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Component;
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{Receiver, channel};

#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[cfg(target_arch = "wasm32")]
mod web;

/// Name of the asset list the web build preloads, relative to the asset root.
pub const MANIFEST_FILE: &str = "manifest.txt";

pub struct Assets {
    root: PathBuf,
    /// In-memory files keyed by resolved path; consulted before the filesystem.
    mem: HashMap<PathBuf, Vec<u8>>,
    #[cfg(not(target_arch = "wasm32"))]
    watcher: Option<RecommendedWatcher>,
    #[cfg(not(target_arch = "wasm32"))]
    rx: Option<Receiver<PathBuf>>,
    pending: HashSet<PathBuf>,
    /// Native debug builds: the web preload list, to warn about reads it doesn't cover.
    #[cfg(not(target_arch = "wasm32"))]
    web_list: Option<WebList>,
}

/// The files a web build would preload, and the unlisted ones already warned about.
#[cfg(not(target_arch = "wasm32"))]
struct WebList {
    /// Where the list came from, and how to fix a missing entry, for the warning.
    source: String,
    hint: &'static str,
    listed: HashSet<PathBuf>,
    warned: Mutex<HashSet<PathBuf>>,
}

/// Parse a `manifest.txt`: one path per line, blank lines and `#` comments ignored.
pub fn parse_manifest(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Drop `.` components, so `./a.png` and `a.png` compare equal.
#[cfg(not(target_arch = "wasm32"))]
fn normalize(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

impl Assets {
    /// Create with `root` as the asset directory. Path is canonicalized eagerly to make
    /// matching against `notify` events deterministic. On the web, `root` is a URL path
    /// relative to the page and is used as-is.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        #[cfg(not(target_arch = "wasm32"))]
        let root = root
            .canonicalize()
            .with_context(|| format!("asset root {} not found", root.display()))?;
        #[cfg(target_arch = "wasm32")]
        let root = root.to_path_buf();
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut assets = Self {
            root,
            mem: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            watcher: None,
            #[cfg(not(target_arch = "wasm32"))]
            rx: None,
            pending: HashSet::new(),
            #[cfg(not(target_arch = "wasm32"))]
            web_list: None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        if cfg!(debug_assertions) {
            assets.load_web_manifest();
        }
        Ok(assets)
    }

    /// Read `manifest.txt` from the asset root, if there is one, as the web preload list.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_web_manifest(&mut self) {
        let path = self.root.join(MANIFEST_FILE);
        if let Ok(text) = std::fs::read_to_string(&path) {
            self.set_web_list(
                path.display().to_string(),
                "regenerate the manifest, e.g. `task assets:manifest`",
                parse_manifest(&text),
            );
        }
    }

    /// Natively, in debug builds: check reads against `paths` instead of `manifest.txt`, for a
    /// game that sets `AppConfig::web_preload`. No-op in release builds and on the web.
    pub fn set_web_preload<I, S>(&mut self, paths: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        #[cfg(not(target_arch = "wasm32"))]
        if cfg!(debug_assertions) {
            let paths = paths.into_iter().map(|p| p.as_ref().to_owned()).collect();
            self.set_web_list(
                "AppConfig::web_preload".into(),
                "add it to that list",
                paths,
            );
        }
        #[cfg(target_arch = "wasm32")]
        let _ = paths;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_web_list(&mut self, source: String, hint: &'static str, paths: Vec<String>) {
        let listed = paths.iter().map(|p| normalize(&self.resolve(p))).collect();
        self.web_list = Some(WebList {
            source,
            hint,
            listed,
            warned: Mutex::new(HashSet::new()),
        });
    }

    /// Natively, in debug builds: warn (once per file) if `rel` isn't in the web preload list,
    /// i.e. loading it would fail in the browser. [`Assets::read`] does this itself; call it
    /// when reading an asset some other way, e.g. by path. Does nothing when there is no
    /// list (no `manifest.txt`), for in-memory files, in release builds and on the web.
    pub fn check_web_listed(&self, rel: impl AsRef<Path>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(list) = &self.web_list else {
                return;
            };
            let p = normalize(&self.resolve(rel.as_ref()));
            if list.listed.contains(&p) || self.mem.contains_key(&p) || !p.starts_with(&self.root) {
                return;
            }
            let mut warned = list
                .warned
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if warned.insert(p) {
                log::warn!(
                    "asset {} is not listed in {}; it loads natively but will fail on the web ({})",
                    rel.as_ref().display(),
                    list.source,
                    list.hint
                );
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = rel;
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

    /// Read an asset's bytes: from memory if it was inserted or preloaded, otherwise from disk
    /// (native only).
    pub fn read(&self, rel: impl AsRef<Path>) -> Result<Vec<u8>> {
        let p = self.resolve(rel.as_ref());
        if let Some(bytes) = self.mem.get(&p) {
            return Ok(bytes.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let bytes = std::fs::read(&p).with_context(|| format!("read {}", p.display()))?;
            self.check_web_listed(rel);
            Ok(bytes)
        }
        #[cfg(target_arch = "wasm32")]
        {
            anyhow::bail!(
                "asset {} was not preloaded; list it in {}/{MANIFEST_FILE}",
                p.display(),
                self.root.display()
            )
        }
    }

    /// Store `bytes` under `rel`, shadowing any file at that path.
    pub fn insert(&mut self, rel: impl AsRef<Path>, bytes: impl Into<Vec<u8>>) {
        let p = self.resolve(rel);
        self.mem.insert(p, bytes.into());
    }

    /// Whether `rel` is available in memory (inserted or preloaded).
    pub fn is_loaded(&self, rel: impl AsRef<Path>) -> bool {
        self.mem.contains_key(&self.resolve(rel))
    }

    /// Start watching the asset root recursively. Idempotent. On the web this logs that
    /// hot-reload is unavailable and returns `Ok`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn watch(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            return Ok(());
        }
        let (tx, rx) = channel::<PathBuf>();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<Event>| match res {
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

    /// Start watching the asset root recursively. Idempotent. On the web this logs that
    /// hot-reload is unavailable and returns `Ok`.
    #[cfg(target_arch = "wasm32")]
    pub fn watch(&mut self) -> Result<()> {
        log::info!("asset hot-reload is not available on the web");
        Ok(())
    }

    /// Drain all queued filesystem changes since last call. Paths are canonicalized when
    /// possible. Idempotent across re-saves within one frame (HashSet-deduped). Always empty
    /// on the web.
    pub fn drain_changes(&mut self) -> Vec<PathBuf> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(rx) = self.rx.as_ref() {
            while let Ok(p) = rx.try_recv() {
                let canon = p.canonicalize().unwrap_or(p);
                self.pending.insert(canon);
            }
        }
        self.pending.drain().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_bytes_shadow_the_filesystem() {
        let mut assets = Assets::new(".").unwrap();
        assert!(!assets.is_loaded("Cargo.toml"));
        assets.insert("Cargo.toml", b"in memory".to_vec());
        assert!(assets.is_loaded("Cargo.toml"));
        assert_eq!(assets.read("Cargo.toml").unwrap(), b"in memory");
    }

    #[test]
    fn parse_manifest_skips_comments_and_blanks() {
        let text = "# header\n\nsprites/a.png\n  b.wav  \n#c.png\n";
        assert_eq!(parse_manifest(text), ["sprites/a.png", "b.wav"]);
    }

    #[test]
    #[cfg(all(not(target_arch = "wasm32"), debug_assertions))]
    fn web_list_flags_unlisted_files_once() {
        let mut assets = Assets::new(".").unwrap();
        assets.set_web_preload(["./Cargo.toml"]);
        let list = assets
            .web_list
            .as_ref()
            .expect("debug build loads the list");
        assert!(
            list.listed
                .contains(&normalize(&assets.resolve("Cargo.toml")))
        );

        assets.check_web_listed("Cargo.toml");
        assets.check_web_listed("src/lib.rs");
        assets.check_web_listed("src/lib.rs");
        let warned = assets.web_list.as_ref().unwrap().warned.lock().unwrap();
        assert_eq!(warned.len(), 1);
        assert!(warned.contains(&assets.resolve("src/lib.rs")));
    }

    #[test]
    fn falls_back_to_disk() {
        let assets = Assets::new(".").unwrap();
        assert!(assets.read("Cargo.toml").unwrap().starts_with(b"[package]"));
        assert!(assets.read("does-not-exist.bin").is_err());
    }
}
