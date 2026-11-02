//! Single-flight (gelijke gelijktijdige verzoeken = één berekening) en negative caching (korte
//! TTL voor "bestaat niet"/fouten zodat we niet blijven herhalen). Beide begrensd.
// VX:W04.07

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::broadcast;

/// Single-flight-groep.
#[derive(Debug)]
pub struct SingleFlight<K, V> {
    inflight: Arc<Mutex<HashMap<K, broadcast::Sender<V>>>>,
}

impl<K, V> Clone for SingleFlight<K, V> {
    fn clone(&self) -> Self {
        Self { inflight: self.inflight.clone() }
    }
}

impl<K: Eq + Hash + Clone + Send + 'static, V: Clone + Send + 'static> Default for SingleFlight<K, V> {
    fn default() -> Self {
        Self { inflight: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl<K: Eq + Hash + Clone + Send + 'static, V: Clone + Send + 'static> SingleFlight<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Voer `f` uit, tenzij dezelfde sleutel al loopt — dan wacht je op dat resultaat.
    /// Geeft `(waarde, gedeeld)` terug.
    pub async fn run<F, Fut>(&self, key: K, f: F) -> (V, bool)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = V>,
    {
        let waiter = {
            let mut m = self.inflight.lock();
            match m.get(&key) {
                Some(tx) => Some(tx.subscribe()),
                None => {
                    let (tx, _) = broadcast::channel(1);
                    m.insert(key.clone(), tx);
                    None
                }
            }
        };
        if let Some(mut rx) = waiter
            && let Ok(v) = rx.recv().await
        {
            return (v, true);
        }
        // Eigenaar (of eigenaar viel weg): zelf berekenen.
        let v = f().await;
        if let Some(tx) = self.inflight.lock().remove(&key) {
            let _ = tx.send(v.clone());
        }
        (v, false)
    }

    pub fn in_flight(&self) -> usize {
        self.inflight.lock().len()
    }
}

/// Negative cache met TTL en maximum aantal sleutels.
#[derive(Debug, Clone)]
pub struct NegativeCache<K> {
    ttl: Duration,
    max: usize,
    inner: Arc<Mutex<HashMap<K, (Instant, String)>>>,
}

impl<K: Eq + Hash + Clone> NegativeCache<K> {
    pub fn new(ttl: Duration, max: usize) -> Self {
        Self { ttl, max: max.max(1), inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Onthoud dat `key` mislukte (met reden).
    pub fn put(&self, key: K, reason: impl Into<String>) {
        let mut m = self.inner.lock();
        if m.len() >= self.max {
            let now = Instant::now();
            m.retain(|_, (t, _)| now.duration_since(*t) < self.ttl);
            if m.len() >= self.max
                && let Some(oldest) = m.iter().min_by_key(|(_, (t, _))| *t).map(|(k, _)| k.clone())
            {
                m.remove(&oldest);
            }
        }
        m.insert(key, (Instant::now(), reason.into()));
    }

    /// Reden als `key` recent mislukte.
    pub fn get(&self, key: &K) -> Option<String> {
        let mut m = self.inner.lock();
        match m.get(key) {
            Some((t, r)) if t.elapsed() < self.ttl => Some(r.clone()),
            Some(_) => {
                m.remove(key);
                None
            }
            None => None,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn concurrent_same_key_computes_once() {
        let sf: SingleFlight<String, u32> = SingleFlight::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut hs = Vec::new();
        for _ in 0..10 {
            let sf = sf.clone();
            let c = calls.clone();
            hs.push(tokio::spawn(async move {
                sf.run("embed:abc".into(), || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    42
                })
                .await
            }));
        }
        let mut shared = 0;
        for h in hs {
            let (v, s) = h.await.unwrap_or((0, false));
            assert_eq!(v, 42);
            shared += s as usize;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(shared, 9);
        assert_eq!(sf.in_flight(), 0);
    }

    #[tokio::test]
    async fn negative_cache_expires_and_is_bounded() {
        let n: NegativeCache<&str> = NegativeCache::new(Duration::from_millis(20), 2);
        n.put("a", "404");
        assert_eq!(n.get(&"a").as_deref(), Some("404"));
        n.put("b", "x");
        n.put("c", "y");
        assert!(n.len() <= 2);
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(n.get(&"c").is_none());
    }
}
