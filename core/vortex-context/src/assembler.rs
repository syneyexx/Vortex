//! Context Assembler (W07.09): bouwt per stap de prompt binnen een tokenbudget.
//!
//! Volgorde van zekerheid: antwoordbudget eerst reserveren → system + rol + tools (stabiele
//! prefix, prompt caching) → gebruikersvraag → recente beurten letterlijk → herinneringen (met
//! tegenbewijs-slot) → oudere beurten **verliesbewust gecompacteerd**: exacte gegevens (paden,
//! ID's, bedragen, eenheden, data, URL's, code-namen, letterlijke correcties) blijven woordelijk;
//! uitleg wordt samengevat; herhaling valt weg. Oude toolresultaten worden een verwijzing.
//! Alles wat wegvalt staat in het rapport (Context Assembly Inspector).
// VX:W07.09

use serde::{Deserialize, Serialize};
use vortex_models::backend::{Message, Role};

/// Ruwe tokenschatting (≈ 3,5 tekens per token voor NL/EN). De echte tokenizer kan later.
pub fn tokens(s: &str) -> usize {
    (s.chars().count() as f64 / 3.5).ceil() as usize
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    /// Contextvenster van het model.
    pub window: usize,
    /// Gereserveerd voor het antwoord.
    pub answer: usize,
    /// Gereserveerd voor toolaanvragen + afsluiting.
    pub tools_reserve: usize,
    /// Zoveel laatste beurten altijd letterlijk.
    pub keep_last_turns: usize,
    /// Toolresultaten langer dan dit worden met code ingekort (kop + staart).
    pub max_tool_chars: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self { window: 32_768, answer: 4_096, tools_reserve: 1_024, keep_last_turns: 6, max_tool_chars: 6_000 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemItem {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub source: String,
    pub date: String,
    pub score: f64,
    pub counter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolBrief {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub role: Role,
    pub content: String,
    /// Toolresultaat (kan als verwijzing vervangen worden).
    #[serde(default)]
    pub tool_ref: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssemblyInput {
    pub system: String,
    #[serde(default)]
    pub role_prompt: Option<String>,
    #[serde(default)]
    pub user_profile: Option<String>,
    #[serde(default)]
    pub tools: Vec<ToolBrief>,
    #[serde(default)]
    pub memories: Vec<MemItem>,
    #[serde(default)]
    pub history: Vec<Turn>,
    /// Eerder gemaakte samenvatting van heel oude beurten (hiërarchisch).
    #[serde(default)]
    pub rolling_summary: Option<String>,
    pub user: String,
    /// Extra instructies (bv. metacognitie: "zeg eerlijk als je het niet weet").
    #[serde(default)]
    pub guidance: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub tokens: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssemblyReport {
    pub budget: usize,
    pub used: usize,
    pub sections: Vec<Section>,
    pub dropped: Vec<String>,
    pub compacted_turns: usize,
    pub preserved_facts: Vec<String>,
    pub tool_refs: Vec<String>,
    /// Stabiele prefix (system+rol+tools) — gelijk tussen stappen = cachebaar.
    pub prefix_hash: String,
}

#[derive(Debug, Clone)]
pub struct Assembled {
    pub messages: Vec<Message>,
    pub report: AssemblyReport,
}

/// Zinnen/fragmenten die exact bewaard moeten blijven.
pub fn exact_facts(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let t = s.trim().trim_matches(|c: char| c == ',' || c == ';').to_string();
        if t.len() > 1 && !out.contains(&t) {
            out.push(t);
        }
    };
    for raw in text.split_whitespace() {
        let w = raw.trim_matches(|c: char| matches!(c, '(' | ')' | '"' | '\'' | '.' | ',' | ';' | ':'));
        let has_digit = w.chars().any(|c| c.is_ascii_digit());
        let is_path = (w.contains('/') || w.contains('\\')) && w.len() > 2;
        let is_url = w.starts_with("http://") || w.starts_with("https://");
        let is_id = w.contains('_') && w.len() > 8 && w.chars().any(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        let is_code = raw.starts_with('`') && raw.ends_with('`') && raw.len() > 2;
        let is_money = w.starts_with('€') || w.starts_with('$') || w.ends_with('€');
        if is_url || is_path || is_id || is_code || is_money {
            push(raw.trim_matches(|c: char| c == ',' || c == '.' || c == ';'));
        } else if has_digit {
            // Getal + eenheid ("16 GB", "3,5 kg", "14:00", "2026-11-02") — neem het volgende woord mee.
            push(w);
        }
    }
    // Getal + eenheid samen bewaren.
    let words: Vec<&str> = text.split_whitespace().collect();
    for pair in words.windows(2) {
        let a = pair[0].trim_matches(|c: char| c == ',' || c == '(');
        let b = pair[1].trim_matches(|c: char| matches!(c, ',' | '.' | ')' | ';'));
        let unit = matches!(
            b.to_lowercase().as_str(),
            "gb" | "mb"
                | "tb"
                | "kb"
                | "kg"
                | "g"
                | "km"
                | "m"
                | "cm"
                | "mm"
                | "ms"
                | "s"
                | "min"
                | "uur"
                | "dagen"
                | "euro"
                | "eur"
                | "%"
                | "w"
                | "mhz"
                | "ghz"
                | "tok/s"
                | "cores"
                | "threads"
        );
        if unit && a.chars().any(|c| c.is_ascii_digit()) {
            let pair_s = format!("{a} {b}");
            if !out.contains(&pair_s) {
                out.push(pair_s);
            }
        }
    }
    // Letterlijke correcties van de gebruiker: hele zin bewaren.
    for s in text.split_inclusive(['.', '!', '?', '\n']) {
        let l = s.trim().to_lowercase();
        if ["nee,", "niet ", "correctie", "eigenlijk", "ik bedoelde", "dat klopt niet", "fout:", "moet zijn"]
            .iter()
            .any(|k| l.starts_with(k) || l.contains(&format!(" {k}")))
            && l.len() < 300
        {
            let t = s.trim().to_string();
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// Korte samenvatting van een beurt (eerste zin, max ~30 woorden) — "samenvatbaar = uitleg".
pub fn gist(text: &str) -> String {
    let first = text.split_inclusive(['.', '!', '?', '\n']).next().unwrap_or(text).trim();
    let words: Vec<&str> = first.split_whitespace().take(30).collect();
    words.join(" ")
}

fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Toolresultaat met code filteren (kop + staart + aantal weggelaten tekens).
pub fn filter_tool_output(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max * 2 / 3).collect();
    let tail: String = s.chars().skip(n - max / 3).collect();
    format!("{head}\n… [{} tekens weggelaten] …\n{tail}", n - max)
}

/// Bouw de context.
pub fn assemble(inp: &AssemblyInput, b: &Budget) -> Assembled {
    let mut rep = AssemblyReport { budget: b.window.saturating_sub(b.answer + b.tools_reserve), ..Default::default() };
    let avail = rep.budget;

    // 1. Stabiele prefix: system + rol + tools (tool discovery: alleen naam + beschrijving).
    let mut prefix = inp.system.trim().to_string();
    if let Some(r) = &inp.role_prompt {
        prefix.push_str("\n\n");
        prefix.push_str(r.trim());
    }
    if !inp.tools.is_empty() {
        prefix.push_str("\n\nBeschikbare tools (details bij gebruik):\n");
        for t in &inp.tools {
            prefix.push_str(&format!("- {}: {}\n", t.name, t.description));
        }
    }
    rep.prefix_hash = fnv(&prefix);
    rep.sections.push(Section { name: "prefix".into(), tokens: tokens(&prefix) });

    // 2. Dynamisch deel van de system prompt: profiel + sturing + herinneringen.
    let mut dynamic = String::new();
    if let Some(p) = &inp.user_profile {
        dynamic.push_str("\n\nOver de gebruiker:\n");
        dynamic.push_str(p.trim());
    }
    for g in &inp.guidance {
        dynamic.push_str("\n\n");
        dynamic.push_str(g.trim());
    }
    let user_tokens = tokens(&inp.user);
    let mut used = tokens(&prefix) + tokens(&dynamic) + user_tokens;

    // 3. Laatste beurten letterlijk (van nieuw naar oud), tot `keep_last_turns`.
    let n = inp.history.len();
    let keep_from = n.saturating_sub(b.keep_last_turns);
    let mut recent: Vec<Message> = Vec::new();
    for t in inp.history[keep_from..].iter() {
        let content =
            if t.tool_ref.is_some() { filter_tool_output(&t.content, b.max_tool_chars) } else { t.content.clone() };
        recent.push(Message { role: t.role, content, tool_calls: vec![], tool_call_id: None, name: None });
    }
    // Te groot? Dan de oudste van de "recente" ook nog compacteren.
    let mut recent_tokens: usize = recent.iter().map(|m| tokens(&m.content)).sum();
    let mut spill: Vec<Turn> = Vec::new();
    while used + recent_tokens > avail * 7 / 10 && recent.len() > 1 {
        let m = recent.remove(0);
        recent_tokens -= tokens(&m.content);
        spill.push(Turn { role: m.role, content: m.content, tool_ref: None });
    }
    used += recent_tokens;
    rep.sections.push(Section { name: "recent".into(), tokens: recent_tokens });

    // 4. Herinneringen (tegenbewijs gereserveerd: altijd mee als er ruimte is).
    let mut mem_block = String::new();
    let mut mems: Vec<&MemItem> = inp.memories.iter().collect();
    mems.sort_by(|a, b| b.counter.cmp(&a.counter).then(b.score.total_cmp(&a.score)));
    for m in mems {
        let line = if m.counter {
            format!("- LET OP, mogelijk tegenstrijdig [{}] ({}, {}): {}\n", m.kind, m.source, m.date, m.text)
        } else {
            format!("- [{}] ({}, {}): {}\n", m.kind, m.source, m.date, m.text)
        };
        if used + tokens(&mem_block) + tokens(&line) > avail * 85 / 100 {
            rep.dropped.push(format!("herinnering {}", m.id));
            continue;
        }
        mem_block.push_str(&line);
    }
    if !mem_block.is_empty() {
        dynamic.push_str("\n\nRelevante herinneringen (eigen geheugen; noem ze als je ze gebruikt):\n");
        dynamic.push_str(&mem_block);
    }
    used += tokens(&mem_block);
    rep.sections.push(Section { name: "memory".into(), tokens: tokens(&mem_block) });

    // 5. Oudere beurten: verliesbewust compacteren.
    let mut older: Vec<Turn> = inp.history[..keep_from].to_vec();
    older.extend(spill);
    let mut facts: Vec<String> = Vec::new();
    let mut gists: Vec<String> = Vec::new();
    for t in &older {
        if let Some(r) = &t.tool_ref {
            rep.tool_refs.push(r.clone());
            gists.push(format!("[toolresultaat {r} weggelaten — opvraagbaar]"));
            continue;
        }
        for f in exact_facts(&t.content) {
            if !facts.contains(&f) {
                facts.push(f);
            }
        }
        let who = if t.role == Role::User { "Gebruiker" } else { "Vortex" };
        let g = gist(&t.content);
        // Weglaatbaar = herhaling.
        if !gists.iter().any(|x| x.ends_with(&g)) {
            gists.push(format!("{who}: {g}"));
        }
    }
    rep.compacted_turns = older.len();
    let mut compact = String::new();
    if let Some(s) = &inp.rolling_summary {
        compact.push_str("Samenvatting van eerder in het gesprek:\n");
        compact.push_str(s.trim());
        compact.push('\n');
    }
    if !gists.is_empty() {
        compact.push_str("Eerder in dit gesprek (samengevat):\n");
        for g in &gists {
            let line = format!("- {g}\n");
            if used + tokens(&compact) + tokens(&line) > avail * 95 / 100 {
                rep.dropped.push("samenvatting oudere beurt".into());
                continue;
            }
            compact.push_str(&line);
        }
    }
    // Exacte gegevens altijd (ze zijn klein en mogen nooit verloren gaan).
    if !facts.is_empty() {
        compact.push_str("Vaste gegevens uit eerder in het gesprek (letterlijk):\n");
        for f in &facts {
            compact.push_str(&format!("- {f}\n"));
        }
    }
    rep.preserved_facts = facts;
    if !compact.is_empty() {
        dynamic.push_str("\n\n");
        dynamic.push_str(&compact);
    }
    used += tokens(&compact);
    rep.sections.push(Section { name: "compacted".into(), tokens: tokens(&compact) });
    rep.used = used;

    let mut messages = vec![Message::system(format!("{prefix}{dynamic}"))];
    messages.extend(recent);
    messages.push(Message::user(inp.user.clone()));
    Assembled { messages, report: rep }
}

/// Geïsoleerde context voor één rol (debatteam, research-team): alleen wat die rol nodig heeft.
pub fn assemble_for_role(role_prompt: &str, task: &str, material: &[String], b: &Budget) -> Assembled {
    let inp = AssemblyInput {
        system: role_prompt.to_string(),
        memories: material
            .iter()
            .enumerate()
            .map(|(i, m)| MemItem {
                id: format!("m{i}"),
                kind: "materiaal".into(),
                text: m.clone(),
                source: "workspace".into(),
                date: String::new(),
                score: 1.0,
                counter: false,
            })
            .collect(),
        user: task.to_string(),
        ..Default::default()
    };
    assemble(&inp, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(role: Role, c: &str) -> Turn {
        Turn { role, content: c.into(), tool_ref: None }
    }

    #[test]
    fn gate_compaction_keeps_exact_data() {
        let mut hist = vec![
            turn(
                Role::User,
                "Mijn config staat in C:\\dev\\Vortex\\config\\models.toml en de run is run_01M6EA44J67EF86N24FA86E5Y8.",
            ),
            turn(
                Role::Assistant,
                "Duidelijk. Dat bestand bevat de registry met alle modellen en servers, inclusief poorten en paden.",
            ),
            turn(Role::User, "De offerte is €1.250,00 voor 16 GB extra RAM, levering op 2026-11-14 om 14:00."),
            turn(Role::Assistant, "Genoteerd, ik reken met dat bedrag en die datum."),
            turn(Role::User, "Nee, de deadline moet zijn vrijdag 17:00, niet donderdag."),
            turn(Role::Assistant, "Begrepen, vrijdag 17:00."),
        ];
        for i in 0..30 {
            hist.push(turn(
                Role::User,
                &format!("Vraag {i}: vertel iets over het weer en de zee, uitgebreid en met veel uitleg. ").repeat(5),
            ));
            hist.push(turn(Role::Assistant, &"Uitleg over golven en wind. ".repeat(40)));
        }
        let inp = AssemblyInput {
            system: "Je bent Vortex.".into(),
            history: hist,
            user: "Wat waren de afspraken?".into(),
            ..Default::default()
        };
        let b = Budget { window: 4_000, answer: 1_000, tools_reserve: 200, keep_last_turns: 4, max_tool_chars: 2_000 };
        let a = assemble(&inp, &b);
        let all: String = a.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
        for must in [
            "C:\\dev\\Vortex\\config\\models.toml",
            "run_01M6EA44J67EF86N24FA86E5Y8",
            "€1.250,00",
            "16 GB",
            "2026-11-14",
            "14:00",
            "17:00",
            "Nee, de deadline moet zijn vrijdag 17:00, niet donderdag.",
        ] {
            assert!(all.contains(must), "exact gegeven verloren: {must}");
        }
        assert!(a.report.used <= a.report.budget, "{} > {}", a.report.used, a.report.budget);
        assert!(a.report.compacted_turns > 0);
        assert_eq!(a.messages.last().map(|m| m.content.as_str()), Some("Wat waren de afspraken?"));
    }

    #[test]
    fn answer_budget_reserved_and_counter_evidence_kept() {
        let mems: Vec<MemItem> = (0..200)
            .map(|i| MemItem {
                id: format!("m{i}"),
                kind: "semantic".into(),
                text: "lang geheugenitem ".repeat(30),
                source: "x".into(),
                date: "2026".into(),
                score: 1.0 - i as f64 / 1000.0,
                counter: i == 150,
            })
            .collect();
        let inp = AssemblyInput { system: "S".into(), memories: mems, user: "q".into(), ..Default::default() };
        let b = Budget { window: 3_000, answer: 1_500, ..Default::default() };
        let a = assemble(&inp, &b);
        assert!(a.report.used <= a.report.budget);
        assert!(a.messages[0].content.contains("LET OP, mogelijk tegenstrijdig"), "tegenbewijs heeft voorrang");
        assert!(!a.report.dropped.is_empty());
    }

    #[test]
    fn old_tool_results_become_references_and_prefix_is_stable() {
        let mut hist = vec![Turn { role: Role::Tool, content: "x".repeat(50_000), tool_ref: Some("tcl_1".into()) }];
        for _ in 0..8 {
            hist.push(turn(Role::User, "verder"));
        }
        let tools = vec![ToolBrief { name: "read_file".into(), description: "lees een bestand".into() }];
        let inp = AssemblyInput {
            system: "S".into(),
            tools: tools.clone(),
            history: hist,
            user: "q".into(),
            ..Default::default()
        };
        let a = assemble(&inp, &Budget::default());
        assert_eq!(a.report.tool_refs, vec!["tcl_1".to_string()]);
        assert!(a.messages[0].content.contains("[toolresultaat tcl_1 weggelaten"));
        assert!(a.messages[0].content.contains("read_file: lees een bestand"));
        let b2 = assemble(&AssemblyInput { user: "andere vraag".into(), ..inp }, &Budget::default());
        assert_eq!(a.report.prefix_hash, b2.report.prefix_hash, "prefix stabiel → prompt cache");
        assert!(filter_tool_output(&"y".repeat(10_000), 600).contains("weggelaten"));
    }
}
