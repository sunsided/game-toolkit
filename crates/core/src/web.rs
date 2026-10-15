//! Browser glue: canvas attachment, console logging and panic reporting.

use anyhow::{Result, anyhow};
use wasm_bindgen::JsCast;
use winit::platform::web::WindowAttributesExtWebSys;
use winit::window::WindowAttributes;

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
