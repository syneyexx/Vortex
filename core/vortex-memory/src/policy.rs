//! Schrijfpolicy (heuristisch; de importance scorer wordt in W11 een neurale layer): wanneer
//! opslaan, hoe belangrijk, hoe saillant. Elke beslissing wordt met features gelogd.
// VX:W05.04

use serde::Serialize;

use crate::types::MemoryKind;

#[derive(Debug, Clone, Serialize)]
pub struct PolicyFeatures {
    pub chars: usize,
    pub words: usize,
    pub explicit_remember: bool,
    pub personal: bool,
    pub question_only: bool,
    pub small_talk: bool,
    pub has_numbers: bool,
    pub urgent: bool,
}

pub fn features(text: &str) -> PolicyFeatures {
    let l = text.to_lowercase();
    let words = l.split_whitespace().count();
    PolicyFeatures {
        chars: text.chars().count(),
        words,
        explicit_remember: ["onthoud", "vergeet niet", "remember", "noteer", "sla op"].iter().any(|w| l.contains(w)),
        personal: [
            "ik ben", "ik heb", "ik wil", "ik woon", "ik werk", "mijn ", "ik hou", "ik vind", "voorkeur", "i am", "my ",
        ]
        .iter()
        .any(|w| l.contains(w)),
        question_only: l.trim_end().ends_with('?') && words < 25,
        small_talk: words <= 4
            && ["hoi", "hallo", "hey", "dank", "bedankt", "thanks", "ok", "oké", "top", "doei", "goedemorgen"]
                .iter()
                .any(|w| l.contains(w)),
        has_numbers: l.chars().any(|c| c.is_ascii_digit()),
        urgent: ["belangrijk", "urgent", "deadline", "dringend", "asap", "!"].iter().any(|w| l.contains(w)),
    }
}

/// Belang 0..1.
pub fn importance(text: &str, kind: MemoryKind) -> (f64, PolicyFeatures) {
    let f = features(text);
    let mut s: f64 = match kind {
        MemoryKind::User | MemoryKind::Project => 0.6,
        MemoryKind::Semantic | MemoryKind::World => 0.55,
        MemoryKind::Procedural => 0.6,
        MemoryKind::Summary => 0.6,
        MemoryKind::Episodic | MemoryKind::LongTerm => 0.45,
        MemoryKind::ShortTerm | MemoryKind::Meta => 0.3,
    };
    if f.explicit_remember {
        s = 1.0;
    }
    if f.personal {
        s += 0.25;
    }
    if f.has_numbers {
        s += 0.05;
    }
    if f.urgent {
        s += 0.1;
    }
    if f.small_talk {
        s = 0.05;
    }
    if f.chars < 12 && !f.explicit_remember {
        s -= 0.25;
    }
    if f.question_only && !f.personal {
        s -= 0.15;
    }
    (s.clamp(0.0, 1.0), f)
}

/// Saillantie 0..1 (urgentie/nadruk).
pub fn salience(text: &str) -> f64 {
    let f = features(text);
    let mut s: f64 = 0.2;
    if f.urgent {
        s += 0.5;
    }
    if f.explicit_remember {
        s += 0.3;
    }
    s.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_sensibly() {
        let (a, _) = importance("Onthoud: mijn GPU is een RTX 5060 Ti.", MemoryKind::User);
        let (b, _) = importance("hoi", MemoryKind::Episodic);
        let (c, _) = importance("Ik woon in Utrecht en werk als ontwikkelaar.", MemoryKind::Episodic);
        let (d, _) = importance("Wat is het weer?", MemoryKind::Episodic);
        assert_eq!(a, 1.0);
        assert!(b < 0.1);
        assert!(c > d);
    }
}
