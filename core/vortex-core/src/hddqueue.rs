//! HDD-wachtrij (W04.04): één I/O-thread voor de koude schijf; verzoeken worden gebundeld en
//! gesorteerd op (bestand, offset) zodat de kop zo weinig mogelijk heen en weer gaat
//! (elevator). Alleen grote, sequentiële blokken.
// VX:W04.04

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::error::{CoreError, Result};

/// Leesverzoek.
#[derive(Debug)]
struct ReadReq {
    path: PathBuf,
    offset: u64,
    len: usize,
    tx: oneshot::Sender<std::io::Result<Vec<u8>>>,
}

/// Handle naar de HDD-wachtrij.
#[derive(Debug, Clone)]
pub struct HddQueue {
    tx: mpsc::Sender<ReadReq>,
    served: Arc<AtomicU64>,
}

impl HddQueue {
    /// Start de I/O-thread. `batch_window`: hoe lang verzoeken verzameld worden vóór sorteren.
    pub fn start(capacity: usize, batch_window: Duration) -> Self {
        let (tx, mut rx) = mpsc::channel::<ReadReq>(capacity.max(1));
        let served = Arc::new(AtomicU64::new(0));
        let s2 = served.clone();
        std::thread::Builder::new()
            .name("vortex-hdd".into())
            .spawn(move || {
                while let Some(first) = rx.blocking_recv() {
                    let mut batch = vec![first];
                    let deadline = std::time::Instant::now() + batch_window;
                    while std::time::Instant::now() < deadline {
                        match rx.try_recv() {
                            Ok(r) => batch.push(r),
                            Err(mpsc::error::TryRecvError::Empty) => std::thread::sleep(Duration::from_micros(200)),
                            Err(mpsc::error::TryRecvError::Disconnected) => break,
                        }
                    }
                    // Elevator: per bestand, oplopend op offset.
                    let mut by_file: BTreeMap<PathBuf, Vec<ReadReq>> = BTreeMap::new();
                    for r in batch {
                        by_file.entry(r.path.clone()).or_default().push(r);
                    }
                    for (path, mut reqs) in by_file {
                        reqs.sort_by_key(|r| r.offset);
                        let mut file = std::fs::File::open(&path);
                        for r in reqs {
                            let res = match &mut file {
                                Ok(f) => (|| {
                                    f.seek(SeekFrom::Start(r.offset))?;
                                    let mut buf = vec![0u8; r.len];
                                    let n = f.read(&mut buf)?;
                                    buf.truncate(n);
                                    Ok(buf)
                                })(),
                                Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
                            };
                            s2.fetch_add(1, Ordering::Relaxed);
                            let _ = r.tx.send(res);
                        }
                    }
                }
            })
            .map_err(|e| tracing::error!(error = %e, "hdd-thread niet gestart"))
            .ok();
        Self { tx, served }
    }

    /// Lees `len` bytes vanaf `offset` (wacht in de rij; backpressure als die vol is).
    pub async fn read(&self, path: impl Into<PathBuf>, offset: u64, len: usize) -> Result<Vec<u8>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ReadReq { path: path.into(), offset, len, tx })
            .await
            .map_err(|_| CoreError::QueueFull("hdd-wachtrij gesloten".into()))?;
        Ok(rx.await.map_err(|_| CoreError::Cancelled)??)
    }

    pub fn served(&self) -> u64 {
        self.served.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_correct_ranges_in_batches() -> Result<()> {
        let d = tempfile::tempdir()?;
        let p = d.path().join("blok.bin");
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data)?;
        let q = HddQueue::start(64, Duration::from_millis(5));
        let mut hs = Vec::new();
        for off in [9000u64, 10, 5000, 300, 7000] {
            let q = q.clone();
            let p = p.clone();
            hs.push(tokio::spawn(async move { (off, q.read(p, off, 100).await) }));
        }
        for h in hs {
            let (off, r) = h.await.map_err(|e| CoreError::Config(e.to_string()))?;
            let r = r?;
            assert_eq!(r, data[off as usize..off as usize + 100].to_vec());
        }
        assert_eq!(q.served(), 5);
        assert!(q.read(d.path().join("weg"), 0, 1).await.is_err());
        Ok(())
    }
}
