//! Tracing: één trace-ID per run door alle lagen (GUI → kern → agents → tools → processen).
//! Spans dragen `trace_id`/`span_id`; events in het log dragen dezelfde ID's.
// VX:W01.16

use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use vortex_types::{RunId, SpanId, TraceId};

/// Trace-context die door de lagen heen wordt meegegeven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceCtx {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub run_id: Option<RunId>,
}

impl TraceCtx {
    pub fn root(run_id: Option<RunId>) -> Self {
        Self { trace_id: TraceId::new(), span_id: SpanId::new(), run_id }
    }
    /// Kind-span binnen dezelfde trace.
    pub fn child(&self) -> Self {
        Self { trace_id: self.trace_id, span_id: SpanId::new(), run_id: self.run_id }
    }
    /// `tracing`-span met de ID's als velden.
    pub fn span(&self, name: &'static str) -> tracing::Span {
        tracing::info_span!("vx", op = name, trace_id = %self.trace_id, span_id = %self.span_id,
            run_id = self.run_id.map(|r| r.to_string()).unwrap_or_default())
    }
}

/// Logformaat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

/// Installeer de globale subscriber (één keer per proces). Filter via `VORTEX_LOG` (standaard `info`).
pub fn init(format: LogFormat) {
    let filter = EnvFilter::try_from_env("VORTEX_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let reg = tracing_subscriber::registry().with(filter);
    let res = match format {
        LogFormat::Pretty => reg.with(fmt::layer().with_target(false)).try_init(),
        LogFormat::Json => reg.with(fmt::layer().json().with_current_span(true).with_span_list(false)).try_init(),
    };
    if res.is_err() {
        // Al geïnstalleerd (bv. in tests): geen fout.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_keeps_trace_and_gets_new_span() {
        let r = TraceCtx::root(Some(RunId::new()));
        let c = r.child();
        assert_eq!(r.trace_id, c.trace_id);
        assert_ne!(r.span_id, c.span_id);
        assert_eq!(r.run_id, c.run_id);
    }
}
