//! Getypeerde, op ULID gebaseerde ID's (sorteerbaar op tijd). Tekstvorm: `prefix_ULID`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ulid::Ulid;

/// Fout bij het parsen van een ID uit tekst.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("ongeldig {kind}-ID: {input}")]
pub struct IdParseError {
    pub kind: &'static str,
    pub input: String,
}

macro_rules! define_id {
    ($(#[$m:meta])* $name:ident, $prefix:literal) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Ulid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;
            /// Nieuw, uniek ID.
            pub fn new() -> Self { Self(Ulid::new()) }
            pub fn from_ulid(u: Ulid) -> Self { Self(u) }
            pub fn ulid(&self) -> Ulid { self.0 }
        }
        impl Default for $name { fn default() -> Self { Self::new() } }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}_{}", $prefix, self.0) }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(self, f) }
        }
        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let err = || IdParseError { kind: $prefix, input: s.to_string() };
                let rest = s.strip_prefix(concat!($prefix, "_")).ok_or_else(err)?;
                Ulid::from_string(rest).map(Self).map_err(|_| err())
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.collect_str(self) }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

define_id!(/// Eén run (taak-uitvoering).
    RunId, "run");
define_id!(/// Eén event in het event log.
    EventId, "evt");
define_id!(/// Trace over processen heen.
    TraceId, "trc");
define_id!(/// Span binnen een trace.
    SpanId, "spn");
define_id!(/// Herinnering in het geheugen.
    MemoryId, "mem");
define_id!(/// Checkpoint van een run.
    CheckpointId, "ckp");
define_id!(/// Beheerde resource (GPU-slot, proces, handle, lock).
    ResourceId, "res");
define_id!(/// Sessie / gesprek.
    SessionId, "ses");
define_id!(/// Bericht in een sessie.
    MessageId, "msg");
define_id!(/// Toolaanroep.
    ToolCallId, "tcl");
define_id!(/// Werkruimte / project.
    WorkspaceId, "wsp");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_text_and_serde() {
        let id = RunId::new();
        let s = id.to_string();
        assert!(s.starts_with("run_"));
        assert_eq!(s.parse::<RunId>().ok(), Some(id));
        let j = serde_json::to_string(&id).unwrap_or_default();
        assert_eq!(serde_json::from_str::<RunId>(&j).ok(), Some(id));
    }

    #[test]
    fn wrong_prefix_is_rejected() {
        let id = EventId::new().to_string();
        assert!(id.parse::<RunId>().is_err());
    }

    #[test]
    fn ids_sort_by_time() {
        let a = EventId::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = EventId::new();
        assert!(a < b);
    }
}
