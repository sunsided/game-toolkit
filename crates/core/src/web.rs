//! Browser glue: canvas attachment and detach watching, the autoplay probe, console logging and
//! panic reporting.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use game_toolkit_audio::DetachHandle;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use winit::platform::web::{WindowAttributesExtWebSys, WindowExtWebSys};
use winit::window::{Window, WindowAttributes};

/// Render into the canvas with `canvas_id`, or append a fresh canvas to `<body>`.
pub(crate) fn canvas_attributes(
    attrs: WindowAttributes,
    canvas_id: Option<&str>,
) -> Result<WindowAttributes> {
    let Some(id) = canvas_id else {
        return Ok(attrs.with_append(true));
    };
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .ok_or_else(|| anyhow!("no element with id {id:?}"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| anyhow!("element {id:?} is not a <canvas>"))?;
    Ok(attrs.with_canvas(Some(canvas)))
}

/// How long a probe may stay `suspended` before the answer counts as no. A fresh context
/// reports `suspended` at first in Firefox and WebKit even where autoplay is allowed, and
/// turns `running` about 300 ms later; Chromium is `running` at once.
const AUTOPLAY_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// What an [`AutoplayProbe`] found out so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Autoplay {
    /// Audio may start now, without another gesture.
    Allowed,
    /// The browser wants a gesture first.
    Denied,
    /// No answer yet; poll again next frame.
    Pending,
}

/// Asks the browser whether audio may start without another user gesture: the page already
/// had one (the player clicked a link on the host site to get to the game), so the toolkit can
/// open the audio output without waiting for a click on the canvas.
///
/// A throwaway `AudioContext` starts `running` only when autoplay is allowed. It never
/// connects a node, so nothing hears it, and it is closed when the probe is dropped. Where
/// the browser tracks user activation (`navigator.userActivation`), a probe only starts after
/// a gesture: before one, the answer is almost always no, and Chrome logs a warning for every
/// context created then.
pub(crate) struct AutoplayProbe {
    context: web_sys::AudioContext,
    /// When the probe started.
    since: web_time::Instant,
}

impl AutoplayProbe {
    /// Start a probe, or `None` when the page had no gesture yet or no context can be made.
    pub(crate) fn start() -> Option<Self> {
        let window = web_sys::window()?;
        let navigator = window.navigator();
        // Older Safari has no `userActivation`; probe there without the shortcut.
        let activation = js_sys::Reflect::get(&navigator, &"userActivation".into())
            .ok()
            .and_then(|v| v.dyn_into::<web_sys::UserActivation>().ok());
        if activation.is_some_and(|a| !a.has_been_active()) {
            return None;
        }
        let context = web_sys::AudioContext::new().ok()?;
        Some(Self {
            context,
            since: web_time::Instant::now(),
        })
    }

    /// `Allowed` once the probe runs, `Denied` if it is still suspended after
    /// [`AUTOPLAY_PROBE_TIMEOUT`], `Pending` in between.
    pub(crate) fn poll(&self) -> Autoplay {
        if self.context.state() == web_sys::AudioContextState::Running {
            Autoplay::Allowed
        } else if self.since.elapsed() > AUTOPLAY_PROBE_TIMEOUT {
            Autoplay::Denied
        } else {
            Autoplay::Pending
        }
    }
}

impl Drop for AutoplayProbe {
    fn drop(&mut self) {
        let _ = self.context.close();
    }
}

/// Route panics and `log` records to the browser console. Leaves an already-installed logger
/// alone, so a game can bring its own.
pub(crate) fn install_hooks() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&info.to_string().into());
    }));
    if log::set_logger(&CONSOLE_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

static CONSOLE_LOGGER: ConsoleLogger = ConsoleLogger;

struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // wgpu and naga are chatty at info; keep them to warnings unless asked for more.
        let target = record.target();
        if record.level() > log::Level::Warn
            && (target.starts_with("wgpu") || target.starts_with("naga"))
        {
            return;
        }
        let msg = format!("[{}] {}", target, record.args()).into();
        match record.level() {
            log::Level::Error => web_sys::console::error_1(&msg),
            log::Level::Warn => web_sys::console::warn_1(&msg),
            log::Level::Info => web_sys::console::info_1(&msg),
            log::Level::Debug | log::Level::Trace => web_sys::console::debug_1(&msg),
        }
    }

    fn flush(&self) {}
}

/// Watches the game's canvas being taken out of the page and put back (an SPA route change,
/// a component unmounting). The browser delivers no frames to a detached canvas, so this
/// can't wait for one: a `MutationObserver` on the document suspends the audio output from
/// the DOM callback itself, and queues the change for the game's next frame.
pub(crate) struct CanvasWatch {
    observer: web_sys::MutationObserver,
    _callback: Closure<dyn FnMut()>,
    state: Rc<RefCell<WatchState>>,
}

struct WatchState {
    attached: bool,
    /// Attachment changes not yet delivered to the game, oldest first.
    changes: Vec<bool>,
}

impl CanvasWatch {
    /// Start watching `window`'s canvas. `audio` is suspended while the canvas is detached.
    pub(crate) fn new(window: &Window, audio: Option<DetachHandle>) -> Result<Self> {
        // Hold on to the element itself: once detached, `getElementById` can't find it.
        let canvas = window
            .canvas()
            .ok_or_else(|| anyhow!("window has no canvas"))?;
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| anyhow!("no document"))?;
        let state = Rc::new(RefCell::new(WatchState {
            attached: canvas.is_connected(),
            changes: Vec::new(),
        }));
        if let Some(audio) = &audio {
            audio.set_detached(!state.borrow().attached);
        }
        let callback = Closure::<dyn FnMut()>::new({
            let state = state.clone();
            move || {
                let attached = canvas.is_connected();
                let mut state = state.borrow_mut();
                if state.attached == attached {
                    return;
                }
                state.attached = attached;
                state.changes.push(attached);
                if let Some(audio) = &audio {
                    audio.set_detached(!attached);
                }
            }
        });
        let observer = web_sys::MutationObserver::new(callback.as_ref().unchecked_ref())
            .map_err(|e| anyhow!("MutationObserver: {e:?}"))?;
        let init = web_sys::MutationObserverInit::new();
        init.set_child_list(true);
        init.set_subtree(true);
        observer
            .observe_with_options(&document, &init)
            .map_err(|e| anyhow!("MutationObserver.observe: {e:?}"))?;
        Ok(Self {
            observer,
            _callback: callback,
            state,
        })
    }

    /// Attachment changes since the last call, oldest first (`true`: put back in the page).
    pub(crate) fn take_changes(&self) -> Vec<bool> {
        std::mem::take(&mut self.state.borrow_mut().changes)
    }
}

impl Drop for CanvasWatch {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}
