//! Research-team (W10.06, W10.08–W10.13): van vraag naar geverifieerde kennis én data in één
//! stroom. Rollen: Lead (plan), Scout (zoekvragen + startpunten), Crawler-team (vortex-crawler),
//! Readers (passages, parallel op het 1060-model), Extractor (getypeerde records + validatie),
//! Archivist (direct naar geheugen + tabel), Bronbeoordelaar (betrouwbaarheid, onafhankelijkheid),
//! Analyst (per deelvraag, brongebonden), Fact-checker (trianguleren), Skepticus (gaten → volgende
//! zoekactie), Judge (genoeg per deelvraag?), Writer (rapport met [n]-verwijzingen + tabel),
//! Leren (domeinbetrouwbaarheid). Modi: quick / standaard / deep.
// VX:W10.08

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vortex_cognition::{GenOpts, Workspace};
use vortex_core::EventLog;
use vortex_crawler::Crawler;
use vortex_memory::{MemoryKind, NewMemory};
use vortex_models::backend::Message;
use vortex_tools::{ExecOptions, ToolHub};
use vortex_types::{EventId, EventKind, Provenance, SourceKind};

use crate::json::{extract, strings};
use crate::{Llm, MemoryAccess, Reporter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMode {
    Quick,
    Standard,
    Deep,
}

impl ResearchMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "quick" => Self::Quick,
            "deep" => Self::Deep,
            _ => Self::Standard,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }
    fn rounds(self) -> usize {
        match self {
            Self::Quick => 1,
            Self::Standard => 2,
            Self::Deep => 3,
        }
    }
    fn fetch_per_query(self) -> usize {
        match self {
            Self::Quick => 1,
            Self::Standard => 3,
            Self::Deep => 4,
        }
    }
}

/// Metacognitie kiest de modus; de Research-knop (mode "research") forceert deep.
pub fn choose_mode(chat_mode: &str, difficulty: f64, words: usize) -> ResearchMode {
    if chat_mode == "research" || chat_mode == "deep" {
        ResearchMode::Deep
    } else if difficulty < 0.4 && words < 14 {
        ResearchMode::Quick
    } else {
        ResearchMode::Standard
    }
}

#[derive(Debug, Clone)]
pub struct ResearchDeps {
    pub main: Llm,
    pub aux: Option<Llm>,
    pub memory: Option<MemoryAccess>,
    /// Geheugen-werkruimte (globale corpus-synthese over een verzameling).
    pub mem_workspace: Option<String>,
    pub tools: Option<Arc<ToolHub>>,
    pub crawler: Option<Arc<Crawler>>,
    pub online: bool,
    pub reporter: Reporter,
    pub exec: ExecOptions,
    pub workspace: Option<Workspace>,
    pub log: EventLog,
    pub research_id: String,
    /// Map voor rapporten (artefacten).
    pub out_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subquestion {
    pub i: usize,
    pub text: String,
    pub status: String,
    pub answer: String,
    pub cited: Vec<usize>,
    pub next_query: Option<String>,
    pub rounds: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub n: usize,
    /// "web" | "geheugen" | "document"
    pub kind: String,
    pub uri: String,
    pub title: String,
    pub domain: String,
    pub locator: Option<String>,
    pub reliability: f64,
    pub passages: Vec<String>,
    #[serde(skip)]
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResearchOutcome {
    pub research_id: String,
    pub question: String,
    pub mode: String,
    pub report: String,
    pub report_path: Option<String>,
    pub subquestions: Vec<Subquestion>,
    pub sources: Vec<Source>,
    pub records: usize,
    pub invalid_records: usize,
    pub rounds: usize,
    pub scope: String,
    /// Gedownloade documenten (PDF e.d.) → ingest.
    pub documents: Vec<String>,
}

async fn ask(llm: &Llm, system: &str, user: String, max: u32, schema: Option<Value>) -> Value {
    let out = llm
        .complete(
            vec![Message::system(system.to_string()), Message::user(user)],
            GenOpts { max_tokens: Some(max), temperature: Some(0.2), json_schema: schema, reasoning_effort: None },
        )
        .await
        .unwrap_or_default();
    extract(&out).unwrap_or(Value::String(out))
}

fn domain(uri: &str) -> String {
    uri.split("//").nth(1).and_then(|h| h.split('/').next()).unwrap_or(uri).trim_start_matches("www.").to_string()
}

fn terms(s: &str) -> BTreeSet<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 3).map(String::from).collect()
}

/// Reader-voorselectie: alinea's met de meeste overlap met de deelvraag (goedkoop, deterministisch).
pub fn top_passages(text: &str, q: &str, n: usize) -> Vec<String> {
    let qt = terms(q);
    let mut paras: Vec<(usize, String)> = text
        .split("\n\n")
        .flat_map(|p| {
            if p.len() > 1500 { p.split('\n').map(String::from).collect::<Vec<_>>() } else { vec![p.to_string()] }
        })
        .map(|p| p.trim().to_string())
        .filter(|p| p.len() > 40)
        .map(|p| (terms(&p).intersection(&qt).count(), p))
        .collect();
    paras.sort_by_key(|p| std::cmp::Reverse(p.0));
    paras.into_iter().filter(|(s, _)| *s > 0).take(n).map(|(_, p)| p.chars().take(1200).collect()).collect()
}

struct State {
    sources: Vec<Source>,
    records: usize,
    invalid: usize,
    documents: Vec<String>,
}

impl ResearchDeps {
    fn aux(&self) -> &Llm {
        self.aux.as_ref().unwrap_or(&self.main)
    }

    fn save_run(
        &self,
        question: &str,
        mode: ResearchMode,
        status: &str,
        subs: &[Subquestion],
        sources: &[Source],
        report: Option<(&str, Option<&str>)>,
    ) {
        let now = vortex_types::now().to_rfc3339();
        let plan = serde_json::to_string(subs).unwrap_or_default();
        let src = serde_json::to_string(sources).unwrap_or_default();
        let run = self.exec.run.map(|r| r.to_string());
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "INSERT INTO research_runs(id, run_id, question, mode, status, plan, sources, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(id) DO UPDATE SET status = ?5, plan = ?6, sources = ?7, updated_at = ?8",
                params![self.research_id, run, question, mode.as_str(), status, plan, src, now],
            )?;
            if let Some((r, p)) = report {
                tx.execute("UPDATE research_runs SET report = ?2, report_path = ?3 WHERE id = ?1", params![self.research_id, r, p])?;
            }
            Ok(())
        });
    }

    fn add_source(
        &self,
        st: &Mutex<State>,
        kind: &str,
        uri: &str,
        title: &str,
        locator: Option<String>,
        text: String,
    ) -> usize {
        let mut g = st.lock();
        if let Some(s) = g.sources.iter().find(|s| s.uri == uri && s.locator == locator) {
            return s.n;
        }
        let n = g.sources.len() + 1;
        let dom = if kind == "web" { domain(uri) } else { kind.to_string() };
        let reliability = match (&self.crawler, kind) {
            (Some(c), "web") => c.reliability(&dom),
            (_, "geheugen" | "document") => 0.7,
            _ => 0.5,
        };
        g.sources.push(Source {
            n,
            kind: kind.into(),
            uri: uri.into(),
            title: title.into(),
            domain: dom,
            locator,
            reliability,
            passages: vec![],
            text,
        });
        drop(g);
        self.reporter.emit(EventKind::ResearchSource, json!({"research": self.research_id, "n": n, "kind": kind, "uri": uri, "title": title, "reliability": reliability}));
        n
    }

    /// Scout + crawler-team + lokaal geheugen: bronnen voor één deelvraag.
    async fn find(&self, st: &Mutex<State>, sq: &Subquestion, mode: ResearchMode) -> Vec<usize> {
        let mut found = Vec::new();
        let query = sq.next_query.clone().unwrap_or_else(|| sq.text.clone());
        // Lokaal (altijd): geheugen incl. ingelezen documenten, met exacte locator.
        if let Some((store, rr)) = &self.memory {
            let mut q = vortex_memory::Query::new(query.clone());
            q.k = 5;
            q.filters.workspace = self.mem_workspace.clone();
            if let Ok(items) = store.retrieve(&q, rr.as_ref()).await {
                for it in items.into_iter().filter(|i| i.score.final_score > 0.12) {
                    let m = it.memory;
                    let uri = m.source_uri.clone().unwrap_or_else(|| format!("geheugen:{}", m.id));
                    let kind = if m.source_uri.as_deref().is_some_and(|u| !u.starts_with("chat:")) {
                        "document"
                    } else {
                        "geheugen"
                    };
                    found.push(self.add_source(st, kind, &uri, &m.title, m.locator.clone(), m.content.clone()));
                }
            }
        }
        // Web (alleen online): Scout-zoekvragen → SearXNG → pagina's lezen; deep: gericht crawlen.
        let (Some(h), true) = (&self.tools, self.online) else { return found };
        let queries: Vec<String> = if mode == ResearchMode::Quick {
            vec![query.clone()]
        } else {
            let v = ask(self.aux(), "Je bent de Scout. Bedenk 2 tot 3 korte, verschillende zoekvragen (ook Engels als dat beter zoekt). JSON: {\"queries\": [\"…\"]}.", query.clone(), 200, None).await;
            let mut q = strings(&v, "queries");
            if q.is_empty() {
                q.push(query.clone());
            }
            q.truncate(3);
            q
        };
        let mut urls: Vec<(String, String)> = Vec::new();
        for (k, q) in queries.iter().enumerate() {
            let id = format!("{}:search:{}:{}:{k}", self.research_id, sq.i, sq.rounds);
            let r = h.executor.call(&id, "web_search", json!({"query": q, "max_results": 6}), &self.exec).await;
            if let Some(o) = r.output.filter(|_| r.ok) {
                for x in o.data["results"].as_array().into_iter().flatten().take(mode.fetch_per_query()) {
                    if let Some(u) = x["url"].as_str()
                        && !urls.iter().any(|(a, _)| a == u)
                    {
                        urls.push((u.to_string(), x["title"].as_str().unwrap_or("").to_string()));
                    }
                }
            }
        }
        let ids: Vec<String> = urls
            .iter()
            .map(|(u, _)| format!("{}:fetch:{}", self.research_id, vortex_tools::stable_hash(u.as_bytes())))
            .collect();
        let fetched =
            futures::future::join_all(urls.iter().zip(ids.iter()).map(|((u, _), id)| {
                h.executor.call(id, "web_fetch", json!({"url": u, "max_chars": 30000}), &self.exec)
            }))
            .await;
        for ((u, t), r) in urls.iter().zip(fetched) {
            if let Some(o) = r.output.filter(|_| r.ok) {
                let title = o.data["title"].as_str().filter(|s| !s.is_empty()).unwrap_or(t).to_string();
                found.push(self.add_source(st, "web", u, &title, None, o.text));
            }
        }
        if mode == ResearchMode::Deep
            && let Some(c) = &self.crawler
            && !urls.is_empty()
        {
            let seeds: Vec<String> = urls.iter().take(3).map(|(u, _)| u.clone()).collect();
            let task = format!("{}:{}", self.research_id, sq.i);
            let rep = c.crawl(&task, &query, seeds, self.exec.run, self.exec.trace, self.exec.cancel.clone()).await;
            for p in rep.pages {
                found.push(self.add_source(st, "web", &p.final_url, &p.title, None, p.text));
            }
            st.lock().documents.extend(rep.documents);
        }
        found.sort();
        found.dedup();
        found
    }

    /// Readers (parallel, 1060) + Extractor + Archivist voor één bron.
    async fn read_extract(&self, st: &Mutex<State>, sq: &Subquestion, n: usize, mode: ResearchMode) {
        let (text, uri, loc) = {
            let g = st.lock();
            let s = &g.sources[n - 1];
            (s.text.clone(), s.uri.clone(), s.locator.clone())
        };
        let mut passages = top_passages(&text, &sq.text, 3);
        if passages.is_empty() && !text.trim().is_empty() {
            passages.push(text.chars().take(1200).collect());
        }
        {
            let mut g = st.lock();
            let s = &mut g.sources[n - 1];
            for p in &passages {
                if !s.passages.contains(p) {
                    s.passages.push(p.clone());
                }
            }
        }
        if mode == ResearchMode::Quick || passages.is_empty() {
            return;
        }
        let kinds = crate::records::KINDS.join(", ");
        let v = ask(
            self.aux(),
            &format!("Je bent de Extractor. Zet de passages om in getypeerde records (soorten: {kinds}). Alleen wat er letterlijk staat; geen interpretatie. Velden per soort: claim{{statement,polarity,date,confidence}}, number{{value,unit,what,when,where}}, entity{{name,type}}, relation{{subject,predicate,object}}, event{{what,who,where,when,status}}, quote{{text,speaker,date}}, table{{headers,rows}}. JSON: {{\"records\": [{{\"kind\": \"…\", …, \"passage\": 0}}]}} (passage = index)."),
            format!("Deelvraag: {}\n\n{}", sq.text, passages.iter().enumerate().map(|(i, p)| format!("[{i}] {p}")).collect::<Vec<_>>().join("\n\n")),
            900,
            None,
        )
        .await;
        for r in v["records"].as_array().cloned().unwrap_or_default().into_iter().take(12) {
            match crate::records::validate(&r) {
                Ok((kind, data)) => {
                    let pi = r["passage"].as_u64().unwrap_or(0) as usize;
                    let passage = passages.get(pi).cloned().unwrap_or_default();
                    self.archive(st, sq.i, &kind, &data, n, &uri, loc.clone(), &passage).await;
                }
                Err(_) => st.lock().invalid += 1,
            }
        }
    }

    /// Archivist: record → tabel + geheugen (direct, met herkomst) + event.
    #[allow(clippy::too_many_arguments)]
    async fn archive(
        &self,
        st: &Mutex<State>,
        subq: usize,
        kind: &str,
        data: &Value,
        n: usize,
        uri: &str,
        loc: Option<String>,
        passage: &str,
    ) {
        let id = EventId::new().to_string();
        let text = crate::records::render(kind, data);
        // Vaste bron-hash per bron: anders maakt elk nieuw record de vorige uit dezelfde bron "verouderd".
        let src_hash = {
            let g = st.lock();
            format!(
                "{:016x}",
                vortex_tools::stable_hash(g.sources.get(n - 1).map(|s| s.text.as_bytes()).unwrap_or(uri.as_bytes()))
            )
        };
        let mut mem_id = None;
        if let Some((store, _)) = &self.memory
            && !matches!(kind, "source" | "table")
        {
            let src = if uri.starts_with("http") { SourceKind::Web } else { SourceKind::File };
            let mut nm =
                NewMemory::new(MemoryKind::Semantic, text.clone(), Provenance::with_origin(src, uri.to_string()));
            nm.title = format!("{kind}: {}", text.chars().take(60).collect::<String>());
            nm.source_uri = Some(uri.to_string());
            nm.source_hash = Some(src_hash);
            nm.locator =
                loc.clone().or_else(|| Some(format!("passage: {}", passage.chars().take(80).collect::<String>())));
            nm.workspace = self.mem_workspace.clone();
            nm.tags = vec!["research".into(), kind.to_string()];
            nm.confidence = data["confidence"].as_f64();
            nm.force = true;
            mem_id = match store.write_or_defer(nm).await {
                Ok(Some(vortex_memory::WriteOutcome::Stored { ids, .. })) => ids.first().map(|i| i.to_string()),
                Ok(Some(vortex_memory::WriteOutcome::Merged { into })) => Some(into.to_string()),
                _ => None,
            };
        }
        let now = vortex_types::now().to_rfc3339();
        let _ = self.log.db().write(|tx| {
            tx.execute(
                "INSERT INTO research_records(id, research_id, subquestion, kind, data, source_url, passage, status, memory_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'unverified', ?8, ?9)",
                params![id, self.research_id, subq as i64, kind, data.to_string(), uri, passage, mem_id, now],
            )?;
            Ok(())
        });
        st.lock().records += 1;
        self.reporter.emit(EventKind::ResearchRecord, json!({"research": self.research_id, "id": id, "kind": kind, "text": text, "source": n, "subquestion": subq}));
    }

    /// Fact-checker: claims/getallen trianguleren over onafhankelijke bronnen (ander domein).
    fn triangulate(&self, st: &Mutex<State>) -> (i64, i64) {
        let rows: Vec<(String, String, String)> = self
            .log
            .db()
            .read(|c| {
                let mut s = c.prepare("SELECT id, data, source_url FROM research_records WHERE research_id = ?1 AND kind IN ('claim','number','event','relation')")?;
                let v = s.query_map([&self.research_id], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default())))?.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        let sources = st.lock().sources.clone();
        let (mut sup, mut unv) = (0, 0);
        for (id, data, url) in rows {
            let d: Value = serde_json::from_str(&data).unwrap_or_default();
            let t = terms(&d.to_string());
            let own = domain(&url);
            let others = sources
                .iter()
                .filter(|s| s.uri != url && (s.domain != own || s.kind != "web"))
                .filter(|s| {
                    s.passages
                        .iter()
                        .chain(std::iter::once(&s.text))
                        .any(|p| terms(p).intersection(&t).count() * 3 >= t.len().max(1) * 2)
                })
                .map(|s| s.domain.clone())
                .collect::<BTreeSet<_>>();
            let status = if !others.is_empty() { "supported" } else { "unverified" };
            if status == "supported" {
                sup += 1;
            } else {
                unv += 1;
            }
            let _ = self.log.db().write(|tx| {
                tx.execute("UPDATE research_records SET status = ?2 WHERE id = ?1", params![id, status])?;
                Ok(())
            });
        }
        (sup, unv)
    }

    async fn analyze(&self, st: &Mutex<State>, sq: &mut Subquestion, srcs: &[usize]) {
        let ctx: String = {
            let g = st.lock();
            srcs.iter()
                .filter_map(|n| g.sources.get(n - 1))
                .map(|s| {
                    let ps = if s.passages.is_empty() {
                        s.text.chars().take(800).collect()
                    } else {
                        s.passages.join("\n…\n")
                    };
                    format!(
                        "[{}] {} ({}{}; betrouwbaarheid {:.2})\n{}",
                        s.n,
                        s.title,
                        s.uri,
                        s.locator.as_ref().map(|l| format!(" · {l}")).unwrap_or_default(),
                        s.reliability,
                        ps
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        if ctx.is_empty() {
            sq.answer = "Geen bronnen gevonden.".into();
            return;
        }
        let v = ask(
            &self.main,
            "Je bent een Analyst. Beantwoord de deelvraag alleen met wat in de genummerde bronnen staat; zet na elke bewering de bron als [n]. Reken na waar nodig. Noem tegenstrijdigheden. JSON: {\"answer\": \"…\", \"cited\": [n], \"confidence\": 0.0-1.0, \"missing\": \"wat nog ontbreekt\"}.",
            format!("Deelvraag: {}\n\nBronnen:\n{ctx}", sq.text),
            1200,
            None,
        )
        .await;
        sq.answer = v["answer"].as_str().map(String::from).unwrap_or_else(|| v.as_str().unwrap_or("").to_string());
        sq.cited = v["cited"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as usize)).collect())
            .unwrap_or_default();
        if sq.cited.is_empty() {
            // Uit de tekst: [n]-verwijzingen.
            let mut rest = sq.answer.as_str();
            while let Some(i) = rest.find('[') {
                let tail = &rest[i + 1..];
                if let Some(j) = tail.find(']')
                    && let Ok(n) = tail[..j].trim().parse::<usize>()
                {
                    sq.cited.push(n);
                }
                rest = tail;
            }
            sq.cited.sort();
            sq.cited.dedup();
        }
    }

    async fn judge(&self, sq: &mut Subquestion, mode: ResearchMode, round: usize) -> bool {
        if mode == ResearchMode::Quick {
            return true;
        }
        let v = ask(
            &self.main,
            "Je bent de Judge van het research-team. Is er genoeg onderbouwd bewijs voor deze deelvraag (bronnen geciteerd, geen grote gaten)? Zo niet: welke ene gerichte zoekactie levert nu het meeste op (Skepticus)? JSON: {\"enough\": true|false, \"reason\": \"…\", \"next_query\": \"…\"}.",
            format!("Deelvraag: {}\nAntwoord: {}\nGeciteerde bronnen: {:?}", sq.text, sq.answer, sq.cited),
            300,
            None,
        )
        .await;
        let enough = v["enough"].as_bool().unwrap_or(!sq.cited.is_empty());
        sq.next_query = v["next_query"].as_str().filter(|s| !s.trim().is_empty()).map(String::from);
        enough || round >= mode.rounds()
    }

    async fn write_report(&self, question: &str, subs: &[Subquestion], st: &Mutex<State>, scope: &str) -> String {
        let sources = st.lock().sources.clone();
        let mut body = String::new();
        for s in subs {
            body.push_str(&format!("## {}\n{}\n\n", s.text, s.answer));
        }
        let v = ask(
            &self.main,
            "Je bent de Writer. Schrijf een kort rapport in het Nederlands: eerst 'Kort antwoord' (2-4 zinnen), dan per deelvraag een sectie. Behoud ALLE [n]-bronverwijzingen precies zoals ze staan; voeg geen feiten zonder bron toe. Markdown.",
            format!("Hoofdvraag: {question}\n\nBevindingen per deelvraag:\n{body}"),
            1800,
            None,
        )
        .await;
        let mut report = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
        if report.trim().is_empty() || report == "null" {
            report = format!("# {question}\n\n{body}");
        }
        // Getallentabel uit de records.
        let nums: Vec<(String, Option<String>)> = self
            .log
            .db()
            .read(|c| {
                let mut s = c.prepare(
                    "SELECT data, source_url FROM research_records WHERE research_id = ?1 AND kind = 'number' LIMIT 30",
                )?;
                let v = s
                    .query_map([&self.research_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        if !nums.is_empty() {
            report.push_str(
                "\n\n## Getallen\n\n| Wat | Waarde | Eenheid | Wanneer | Bron |\n| --- | --- | --- | --- | --- |\n",
            );
            for (d, u) in nums {
                let d: Value = serde_json::from_str(&d).unwrap_or_default();
                let n = sources
                    .iter()
                    .find(|s| Some(&s.uri) == u.as_ref())
                    .map(|s| format!("[{}]", s.n))
                    .unwrap_or_default();
                report.push_str(&format!(
                    "| {} | {} | {} | {} | {n} |\n",
                    d["what"].as_str().unwrap_or(""),
                    d["value"],
                    d["unit"].as_str().unwrap_or(""),
                    d["when"].as_str().unwrap_or("")
                ));
            }
        }
        // Bewijsoverzicht: bronnen met betrouwbaarheid + onafhankelijke groepen.
        report.push_str(&format!("\n\n## Bronnen ({scope})\n\n"));
        for s in &sources {
            report.push_str(&format!(
                "[{}] {} — {}{} · {} · betrouwbaarheid {:.2}\n",
                s.n,
                if s.title.is_empty() { &s.uri } else { &s.title },
                s.uri,
                s.locator.as_ref().map(|l| format!(" ({l})")).unwrap_or_default(),
                s.kind,
                s.reliability
            ));
        }
        let groups: BTreeSet<&str> = sources.iter().map(|s| s.domain.as_str()).collect();
        report.push_str(&format!("\nOnafhankelijke bronnen (domeinen/documenten): {}\n", groups.len()));
        let open: Vec<&Subquestion> = subs.iter().filter(|s| s.status != "answered").collect();
        if !open.is_empty() {
            report.push_str("\n## Open vragen\n\n");
            for s in open {
                report.push_str(&format!(
                    "- {}{}\n",
                    s.text,
                    s.next_query.as_ref().map(|q| format!(" (volgende zoekactie: {q})")).unwrap_or_default()
                ));
            }
        }
        report
    }
}

/// Het hele research-proces.
pub async fn run(d: &ResearchDeps, question: &str, mode: ResearchMode) -> ResearchOutcome {
    let rep = &d.reporter;
    let scope = if d.online && d.tools.is_some() { "lokaal + web" } else { "lokaal" };
    rep.emit(
        EventKind::ResearchStarted,
        json!({"research": d.research_id, "question": question, "mode": mode.as_str(), "scope": scope}),
    );
    // 1. Plan (Lead).
    rep.started("Lead", "deelvragen en stopcriteria");
    let subs_text: Vec<String> = if mode == ResearchMode::Quick {
        vec![question.to_string()]
    } else {
        let v = ask(
            &d.main,
            "Je bent de Research Lead. Splits de onderzoeksvraag in 2 tot 4 deelvragen die samen de vraag beantwoorden, plus hypotheses en een stopcriterium. JSON: {\"subquestions\": [\"…\"], \"hypotheses\": [\"…\"], \"stop\": \"…\"}.",
            question.to_string(),
            500,
            None,
        )
        .await;
        if let Some(ws) = &d.workspace {
            for h in strings(&v, "hypotheses") {
                ws.add_hypothesis(&h, 0.5);
            }
        }
        let mut s = strings(&v, "subquestions");
        s.truncate(4);
        if s.is_empty() {
            s.push(question.to_string());
        }
        s
    };
    let mut subs: Vec<Subquestion> = subs_text
        .into_iter()
        .enumerate()
        .map(|(i, t)| Subquestion {
            i,
            text: t,
            status: "open".into(),
            answer: String::new(),
            cited: vec![],
            next_query: None,
            rounds: 0,
        })
        .collect();
    if let Some(ws) = &d.workspace {
        for s in &subs {
            ws.push_goal(&s.text, None, vec![]);
        }
    }
    rep.done(
        "Lead",
        &format!("{} deelvragen", subs.len()),
        json!({"subquestions": subs.iter().map(|s| s.text.clone()).collect::<Vec<_>>()}),
    );
    for s in &subs {
        rep.emit(EventKind::ResearchSubquestion, json!({"research": d.research_id, "subquestion": s}));
    }
    d.save_run(question, mode, "bezig", &subs, &[], None);
    let st = Mutex::new(State { sources: vec![], records: 0, invalid: 0, documents: vec![] });
    let mut rounds = 0;
    for round in 1..=mode.rounds() {
        if d.exec.cancel.is_cancelled() {
            break;
        }
        rounds = round;
        let open: Vec<usize> = subs.iter().filter(|s| s.status != "answered").map(|s| s.i).collect();
        if open.is_empty() {
            break;
        }
        // 2. Vind — alle open deelvragen tegelijk (CPU/netwerk/1060 parallel).
        rep.started("Scouts", &format!("ronde {round}: bronnen zoeken"));
        let found: Vec<Vec<usize>> = futures::future::join_all(open.iter().map(|&i| d.find(&st, &subs[i], mode))).await;
        rep.done("Scouts", &format!("{} bronnen", st.lock().sources.len()), json!({}));
        // 3. Lees + zet om — Readers/Extractor parallel per bron.
        rep.started("Readers", "passages en records");
        let jobs: Vec<(usize, usize)> =
            open.iter().zip(found.iter()).flat_map(|(&i, ns)| ns.iter().map(move |&n| (i, n))).collect();
        futures::future::join_all(jobs.iter().map(|&(i, n)| d.read_extract(&st, &subs[i], n, mode))).await;
        let (nrec, ninv) = {
            let g = st.lock();
            (g.records, g.invalid)
        };
        rep.done(
            "Readers",
            &format!("{nrec} records ({ninv} ongeldig verworpen)"),
            json!({"records": nrec, "invalid": ninv}),
        );
        // 4–6. Analyseer (hoofdmodel, na elkaar), verifieer, oordeel per deelvraag.
        for (k, &i) in open.iter().enumerate() {
            let srcs = found[k].clone();
            let mut sq = subs[i].clone();
            sq.rounds = round;
            rep.started("Analyst", &sq.text.chars().take(80).collect::<String>());
            d.analyze(&st, &mut sq, &srcs).await;
            let ok = d.judge(&mut sq, mode, round).await;
            sq.status = if ok && !sq.cited.is_empty() {
                "answered".into()
            } else if ok {
                "insufficient".into()
            } else {
                "open".into()
            };
            rep.done(
                "Judge",
                &format!("deelvraag {}: {}", i + 1, sq.status),
                json!({"subquestion": i, "status": sq.status}),
            );
            rep.emit(EventKind::ResearchSubquestion, json!({"research": d.research_id, "subquestion": sq}));
            if let Some(ws) = &d.workspace {
                ws.add_note(&format!("deelvraag {}: {} ({} bronnen)", i + 1, sq.status, sq.cited.len()));
            }
            subs[i] = sq;
        }
        let (sup, unv) = d.triangulate(&st);
        rep.done(
            "Fact-checker",
            &format!("{sup} records gestaafd door een onafhankelijke bron, {unv} onbevestigd"),
            json!({"supported": sup, "unverified": unv}),
        );
        d.save_run(question, mode, "bezig", &subs, &st.lock().sources, None);
    }
    // 7. Lever op (Writer) — rapport als artefact + samenvatting in het geheugen.
    rep.started("Writer", "rapport met bronverwijzingen");
    let report = d.write_report(question, &subs, &st, scope).await;
    let mut report_path = None;
    if let Some(dir) = &d.out_dir {
        let p = dir.join(format!("{}.md", d.research_id));
        if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&p, &report).is_ok() {
            report_path = Some(p.display().to_string());
        }
    }
    if let Some((store, _)) = &d.memory {
        let mut nm = NewMemory::new(
            MemoryKind::Summary,
            report.chars().take(6000).collect::<String>(),
            Provenance::with_origin(SourceKind::Tool, format!("research:{}", d.research_id)),
        );
        nm.title = format!("Onderzoek: {}", question.chars().take(80).collect::<String>());
        nm.source_uri = report_path.clone().or_else(|| Some(format!("research:{}", d.research_id)));
        nm.workspace = d.mem_workspace.clone();
        nm.tags = vec!["research".into(), "rapport".into()];
        nm.force = true;
        let _ = store.write_or_defer(nm).await;
    }
    let sources = st.lock().sources.clone();
    d.save_run(question, mode, "klaar", &subs, &sources, Some((&report, report_path.as_deref())));
    rep.done("Writer", &format!("{} tekens, {} bronnen", report.len(), sources.len()), json!({"path": report_path}));
    rep.emit(
        EventKind::ResearchReport,
        json!({"research": d.research_id, "path": report_path, "chars": report.len(), "sources": sources.len()}),
    );
    // 8. Leer — bronbetrouwbaarheid per domein (geciteerd = nuttig; gestaafde records).
    if let Some(c) = &d.crawler {
        let cited: BTreeSet<usize> = subs.iter().flat_map(|s| s.cited.iter().copied()).collect();
        let supported: HashMap<String, i64> = d
            .log
            .db()
            .read(|cn| {
                let mut s = cn.prepare("SELECT source_url, COUNT(*) FROM research_records WHERE research_id = ?1 AND status = 'supported' GROUP BY source_url")?;
                let v = s.query_map([&d.research_id], |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get::<_, i64>(1)?)))?.collect::<rusqlite::Result<HashMap<_, _>>>()?;
                Ok(v)
            })
            .unwrap_or_default();
        for s in sources.iter().filter(|s| s.kind == "web") {
            c.record_outcome(&s.domain, cited.contains(&s.n), supported.get(&s.uri).copied().unwrap_or(0), 0);
        }
    }
    let g = st.lock();
    ResearchOutcome {
        research_id: d.research_id.clone(),
        question: question.into(),
        mode: mode.as_str().into(),
        report,
        report_path,
        subquestions: subs,
        sources,
        records: g.records,
        invalid_records: g.invalid,
        rounds,
        scope: scope.into(),
        documents: g.documents.clone(),
    }
}

/// Global corpus synthesis (W10.13): vraag over een héle verzameling (werkruimte): map over de
/// documentsamenvattingen (Readers, 1060) → reduce (hoofdmodel) met documentverwijzingen.
pub async fn corpus_synthesis(
    main: &Llm,
    aux: Option<&Llm>,
    store: &vortex_memory::MemoryStore,
    workspace: Option<&str>,
    question: &str,
) -> String {
    let docs = store.documents(workspace).unwrap_or_default();
    let mut notes = Vec::new();
    for (k, d) in docs.iter().take(40).enumerate() {
        let uri = d["uri"].as_str().unwrap_or("");
        let mems = store.by_source(uri).unwrap_or_default();
        let summary = mems
            .iter()
            .filter(|m| m.kind == MemoryKind::Summary)
            .max_by_key(|m| m.tags.iter().any(|t| t == "boek"))
            .map(|m| m.content.clone())
            .unwrap_or_else(|| {
                mems.iter()
                    .take(3)
                    .map(|m| m.content.chars().take(500).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        if summary.trim().is_empty() {
            continue;
        }
        let v = ask(
            aux.unwrap_or(main),
            "Wat zegt dit document over de vraag? Eén tot drie zinnen, of 'niets' als het niet relevant is.",
            format!("Vraag: {question}\n\nDocument: {}\n{summary}", d["title"].as_str().unwrap_or(uri)),
            200,
            None,
        )
        .await;
        let t = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
        if !t.to_lowercase().starts_with("niets") {
            notes.push(format!("[D{}] {} — {t}", k + 1, d["title"].as_str().unwrap_or(uri)));
        }
    }
    if notes.is_empty() {
        return "Geen documenten in deze verzameling zeggen iets over deze vraag.".into();
    }
    let v = ask(
        main,
        "Synthetiseer over de hele verzameling: patronen, overeenkomsten, verschillen; verwijs met [Dn]. Nederlands.",
        format!("Vraag: {question}\n\nNotities per document:\n{}", notes.join("\n")),
        1500,
        None,
    )
    .await;
    v.as_str().map(String::from).unwrap_or_else(|| v.to_string())
}
