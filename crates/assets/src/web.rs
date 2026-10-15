//! Browser asset loading: fetch files over HTTP into the in-memory store.

use anyhow::{Result, anyhow, bail};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::{Assets, MANIFEST_FILE};

impl Assets {
    /// Fetch every path in `paths` (relative to the asset root) into memory, so later
    /// [`Assets::read`] calls succeed synchronously. All requests are issued before the first
    /// one is awaited, so they download in parallel.
    pub async fn preload<I, S>(&mut self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let window = web_sys::window().ok_or_else(|| anyhow!("no browser window"))?;
        let pending: Vec<(String, js_sys::Promise)> = paths
            .into_iter()
            .map(|rel| {
                let rel = rel.as_ref().to_owned();
                let url = self.url(&rel);
                let promise = window.fetch_with_str(&url);
                (rel, promise)
            })
            .collect();
        for (rel, promise) in pending {
            let bytes = response_bytes(promise, &self.url(&rel)).await?;
            self.insert(rel, bytes);
        }
        Ok(())
    }

    /// Fetch and parse `manifest.txt` under the asset root: one path per line, blank lines
    /// and `#` comments ignored. A missing manifest yields an empty list (with a warning), so
    /// a game without assets needs none.
    pub async fn fetch_manifest(&self) -> Result<Vec<String>> {
        let window = web_sys::window().ok_or_else(|| anyhow!("no browser window"))?;
        let url = self.url(MANIFEST_FILE);
        let bytes = match response_bytes(window.fetch_with_str(&url), &url).await {
            Ok(b) => b,
            Err(e) => {
                log::warn!("no asset manifest, preloading nothing: {e:#}");
                return Ok(Vec::new());
            }
        };
        let text = String::from_utf8(bytes).map_err(|e| anyhow!("{url} is not UTF-8: {e}"))?;
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect())
    }

    fn url(&self, rel: &str) -> String {
        let root = self.root.to_string_lossy();
        let root = root.trim_end_matches('/');
        if root.is_empty() {
            rel.to_owned()
        } else {
            format!("{root}/{rel}")
        }
    }
}

async fn response_bytes(promise: js_sys::Promise, url: &str) -> Result<Vec<u8>> {
    let resp = JsFuture::from(promise)
        .await
        .map_err(|e| anyhow!("fetch {url}: {e:?}"))?;
    let resp: web_sys::Response = resp
        .dyn_into()
        .map_err(|_| anyhow!("fetch {url}: not a Response"))?;
    if !resp.ok() {
        bail!("fetch {url}: HTTP {}", resp.status());
    }
    let buf = resp
        .array_buffer()
        .map_err(|e| anyhow!("fetch {url}: {e:?}"))?;
    let buf = JsFuture::from(buf)
        .await
        .map_err(|e| anyhow!("fetch {url}: {e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}
