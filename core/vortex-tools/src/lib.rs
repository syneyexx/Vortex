//! Tools en skills (W08): registry met contracten, getypeerde acties, pre/postcondities,
//! idempotente en transactionele uitvoering, effectanalyse, sandbox, ingebouwde tools,
//! MCP-client, tool discovery, resultaatfilters + cache, herstel bij fouten, capability handles
//! en skills (eigen formaat + converters Claude Skills ⇄ Cursor rules ⇄ AGENTS.md).
//!
//! Instap: [`ToolHub`] — bouwt registry + executor + skills; de server hangt er geheugen-tools aan.
// VX:W08.01
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod action;
pub mod builtin;
pub mod caps;
pub mod conditions;
pub mod contract;
pub mod diff;
pub mod error;
pub mod executor;
pub mod filter;
pub mod generated;
pub mod glob;
pub mod hub;
pub mod ledger;
pub mod mcp;
pub mod plan;
pub mod registry;
pub mod sandbox;
pub mod schema;
pub mod skills;
pub mod tape;
pub mod tool;
pub mod txn;

pub use action::{Action, ActionResult};
pub use caps::CapabilityHandle;
pub use conditions::Condition;
pub use contract::{Effect, ResourceUse, ToolContract};
pub use error::{ToolError, ToolErrorKind};
pub use executor::{ExecOptions, Executor};
pub use hub::{ToolHub, ToolHubConfig};
pub use registry::{ToolBrief, ToolRegistry};
pub use skills::{Skill, SkillStore};
pub use tape::{Tape, TapeMode};
pub use tool::{Tool, ToolCtx, ToolOutput};

/// Stabiele 64-bits hash (FNV-1a) — voor idempotentie- en cachesleutels.
pub fn stable_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Canonieke JSON (gesorteerde sleutels) → zelfde invoer = zelfde sleutel.
pub fn canonical_json(v: &serde_json::Value) -> String {
    fn walk(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::Value::String((*k).clone()).to_string());
                    out.push(':');
                    walk(&m[*k], out);
                }
                out.push('}');
            }
            serde_json::Value::Array(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    walk(x, out);
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut s = String::new();
    walk(v, &mut s);
    s
}

/// Sleutel voor (tool, invoer).
pub fn input_key(tool: &str, input: &serde_json::Value) -> String {
    format!("{tool}:{:016x}", stable_hash(canonical_json(input).as_bytes()))
}
