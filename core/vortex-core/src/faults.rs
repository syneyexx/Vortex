//! Fault injection (W17.06): TCP-proxy tussen Vortex en een dienst (llama-server, worker,
//! MCP-server) die op commando fouten maakt — verbinding weigeren, vertragen, afkappen na N
//! bytes, bytes verminken — zodat herstel per component getest kan worden. Kleine tests per
//! component draaien altijd; de volledige matrix (`vortex faults`) is 🔒 een grote test.
// VX:W17.06

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "arg")]
pub enum FaultMode {
    Pass,
    /// Elke chunk `ms` vertragen (trage dienst).
    Delay(u64),
    /// Na zoveel bytes van upstream de verbinding verbreken (verbinding weg midden in een stream).
    CutAfter(usize),
    /// Bytes van upstream verminken (kapotte uitvoer).
    Garble,
    /// Verbinding direct sluiten (dienst weg).
    Refuse,
}

#[derive(Debug, Clone)]
pub struct FaultProxy {
    pub addr: std::net::SocketAddr,
    mode: Arc<Mutex<FaultMode>>,
    stop: CancellationToken,
}

impl FaultProxy {
    pub async fn start(upstream: &str) -> std::io::Result<Self> {
        let l = TcpListener::bind("127.0.0.1:0").await?;
        let addr = l.local_addr()?;
        let mode = Arc::new(Mutex::new(FaultMode::Pass));
        let stop = CancellationToken::new();
        let (m, st, up) = (mode.clone(), stop.clone(), upstream.to_string());
        tokio::spawn(async move {
            loop {
                let conn = tokio::select! {
                    _ = st.cancelled() => break,
                    c = l.accept() => c,
                };
                let Ok((client, _)) = conn else { continue };
                let (m, up) = (m.clone(), up.clone());
                tokio::spawn(async move {
                    let _ = handle(client, &up, m).await;
                });
            }
        });
        Ok(Self { addr, mode, stop })
    }

    pub fn set(&self, m: FaultMode) {
        *self.mode.lock() = m;
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn stop(&self) {
        self.stop.cancel();
    }
}

async fn handle(mut client: TcpStream, upstream: &str, mode: Arc<Mutex<FaultMode>>) -> std::io::Result<()> {
    let m = *mode.lock();
    if m == FaultMode::Refuse {
        return client.shutdown().await;
    }
    let mut server = TcpStream::connect(upstream).await?;
    let (mut cr, mut cw) = client.split();
    let (mut sr, mut sw) = server.split();
    let up = async {
        let mut buf = [0u8; 8192];
        loop {
            let n = cr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            sw.write_all(&buf[..n]).await?;
        }
        sw.shutdown().await
    };
    let down = async {
        let mut buf = [0u8; 8192];
        let mut sent = 0usize;
        loop {
            let n = sr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            let cur = *mode.lock();
            match cur {
                FaultMode::Delay(ms) => tokio::time::sleep(Duration::from_millis(ms)).await,
                FaultMode::Garble => {
                    for b in buf[..n].iter_mut().skip(n / 3) {
                        *b = b.wrapping_mul(31).wrapping_add(7);
                    }
                }
                _ => {}
            }
            if let FaultMode::CutAfter(limit) = cur
                && sent + n > limit
            {
                let k = limit.saturating_sub(sent);
                cw.write_all(&buf[..k]).await?;
                return cw.shutdown().await;
            }
            cw.write_all(&buf[..n]).await?;
            sent += n;
        }
        cw.shutdown().await
    };
    let _ = tokio::join!(up, down);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn modes() -> std::io::Result<()> {
        // Echo-server die 100 bytes terugstuurt.
        let l = TcpListener::bind("127.0.0.1:0").await?;
        let up = l.local_addr()?.to_string();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 16];
                    let _ = s.read(&mut b).await;
                    let _ = s.write_all(&[b'a'; 100]).await;
                });
            }
        });
        let p = FaultProxy::start(&up).await?;
        let get = |p: FaultProxy| async move {
            let mut c = TcpStream::connect(p.addr).await?;
            c.write_all(b"hoi").await?;
            let mut out = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(2), c.read_to_end(&mut out)).await;
            Ok::<_, std::io::Error>(out)
        };
        assert_eq!(get(p.clone()).await?.len(), 100);
        p.set(FaultMode::CutAfter(30));
        assert_eq!(get(p.clone()).await?.len(), 30);
        p.set(FaultMode::Garble);
        let g = get(p.clone()).await?;
        assert!(g.len() == 100 && g.iter().any(|b| *b != b'a'));
        p.set(FaultMode::Refuse);
        assert!(get(p.clone()).await?.is_empty());
        p.stop();
        Ok(())
    }
}
