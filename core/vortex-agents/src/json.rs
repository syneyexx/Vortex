//! JSON uit modeluitvoer halen (ook als het model er tekst omheen zet of ```json gebruikt).

pub fn extract(s: &str) -> Option<serde_json::Value> {
    let t = s.trim();
    if let Ok(v) = serde_json::from_str(t) {
        return Some(v);
    }
    let t = t.trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    if let Ok(v) = serde_json::from_str(t) {
        return Some(v);
    }
    let (a, b) = (t.find(['{', '[']), t.rfind(['}', ']']));
    match (a, b) {
        (Some(a), Some(b)) if b > a => serde_json::from_str(&t[a..=b]).ok(),
        _ => None,
    }
}

pub fn strings(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn tolerant() {
        assert_eq!(super::extract("```json\n{\"a\":1}\n```").unwrap()["a"], 1);
        assert_eq!(super::extract("Hier: {\"a\": [1]} klaar").unwrap()["a"][0], 1);
        assert!(super::extract("geen json").is_none());
    }
}
