//! Pre- en postcondities (W08.03). Toolsucces ≠ doel bereikt: postcondities controleren het doel.

use serde::{Deserialize, Serialize};

use crate::caps::CapabilityHandle;
use crate::tool::ToolOutput;
use crate::txn::FileTxn;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    PathExists {
        path: String,
    },
    PathAbsent {
        path: String,
    },
    FileContains {
        path: String,
        text: String,
    },
    FileNotContains {
        path: String,
        text: String,
    },
    /// Tekstuitvoer bevat `text`.
    OutputContains {
        text: String,
    },
    /// JSON-pointer in `data` bestaat (en is evt. gelijk aan `equals`).
    OutputField {
        pointer: String,
        #[serde(default)]
        equals: Option<serde_json::Value>,
    },
}

/// Lees een bestand — eerst uit de lopende transactie (gestaged), anders van schijf.
fn read(path: &std::path::Path, txn: Option<&FileTxn>) -> Option<String> {
    if let Some(t) = txn
        && let Some(staged) = t.staged(path)
    {
        return staged;
    }
    std::fs::read_to_string(path).ok()
}

impl Condition {
    /// `None` = voldaan; `Some(reden)` = niet voldaan.
    pub fn check(&self, caps: &CapabilityHandle, out: Option<&ToolOutput>, txn: Option<&FileTxn>) -> Option<String> {
        let res = |p: &str| caps.resolve(p).ok();
        match self {
            Condition::PathExists { path } => match res(path) {
                Some(p) if read(&p, txn).is_some() || p.is_dir() => None,
                _ => Some(format!("{path} bestaat niet")),
            },
            Condition::PathAbsent { path } => match res(path) {
                Some(p) if read(&p, txn).is_none() && !p.is_dir() => None,
                _ => Some(format!("{path} bestaat al")),
            },
            Condition::FileContains { path, text } => match res(path).and_then(|p| read(&p, txn)) {
                Some(c) if c.contains(text.as_str()) => None,
                Some(_) => Some(format!("{path} bevat '{text}' niet")),
                None => Some(format!("{path} niet leesbaar")),
            },
            Condition::FileNotContains { path, text } => match res(path).and_then(|p| read(&p, txn)) {
                Some(c) if c.contains(text.as_str()) => Some(format!("{path} bevat nog '{text}'")),
                _ => None,
            },
            Condition::OutputContains { text } => match out {
                Some(o) if o.text.contains(text.as_str()) => None,
                _ => Some(format!("uitvoer bevat '{text}' niet")),
            },
            Condition::OutputField { pointer, equals } => match out.and_then(|o| o.data.pointer(pointer)) {
                Some(v) => match equals {
                    Some(e) if e != v => Some(format!("{pointer} = {v}, verwacht {e}")),
                    _ => None,
                },
                None => Some(format!("{pointer} ontbreekt in de uitvoer")),
            },
        }
    }
}
