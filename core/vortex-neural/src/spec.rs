//! De layers (W11.05): wat elk net leert, zijn kenmerken (vaste volgorde = contract tussen
//! vectorizer in de consument, dataset en net) en de uitvoer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    /// Klassen (softmax), label = klasse-index.
    Multiclass(usize),
    /// Kans (sigmoid), label 0/1.
    Binary,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayerSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: LayerKind,
    pub features: &'static [&'static str],
    pub classes: &'static [&'static str],
    /// Waar de beslissing valt (documentatie/dashboard).
    pub used_in: &'static str,
    pub hidden: usize,
}

pub const ROUTER_CLASSES: &[&str] =
    &["chat", "code", "math", "analysis", "research", "planning", "personal", "factual"];

pub fn specs() -> Vec<LayerSpec> {
    vec![
        LayerSpec {
            name: "router",
            description: "Taaktype van een bericht (router-net)",
            kind: LayerKind::Multiclass(8),
            features: &[
                "chars_k",
                "words_h",
                "questions",
                "has_code",
                "has_numbers",
                "math_ops",
                "analysis_words",
                "research_words",
                "planning_words",
                "personal",
                "time_sensitive",
                "small_talk",
            ],
            classes: ROUTER_CLASSES,
            used_in: "vortex-cognition::router::route_with",
            hidden: 24,
        },
        LayerSpec {
            name: "importance",
            description: "Wordt deze herinnering later gebruikt? (schrijfpolicy)",
            kind: LayerKind::Binary,
            features: &[
                "chars_k",
                "words_h",
                "explicit_remember",
                "personal",
                "question_only",
                "small_talk",
                "has_numbers",
                "urgent",
                "k_user_project",
                "k_semantic_world",
                "k_procedural_summary",
                "k_episodic",
                "k_short_meta",
            ],
            classes: &[],
            used_in: "vortex-memory::policy (schrijven)",
            hidden: 16,
        },
        LayerSpec {
            name: "usefulness",
            description: "Helpt deze opgehaalde herinnering het antwoord?",
            kind: LayerKind::Binary,
            features: &["vector", "bm25_inv_rank", "graph", "temporal", "rrf", "rerank", "recency", "importance"],
            classes: &[],
            used_in: "vortex-memory::retrieve (eindscore)",
            hidden: 16,
        },
        LayerSpec {
            name: "entity",
            description: "Zijn twee entiteitsnamen hetzelfde ding? (entity matcher)",
            kind: LayerKind::Binary,
            features: &["lev_ratio", "token_jaccard", "prefix_ratio", "len_ratio", "same_initial", "both_capitalized"],
            classes: &[],
            used_in: "vortex-memory (entiteiten samenvoegen)",
            hidden: 12,
        },
        LayerSpec {
            name: "salience",
            description: "Is dit bericht belangrijk/urgent voor de gebruiker? (salience-net)",
            kind: LayerKind::Binary,
            features: &["important", "urgent", "novelty", "contradictory", "valence", "words_h"],
            classes: &[],
            used_in: "vortex-cognition::salience::tag_with",
            hidden: 12,
        },
        LayerSpec {
            name: "skill",
            description: "Helpt deze skill bij deze vraag? (skill selector)",
            kind: LayerKind::Binary,
            features: &["keyword_frac", "semantic", "trigger", "glob", "success_rate"],
            classes: &[],
            used_in: "vortex-tools::skills::SkillStore::select",
            hidden: 12,
        },
        LayerSpec {
            name: "verifier",
            description: "Is dit antwoord correct? (verifier/reward model, W13.05)",
            kind: LayerKind::Binary,
            features: &[
                "chars_k",
                "citations",
                "hedges",
                "judge_confidence",
                "critic_high",
                "critic_medium",
                "claims_supported",
                "claims_contradicted",
                "objective_check",
                "rounds",
            ],
            classes: &[],
            used_in: "vortex-agents::debate (verifier-score)",
            hidden: 16,
        },
        LayerSpec {
            name: "prm",
            description: "Is deze tussenstap goed? (process reward model, W13.05)",
            kind: LayerKind::Binary,
            features: &["position", "chars_k", "has_numbers", "hedges", "question_overlap", "flagged_by_critic"],
            classes: &[],
            used_in: "vortex-server::cognitive (planstappen)",
            hidden: 12,
        },
        LayerSpec {
            name: "decay",
            description: "Wordt deze herinnering nog gebruikt? (decay-predictor → koud geheugen)",
            kind: LayerKind::Binary,
            features: &[
                "age_years",
                "access_log",
                "importance",
                "confidence",
                "idle_years",
                "k_user_project",
                "k_semantic_world",
                "k_episodic",
                "k_other",
            ],
            classes: &[],
            used_in: "vortex-memory::maintenance::decay",
            hidden: 12,
        },
        LayerSpec {
            name: "crawl_rank",
            description: "Levert deze link een bewaarde, nuttige pagina op? (focused crawling, W14.03)",
            kind: LayerKind::Binary,
            features: &[
                "relevance",
                "reliability",
                "new_domain",
                "depth",
                "is_pdf",
                "anchor_words",
                "anchor_hits",
                "url_len",
                "has_query",
            ],
            classes: &[],
            used_in: "vortex-crawler::crawl::run (frontier-score)",
            hidden: 12,
        },
    ]
}

pub fn get(name: &str) -> Option<LayerSpec> {
    specs().into_iter().find(|s| s.name == name)
}
