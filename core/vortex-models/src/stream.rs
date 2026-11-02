//! Streaming-hulpen: incrementele UTF-8-decodering, stopsequenties over fragmentgrenzen,
//! en tool-argumenten die pas uitgevoerd mogen worden als ze volledig en geldig zijn.
// VX:W02.14

/// Decodeert bytes in stukken; houdt onvolledige UTF-8-reeksen vast tot de rest binnen is.
#[derive(Debug, Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Voeg bytes toe; geeft de tekst terug die nu compleet is.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    out.push_str(s);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // valid_up_to garandeert geldige UTF-8 tot daar.
                    out.push_str(&String::from_utf8_lossy(&self.pending[..valid]));
                    match e.error_len() {
                        // Onvolledige reeks aan het eind: bewaren.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                        // Echt ongeldige bytes: vervangen en doorgaan.
                        Some(n) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..valid + n);
                        }
                    }
                }
            }
        }
    }

    /// Einde stroom: wat overblijft is ongeldig.
    pub fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        self.pending.clear();
        "\u{FFFD}".into()
    }
}

/// Resultaat van [`StopMatcher::push`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopOutput {
    /// Tekst die veilig getoond kan worden.
    pub text: String,
    /// Welke stopsequentie geraakt werd (stroom is dan klaar).
    pub stopped: Option<String>,
}

/// Herkent stopsequenties ook als ze over meerdere fragmenten verdeeld binnenkomen.
/// Houdt alleen het kleinst mogelijke staartje terug.
#[derive(Debug, Clone)]
pub struct StopMatcher {
    stops: Vec<String>,
    held: String,
    done: bool,
}

impl StopMatcher {
    pub fn new(stops: impl IntoIterator<Item = String>) -> Self {
        Self { stops: stops.into_iter().filter(|s| !s.is_empty()).collect(), held: String::new(), done: false }
    }

    pub fn push(&mut self, fragment: &str) -> StopOutput {
        if self.done {
            return StopOutput { text: String::new(), stopped: None };
        }
        self.held.push_str(fragment);
        // Volledige treffer?
        let mut best: Option<(usize, &String)> = None;
        for s in &self.stops {
            if let Some(i) = self.held.find(s.as_str())
                && best.is_none_or(|(b, _)| i < b)
            {
                best = Some((i, s));
            }
        }
        if let Some((i, s)) = best {
            let text = self.held[..i].to_string();
            let stopped = Some(s.clone());
            self.held.clear();
            self.done = true;
            return StopOutput { text, stopped };
        }
        // Langste staart die het begin van een stopsequentie kan zijn, terughouden.
        let mut keep = 0;
        for s in &self.stops {
            for (k, _) in s.char_indices().skip(1).chain(std::iter::once((s.len(), ' '))) {
                if k <= self.held.len() && k > keep && self.held.ends_with(&s[..k]) && k < s.len() {
                    keep = k;
                }
            }
        }
        let cut = self.held.len() - keep;
        let text = self.held[..cut].to_string();
        self.held.drain(..cut);
        StopOutput { text, stopped: None }
    }

    /// Einde stroom zonder stop: teruggehouden tekst vrijgeven.
    pub fn finish(&mut self) -> String {
        self.done = true;
        std::mem::take(&mut self.held)
    }
}

/// Bouwt de argumenten van één toolaanroep op uit gestreamde stukken.
/// Uitvoeren mag pas na [`ToolArgsAssembler::complete`] (volledig + geldige JSON).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolArgsAssembler {
    pub id: Option<String>,
    pub name: Option<String>,
    buf: String,
}

/// Fase van een gestreamde toolaanroep (voor de GUI: "aan het opstellen" vs "klaar").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolArgsPhase {
    Streaming,
    Complete,
}

impl ToolArgsAssembler {
    pub fn push(&mut self, id: Option<&str>, name: Option<&str>, args_fragment: &str) {
        if let Some(i) = id.filter(|s| !s.is_empty()) {
            self.id = Some(i.to_string());
        }
        if let Some(n) = name.filter(|s| !s.is_empty()) {
            self.name = Some(n.to_string());
        }
        self.buf.push_str(args_fragment);
    }

    pub fn raw(&self) -> &str {
        &self.buf
    }

    /// Geeft (naam, argumenten) alleen als alles compleet en geldig is; anders een fout.
    pub fn complete(&self) -> Result<(String, serde_json::Value), String> {
        let name = self.name.clone().ok_or("toolaanroep zonder naam")?;
        let raw = if self.buf.trim().is_empty() { "{}" } else { self.buf.as_str() };
        let v: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("ongeldige argumenten voor {name}: {e}"))?;
        if !v.is_object() {
            return Err(format!("argumenten voor {name} zijn geen object"));
        }
        Ok((name, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_across_chunks() {
        let s = "héllo 🌀 wereld";
        let b = s.as_bytes();
        let mut d = Utf8Decoder::new();
        let mut out = String::new();
        for chunk in b.chunks(1) {
            out.push_str(&d.push(chunk));
        }
        out.push_str(&d.finish());
        assert_eq!(out, s);
    }

    #[test]
    fn utf8_invalid_bytes_are_replaced_not_stuck() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.push(&[b'a', 0xff, b'b']), "a\u{FFFD}b");
        assert_eq!(d.push(&[0xe2, 0x82]), "");
        assert_eq!(d.finish(), "\u{FFFD}");
    }

    #[test]
    fn stop_sequence_across_fragments() {
        let mut m = StopMatcher::new(["<|end|>".to_string()]);
        let mut shown = String::new();
        for f in ["Hallo <", "|en", "d|> rest"] {
            let o = m.push(f);
            shown.push_str(&o.text);
            if o.stopped.is_some() {
                break;
            }
        }
        assert_eq!(shown, "Hallo ");
    }

    #[test]
    fn partial_prefix_released_when_not_a_stop() {
        let mut m = StopMatcher::new(["STOP".to_string()]);
        let a = m.push("ST");
        assert_eq!(a.text, "");
        let b = m.push("ART");
        assert_eq!(b.text, "START");
        assert_eq!(m.finish(), "");
    }

    #[test]
    fn earliest_stop_wins() {
        let mut m = StopMatcher::new(["B".to_string(), "A".to_string()]);
        let o = m.push("xxAyyB");
        assert_eq!(o.text, "xx");
        assert_eq!(o.stopped.as_deref(), Some("A"));
    }

    #[test]
    fn tool_args_only_complete_when_valid() {
        let mut t = ToolArgsAssembler::default();
        t.push(Some("c1"), Some("read_file"), "{\"pa");
        assert!(t.complete().is_err());
        t.push(None, None, "th\": \"a.txt\"}");
        let (n, v) = t.complete().unwrap_or_default();
        assert_eq!(n, "read_file");
        assert_eq!(v["path"], "a.txt");
        let mut bad = ToolArgsAssembler::default();
        bad.push(None, Some("x"), "[1,2]");
        assert!(bad.complete().is_err(), "geen object = fout, nooit actie");
    }
}
