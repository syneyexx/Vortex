//! Inference-metingen: wachttijd, TTFT, prefill/decode t/s, inter-token-tijd, laadtijd.
//! NaN/inf in uitvoer of timings = runtimefout.
// VX:W02.15

use std::time::Instant;

use serde::{Deserialize, Serialize};
use vortex_types::{Event, EventKind, RunId, TraceId};

use crate::error::{ModelError, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InferenceMetrics {
    pub model: String,
    /// Verzoek verstuurd → eerste byte van antwoord (wachtrij + prefill bij niet-streamen).
    pub wait_ms: f64,
    /// Verzoek verstuurd → eerste token.
    pub ttft_ms: Option<f64>,
    pub prefill_tps: Option<f64>,
    pub decode_tps: Option<f64>,
    pub inter_token_ms_avg: Option<f64>,
    pub inter_token_ms_max: Option<f64>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_ms: f64,
    pub peak_vram_mib: Option<u64>,
}

impl InferenceMetrics {
    /// Controleer op NaN/inf.
    pub fn validate(&self) -> Result<()> {
        for (n, v) in [
            ("ttft", self.ttft_ms),
            ("prefill_tps", self.prefill_tps),
            ("decode_tps", self.decode_tps),
            ("inter_token_avg", self.inter_token_ms_avg),
        ] {
            if let Some(x) = v
                && !x.is_finite()
            {
                return Err(ModelError::Numeric(format!("{n} = {x}")));
            }
        }
        Ok(())
    }

    /// Als event naar het log (→ tracing, dashboards, kostenmodel van de Dirigent).
    pub fn to_event(&self, trace: TraceId, run: Option<RunId>) -> Event {
        let mut e = Event::new(EventKind::InferenceMeasured, trace, serde_json::to_value(self).unwrap_or_default());
        e.run_id = run;
        e
    }
}

/// Meet tijdens een (stream-)verzoek.
#[derive(Debug)]
pub struct MetricsRecorder {
    start: Instant,
    first_byte: Option<Instant>,
    first_token: Option<Instant>,
    last_token: Option<Instant>,
    gaps_sum: f64,
    gaps_max: f64,
    gaps_n: u64,
    pub m: InferenceMetrics,
}

impl MetricsRecorder {
    pub fn start(model: &str) -> Self {
        Self {
            start: Instant::now(),
            first_byte: None,
            first_token: None,
            last_token: None,
            gaps_sum: 0.0,
            gaps_max: 0.0,
            gaps_n: 0,
            m: InferenceMetrics { model: model.into(), ..Default::default() },
        }
    }
    pub fn first_byte(&mut self) {
        if self.first_byte.is_none() {
            self.first_byte = Some(Instant::now());
        }
    }
    pub fn token(&mut self) {
        let now = Instant::now();
        if self.first_token.is_none() {
            self.first_token = Some(now);
        }
        if let Some(prev) = self.last_token {
            let g = now.duration_since(prev).as_secs_f64() * 1000.0;
            self.gaps_sum += g;
            self.gaps_max = self.gaps_max.max(g);
            self.gaps_n += 1;
        }
        self.last_token = Some(now);
    }
    /// Server-timings van llama-server (`timings` in het laatste chunk) overnemen.
    pub fn server_timings(&mut self, t: &serde_json::Value) {
        if let Some(v) = t.get("prompt_per_second").and_then(|v| v.as_f64()) {
            self.m.prefill_tps = Some(v);
        }
        if let Some(v) = t.get("predicted_per_second").and_then(|v| v.as_f64()) {
            self.m.decode_tps = Some(v);
        }
        if let Some(v) = t.get("prompt_n").and_then(|v| v.as_u64()) {
            self.m.prompt_tokens = v;
        }
        if let Some(v) = t.get("predicted_n").and_then(|v| v.as_u64()) {
            self.m.completion_tokens = v;
        }
    }
    pub fn finish(mut self) -> Result<InferenceMetrics> {
        let ms = |i: Option<Instant>| i.map(|i| i.duration_since(self.start).as_secs_f64() * 1000.0);
        self.m.wait_ms = ms(self.first_byte).unwrap_or_default();
        self.m.ttft_ms = ms(self.first_token);
        self.m.total_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if self.gaps_n > 0 {
            self.m.inter_token_ms_avg = Some(self.gaps_sum / self.gaps_n as f64);
            self.m.inter_token_ms_max = Some(self.gaps_max);
        }
        if self.m.decode_tps.is_none()
            && let (Some(a), Some(b)) = (self.first_token, self.last_token)
            && self.gaps_n > 0
        {
            let secs = b.duration_since(a).as_secs_f64();
            if secs > 0.0 {
                self.m.decode_tps = Some(self.gaps_n as f64 / secs);
            }
        }
        self.m.validate()?;
        Ok(self.m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_is_runtime_error() {
        let m = InferenceMetrics { decode_tps: Some(f64::NAN), ..Default::default() };
        assert!(matches!(m.validate(), Err(ModelError::Numeric(_))));
    }

    #[test]
    fn recorder_measures_ttft_and_gaps() -> Result<()> {
        let mut r = MetricsRecorder::start("m");
        r.first_byte();
        r.token();
        std::thread::sleep(std::time::Duration::from_millis(5));
        r.token();
        r.server_timings(
            &serde_json::json!({"prompt_per_second": 900.0, "predicted_per_second": 95.0, "prompt_n": 12}),
        );
        let m = r.finish()?;
        assert!(m.ttft_ms.is_some());
        assert!(m.inter_token_ms_avg.unwrap_or_default() >= 4.0);
        assert_eq!(m.decode_tps, Some(95.0));
        assert_eq!(m.prompt_tokens, 12);
        Ok(())
    }
}
