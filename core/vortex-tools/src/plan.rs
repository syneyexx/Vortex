//! Effectanalyse (W08.05): welke acties mogen tegelijk, welke moeten na elkaar.
//! Regels: expliciete en impliciete (`$ref`) afhankelijkheden; lezers mogen parallel; een
//! schrijver op hetzelfde doel of dezelfde benoemde resource komt ná eerdere acties daarop;
//! een schrijver zonder bekend doel is een barrière; externe acties van dezelfde tool na elkaar.

use crate::action::Action;
use crate::contract::{Effect, ResourceUse, ToolContract};

fn conflicts(a: &Action, ca: &ToolContract, b: &Action, cb: &ToolContract) -> bool {
    let writes = ca.effect != Effect::Read || cb.effect != Effect::Read;
    if !writes {
        return false;
    }
    if (ca.effect != Effect::Read && a.target.is_none()) || (cb.effect != Effect::Read && b.target.is_none()) {
        return true;
    }
    if a.target.is_some() && a.target == b.target {
        return true;
    }
    if ca.effect == Effect::External && cb.effect == Effect::External && ca.name == cb.name {
        return true;
    }
    ca.resources.iter().any(|r| matches!(r, ResourceUse::Named(_)) && cb.resources.contains(r))
}

/// Verdeel acties in fases; binnen een fase mag alles parallel. Indexen in de invoervolgorde.
/// Onbekende tools krijgen een eigen fase (ze falen toch bij uitvoeren).
pub fn stages(actions: &[Action], contract: impl Fn(&str) -> Option<ToolContract>) -> Vec<Vec<usize>> {
    let contracts: Vec<Option<ToolContract>> = actions.iter().map(|a| contract(&a.tool)).collect();
    let mut stage_of: Vec<usize> = vec![0; actions.len()];
    for i in 0..actions.len() {
        let mut s = 0;
        let deps: Vec<String> = actions[i].depends_on.iter().cloned().chain(actions[i].refs()).collect();
        for j in 0..i {
            let dep = deps.contains(&actions[j].id);
            let clash = match (&contracts[i], &contracts[j]) {
                (Some(ci), Some(cj)) => conflicts(&actions[i], ci, &actions[j], cj),
                _ => true,
            };
            if dep || clash {
                s = s.max(stage_of[j] + 1);
            }
        }
        stage_of[i] = s;
    }
    let n = stage_of.iter().copied().max().map_or(0, |m| m + 1);
    let mut out = vec![Vec::new(); n];
    for (i, s) in stage_of.into_iter().enumerate() {
        out[s].push(i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn c(name: &str, e: Effect) -> ToolContract {
        ToolContract::new(name, "", e, json!({}))
    }

    #[test]
    fn readers_parallel_writers_ordered() {
        let acts = vec![
            Action::new("r1", "read_file", json!({"path": "a"})),
            Action::new("r2", "read_file", json!({"path": "b"})),
            Action::new("w1", "write_file", json!({"path": "a"})),
            Action::new("w2", "write_file", json!({"path": "c"})),
            Action::new("r3", "read_file", json!({"path": {"$ref": "w2.data.path"}})),
        ];
        let st = stages(&acts, |n| Some(c(n, if n.starts_with("read") { Effect::Read } else { Effect::Local })));
        assert_eq!(st, vec![vec![0, 1, 3], vec![2, 4]]);
    }

    #[test]
    fn untargeted_writer_is_barrier() {
        let acts = vec![
            Action::new("a", "read_file", json!({"path": "a"})),
            Action::new("b", "python", json!({"code": "x"})),
            Action::new("c", "read_file", json!({"path": "b"})),
        ];
        let st = stages(&acts, |n| Some(c(n, if n == "python" { Effect::Local } else { Effect::Read })));
        assert_eq!(st, vec![vec![0], vec![1], vec![2]]);
    }
}
