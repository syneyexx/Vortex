//! Online of offline? (W09.04) — korte check, 60 s gecachet. Offline-first: lokaal altijd,
//! web alleen als er verbinding is én een zoekbron is ingesteld.

use std::time::{Duration, Instant};

use parking_lot::Mutex;

#[derive(Debug)]
pub struct Connectivity {
    probe_url: Option<String>,
    cache: Mutex<Option<(Instant, bool)>>,
    forced: Mutex<Option<bool>>,
}

impl Connectivity {
    /// `probe_url` = bv. de SearXNG-URL. Geen URL = altijd offline (geen webbron).
    pub fn new(probe_url: Option<String>) -> Self {
        Self { probe_url, cache: Mutex::new(None), forced: Mutex::new(None) }
    }

    /// Voor tests en de "offline modus"-schakelaar.
    pub fn force(&self, online: Option<bool>) {
        *self.forced.lock() = online;
    }

    pub async fn online(&self) -> bool {
        if let Some(f) = *self.forced.lock() {
            return f;
        }
        let Some(url) = &self.probe_url else { return false };
        if let Some((t, v)) = *self.cache.lock()
            && t.elapsed() < Duration::from_secs(60)
        {
            return v;
        }
        let ok = match reqwest::Client::builder().timeout(Duration::from_secs(2)).build() {
            Ok(c) => c.get(url).send().await.is_ok(),
            Err(_) => false,
        };
        *self.cache.lock() = Some((Instant::now(), ok));
        ok
    }
}
