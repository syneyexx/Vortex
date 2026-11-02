//! Typen van het geheugen.

use serde::{Deserialize, Serialize};
use vortex_types::{MemoryId, Provenance, SessionId};

/// Geheugentype (VAULT/modules/geheugen.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    ShortTerm,
    LongTerm,
    Episodic,
    Semantic,
    User,
    Project,
    World,
    Procedural,
    Meta,
    Summary,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::ShortTerm => "short_term",
            MemoryKind::LongTerm => "long_term",
            MemoryKind::Episodic => "episodic",
            MemoryKind::Semantic => "semantic",
            MemoryKind::User => "user",
            MemoryKind::Project => "project",
            MemoryKind::World => "world",
            MemoryKind::Procedural => "procedural",
            MemoryKind::Meta => "meta",
            MemoryKind::Summary => "summary",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "short_term" => MemoryKind::ShortTerm,
            "long_term" => MemoryKind::LongTerm,
            "episodic" => MemoryKind::Episodic,
            "semantic" => MemoryKind::Semantic,
            "user" => MemoryKind::User,
            "project" => MemoryKind::Project,
            "world" => MemoryKind::World,
            "procedural" => MemoryKind::Procedural,
            "meta" => MemoryKind::Meta,
            "summary" => MemoryKind::Summary,
            _ => return None,
        })
    }
}

/// Wat er opgeslagen moet worden (invoer van de schrijfpipeline).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewMemory {
    pub kind: MemoryKind,
    pub content: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub session_id: Option<SessionId>,
    /// Bron-URI (bestand, URL, "chat:<sessie>"); wordt een `sources`-rij.
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub source_hash: Option<String>,
    /// Pagina/sectie/regel voor exacte bronverwijzing.
    #[serde(default)]
    pub locator: Option<String>,
    pub provenance: Provenance,
    #[serde(default)]
    pub event_time: Option<String>,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
    /// Door de aanroeper opgegeven belang (anders bepaalt de policy).
    #[serde(default)]
    pub importance: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Afgeleid van deze herinneringen (samenvattingen e.d.).
    #[serde(default)]
    pub derived_from: Vec<MemoryId>,
    /// Policy overslaan (bv. "onthoud dit" van de gebruiker).
    #[serde(default)]
    pub force: bool,
}

impl NewMemory {
    pub fn new(kind: MemoryKind, content: impl Into<String>, provenance: Provenance) -> Self {
        Self {
            kind,
            content: content.into(),
            title: String::new(),
            workspace: None,
            session_id: None,
            source_uri: None,
            source_hash: None,
            locator: None,
            provenance,
            event_time: None,
            valid_from: None,
            valid_to: None,
            importance: None,
            confidence: None,
            tags: vec![],
            derived_from: vec![],
            force: false,
        }
    }
}

/// Opgeslagen herinnering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub workspace: Option<String>,
    pub session_id: Option<String>,
    pub title: String,
    pub content: String,
    pub context: String,
    pub summary: String,
    pub source_uri: Option<String>,
    pub locator: Option<String>,
    pub provenance: Provenance,
    pub importance: f64,
    pub salience: f64,
    pub confidence: f64,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub event_time: Option<String>,
    pub access_count: i64,
    pub version: i64,
    pub stale: bool,
    pub merged_count: i64,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Uitkomst van schrijven.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WriteOutcome {
    Stored { ids: Vec<MemoryId>, conflicts: usize },
    Merged { into: MemoryId },
    Skipped { reason: String, importance: f64 },
}

/// Filters bij ophalen.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub kinds: Vec<MemoryKind>,
    /// Werkruimte; algemene kennis (zonder werkruimte) komt altijd mee.
    pub workspace: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub min_confidence: Option<f64>,
    pub source_prefix: Option<String>,
    pub include_stale: bool,
}

/// Herkomst van de score van een opgehaald item (zichtbaar in de GUI: "waarom opgehaald?").
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScoreParts {
    pub vector: Option<f32>,
    pub bm25_rank: Option<usize>,
    pub graph_hops: Option<u32>,
    pub temporal: bool,
    pub rrf: f64,
    pub rerank: Option<f32>,
    pub recency: f64,
    pub importance: f64,
    pub final_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Retrieved {
    pub memory: Memory,
    pub score: ScoreParts,
    /// In het tegenbewijs-slot (spreekt de top-resultaten tegen).
    pub counter_evidence: bool,
}

/// Beslissing van het conflict-team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Resolution {
    /// Eén wint; de ander wordt historie (valid_to = nu, lagere confidence, afgeleiden verouderd).
    Winner { winner: MemoryId, reason: String },
    /// Beide waar, elk in een eigen periode (A tot `a_until`, B vanaf `b_from`).
    Periods { a_until: String, b_from: String, reason: String },
    /// Niet te beslissen: zichtbaar als betwist.
    Disputed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    pub a: MemoryId,
    pub b: MemoryId,
    pub reason: String,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: String,
}
