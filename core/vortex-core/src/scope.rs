//! Structured concurrency: subtaken leven binnen de scope van hun ouder en worden bij het
//! einde opgeruimd. Losgekoppelde achtergrondtaken krijgen expliciet een eigenaar.
// VX:W01.05

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::task::{AbortHandle, JoinSet};
use tokio_util::sync::CancellationToken;

/// Scope voor subtaken. Bij `shutdown` (of drop) worden alle kinderen geannuleerd/afgebroken.
#[derive(Debug)]
pub struct TaskScope<T: 'static> {
    name: String,
    token: CancellationToken,
    set: JoinSet<T>,
}

impl<T: Send + 'static> TaskScope<T> {
    /// Nieuwe scope onder `parent` (stoppen van de ouder stopt deze scope).
    pub fn new(name: impl Into<String>, parent: &CancellationToken) -> Self {
        Self { name: name.into(), token: parent.child_token(), set: JoinSet::new() }
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Start een subtaak; die krijgt een child-token van deze scope.
    pub fn spawn<F, Fut>(&mut self, f: F) -> AbortHandle
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
    {
        let fut = f(self.token.child_token());
        self.set.spawn(fut)
    }

    /// Wacht op de volgende afgeronde subtaak.
    pub async fn join_next(&mut self) -> Option<Result<T, tokio::task::JoinError>> {
        self.set.join_next().await
    }

    /// Wacht op alle subtaken (in afrondingsvolgorde).
    pub async fn join_all(mut self) -> Vec<Result<T, tokio::task::JoinError>> {
        let mut out = Vec::with_capacity(self.set.len());
        while let Some(r) = self.set.join_next().await {
            out.push(r);
        }
        out
    }

    /// Stop netjes: annuleer, wacht `grace` op coöperatief stoppen, breek de rest af.
    /// Geeft het aantal hard afgebroken taken terug.
    pub async fn shutdown(mut self, grace: Duration) -> usize {
        self.token.cancel();
        let deadline = tokio::time::Instant::now() + grace;
        while !self.set.is_empty() {
            match tokio::time::timeout_at(deadline, self.set.join_next()).await {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => {
                    let n = self.set.len();
                    tracing::warn!(scope = %self.name, aborted = n, "subtaken stopten niet op tijd; afgebroken");
                    self.set.abort_all();
                    while self.set.join_next().await.is_some() {}
                    return n;
                }
            }
        }
        0
    }
}

impl<T: 'static> Drop for TaskScope<T> {
    fn drop(&mut self) {
        // Geen weeskinderen: alles wat nog loopt stopt mee met de scope.
        self.token.cancel();
        self.set.abort_all();
    }
}

/// Register van losgekoppelde achtergrondtaken, elk met een eigenaar.
#[derive(Debug, Clone, Default)]
pub struct DetachedTasks {
    inner: Arc<Mutex<HashMap<u64, (String, AbortHandle)>>>,
    next: Arc<std::sync::atomic::AtomicU64>,
}

impl DetachedTasks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start een losgekoppelde taak met eigenaar; verdwijnt uit het register als hij klaar is.
    pub fn spawn<F>(&self, owner: impl Into<String>, fut: F) -> u64
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let map = self.inner.clone();
        let mut guard = self.inner.lock();
        let handle = tokio::spawn(async move {
            fut.await;
            map.lock().remove(&id);
        });
        guard.insert(id, (owner.into(), handle.abort_handle()));
        id
    }

    /// Eigenaars van taken die nog lopen.
    pub fn owners(&self) -> Vec<String> {
        self.inner.lock().values().map(|(o, _)| o.clone()).collect()
    }

    /// Stop alle taken van één eigenaar.
    pub fn abort_owner(&self, owner: &str) -> usize {
        let mut m = self.inner.lock();
        let ids: Vec<u64> = m.iter().filter(|(_, (o, _))| o == owner).map(|(k, _)| *k).collect();
        for id in &ids {
            if let Some((_, h)) = m.remove(id) {
                h.abort();
            }
        }
        ids.len()
    }

    /// Stop alles (afsluiten).
    pub fn abort_all(&self) -> usize {
        let mut m = self.inner.lock();
        let n = m.len();
        for (_, (_, h)) in m.drain() {
            h.abort();
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn cancelling_parent_cancels_children() {
        let root = CancellationToken::new();
        let mut scope: TaskScope<bool> = TaskScope::new("t", &root);
        scope.spawn(|tok| async move {
            tok.cancelled().await;
            true
        });
        root.cancel();
        let res = scope.join_all().await;
        assert_eq!(res.len(), 1);
        assert!(matches!(res[0], Ok(true)));
    }

    #[tokio::test]
    async fn shutdown_aborts_uncooperative_tasks() {
        let root = CancellationToken::new();
        let mut scope: TaskScope<()> = TaskScope::new("t", &root);
        scope.spawn(|_tok| async move { std::future::pending::<()>().await });
        let aborted = scope.shutdown(Duration::from_millis(20)).await;
        assert_eq!(aborted, 1);
    }

    #[tokio::test]
    async fn dropping_scope_stops_children() {
        let flag = Arc::new(AtomicBool::new(false));
        {
            let root = CancellationToken::new();
            let mut scope: TaskScope<()> = TaskScope::new("t", &root);
            let f = flag.clone();
            scope.spawn(move |_| async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                f.store(true, Ordering::SeqCst);
            });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!flag.load(Ordering::SeqCst), "kind mag de scope niet overleven");
    }

    #[tokio::test]
    async fn detached_tasks_have_owner_and_can_be_aborted() {
        let d = DetachedTasks::new();
        d.spawn("indexer", std::future::pending());
        d.spawn("indexer", std::future::pending());
        d.spawn("other", async {});
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(d.owners().iter().filter(|o| *o == "indexer").count(), 2);
        assert_eq!(d.abort_owner("indexer"), 2);
        assert!(d.owners().is_empty());
    }
}
