//! Begrensde wachtrijen met backpressure en metingen. Er bestaan in Vortex geen onbegrensde kanalen.
// VX:W01.08

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use crate::error::{CoreError, Result};

/// Metingen van een wachtrij (voor drukmeters en tracing).
#[derive(Debug, Default)]
pub struct QueueStats {
    pub sent: AtomicU64,
    pub backpressured: AtomicU64,
    pub rejected: AtomicU64,
}

/// Zender met backpressure.
#[derive(Debug)]
pub struct BoundedSender<T> {
    name: Arc<str>,
    tx: mpsc::Sender<T>,
    stats: Arc<QueueStats>,
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        Self { name: self.name.clone(), tx: self.tx.clone(), stats: self.stats.clone() }
    }
}

/// Maak een begrensde wachtrij. `capacity` moet > 0 zijn.
pub fn bounded<T>(name: &str, capacity: usize) -> (BoundedSender<T>, mpsc::Receiver<T>) {
    let (tx, rx) = mpsc::channel(capacity.max(1));
    (BoundedSender { name: name.into(), tx, stats: Arc::new(QueueStats::default()) }, rx)
}

impl<T> BoundedSender<T> {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
    /// Huidige vulling (0..=capacity).
    pub fn depth(&self) -> usize {
        self.tx.max_capacity() - self.tx.capacity()
    }
    pub fn capacity(&self) -> usize {
        self.tx.max_capacity()
    }

    /// Versturen; wacht als de wachtrij vol is (producent vertraagt = backpressure).
    pub async fn send(&self, v: T) -> Result<()> {
        if self.tx.capacity() == 0 {
            self.stats.backpressured.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(queue = %self.name, "backpressure: wachtrij vol, producent wacht");
        }
        self.tx.send(v).await.map_err(|_| CoreError::QueueFull(format!("{} gesloten", self.name)))?;
        self.stats.sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Niet wachten: vol = fout (voor P0-paden die nooit mogen blokkeren).
    pub fn try_send(&self, v: T) -> Result<()> {
        match self.tx.try_send(v) {
            Ok(()) => {
                self.stats.sent.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.stats.rejected.fetch_add(1, Ordering::Relaxed);
                Err(CoreError::QueueFull(self.name.to_string()))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(CoreError::QueueFull(format!("{} gesloten", self.name))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn full_queue_applies_backpressure() -> Result<()> {
        let (tx, mut rx) = bounded::<u32>("t", 2);
        tx.send(1).await?;
        tx.send(2).await?;
        assert_eq!(tx.depth(), 2);
        assert!(tx.try_send(3).is_err());
        let t2 = tx.clone();
        let h = tokio::spawn(async move { t2.send(3).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!h.is_finished(), "producent moet wachten");
        assert_eq!(rx.recv().await, Some(1));
        h.await.map_err(|e| CoreError::Config(e.to_string()))??;
        assert_eq!(tx.stats().backpressured.load(Ordering::Relaxed), 1);
        assert_eq!(tx.stats().rejected.load(Ordering::Relaxed), 1);
        Ok(())
    }
}
