//! Runner-tests met een nep-subject + validatie van de echte eval-sets in de repo.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use vortex_core::{Db, EventLog, db::CORE_MIGRATIONS};
use vortex_eval::*;

fn repo_evals() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals")
}

#[test]
fn repo_eval_sets_are_valid() {
    let suites = load_dir(&repo_evals().join("sets")).unwrap_or_else(|e| panic!("{e}"));
    assert!(suites.len() >= 5, "{}", suites.len());
    assert!(suites.iter().all(|s| !s.tasks.is_empty()));
    let total: usize = suites.iter().map(|s| s.tasks.len()).sum();
    assert!(total >= 30, "{total}");
    let r: RetrievalSet =
        serde_json::from_str(&std::fs::read_to_string(repo_evals().join("retrieval.json")).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
    let ids: Vec<&str> = r.corpus.iter().map(|d| d.id.as_str()).collect();
    assert!(r.questions.iter().all(|q| q.no_answer || q.relevant.iter().all(|x| ids.contains(&x.as_str()))));
}

struct Fake {
    good: bool,
    facts: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl Subject for Fake {
    fn version(&self) -> String {
        format!("fake-{}", self.good)
    }
    fn has(&self, f: &str) -> bool {
        f != "W08"
    }
    async fn prepare(&self, _ws: &str, facts: &[String]) -> Result<(), String> {
        *self.facts.lock().map_err(|e| e.to_string())? = facts.to_vec();
        Ok(())
    }
    async fn answer(&self, _ws: &str, prompt: &str, _m: Option<&str>) -> Result<String, String> {
        if !self.good {
            return Ok("geen idee".into());
        }
        Ok(match prompt {
            p if p.contains("17 × 23") => "Dat is 391".into(),
            p if p.contains("palindroom") => {
                "```python\ndef is_palindroom(s):\n    t=s.replace(' ','').lower()\n    return t==t[::-1]\n```".into()
            }
            _ => self.facts.lock().map(|f| f.join(" ")).unwrap_or_default(),
        })
    }
    async fn open_conflicts(&self, _ws: &str) -> usize {
        0
    }
    async fn cleanup(&self, _ws: &str) {}
}

fn suite() -> Suite {
    let t = |id: &str, prompt: &str, checks: Vec<Check>, req: Option<&str>| EvalTask {
        id: id.into(),
        prompt: prompt.into(),
        checks,
        memory: vec![],
        requires: req.map(String::from),
        mode: None,
        tags: vec![],
        difficulty: None,
    };
    let py = "assert is_palindroom('Lepel') and not is_palindroom('ab')";
    let mut mem = t("m", "waar?", vec![Check::Contains { all: vec!["Helix".into()] }], None);
    mem.memory = vec!["Mijn editor is Helix".into()];
    Suite {
        name: "mini".into(),
        tasks: vec![
            t("a", "Wat is 17 × 23?", vec![Check::Number { value: 391.0, tolerance: 0.0 }], None),
            t("b", "palindroom", vec![Check::PythonTests { tests: py.into() }], None),
            t("c", "tool", vec![Check::Contains { all: vec!["x".into()] }], Some("W08")),
            mem,
        ],
    }
}

#[tokio::test]
async fn run_store_and_compare() {
    let log = EventLog::new(Arc::new(Db::open_in_memory(CORE_MIGRATIONS).unwrap_or_else(|e| panic!("{e}"))));
    let py = if std::process::Command::new("python3").arg("-V").output().is_ok() { "python3" } else { "python" };
    let good = Fake { good: true, facts: Mutex::new(vec![]) };
    let bad = Fake { good: false, facts: Mutex::new(vec![]) };
    let s = suite();
    let r1 = Runner { log: &log, subject: &good, python: py.into() }
        .run_suite(&s, Some("goed"))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((r1.n, r1.passed, r1.skipped), (4, 3, 1));
    assert_eq!(r1.score, Some(1.0));
    let c1 = compare(&log, &r1.id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c1.verdict, "eerste meting");
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let r2 = Runner { log: &log, subject: &bad, python: py.into() }
        .run_suite(&s, Some("slecht"))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let c2 = compare(&log, &r2.id).unwrap_or_else(|e| panic!("{e}"));
    assert!(c2.verdict.starts_with("ROOD"), "{}", c2.verdict);
    assert_eq!(c2.regressions.len(), 3);
    assert_eq!(list_runs(&log, Some("mini"), 10).unwrap_or_default().len(), 2);
    let res = results(&log, &r2.id).unwrap_or_default();
    assert!(res.iter().any(|r| r.status == "skip" && r.detail.contains("W08")));
}

#[tokio::test]
async fn retrieval_eval_on_repo_set() {
    let set: RetrievalSet =
        serde_json::from_str(&std::fs::read_to_string(repo_evals().join("retrieval.json")).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
    let rep = vortex_eval::retrieval::run(
        &set,
        Arc::new(vortex_memory::HashEmbedder::new(256)),
        Arc::new(vortex_memory::HeuristicLlm),
        None,
    )
    .await
    .unwrap_or_else(|e| panic!("{e}"));
    // Ondergrens met de hash-embedder; het echte model hoort hier ruim boven te zitten.
    assert!(rep.recall_at_5 >= 0.8, "{rep:?}");
    assert!(rep.mrr > 0.5, "{rep:?}");
}
