//! robots.txt: groepen per user-agent, Allow/Disallow (langste match wint, `*` en `$`),
//! Crawl-delay.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Robots {
    rules: Vec<(bool, String)>,
    pub crawl_delay: Option<f64>,
}

fn matches(pattern: &str, path: &str) -> bool {
    let (pat, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pat.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if let Some(j) = path[pos..].find(part) {
            pos += j + part.len();
        } else {
            return false;
        }
    }
    !anchored || pos == path.len() || parts.last().is_some_and(|p| p.is_empty())
}

impl Robots {
    /// Parse voor onze user-agent (`token`, bv. "vortex"); valt terug op `*`.
    pub fn parse(txt: &str, token: &str) -> Self {
        let token = token.to_lowercase();
        let mut groups: Vec<(Vec<String>, Robots)> = Vec::new();
        let mut cur_agents: Vec<String> = Vec::new();
        let mut cur = Robots::default();
        let mut in_rules = false;
        for line in txt.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once(':') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_string());
            match k.as_str() {
                "user-agent" => {
                    if in_rules {
                        groups.push((std::mem::take(&mut cur_agents), std::mem::take(&mut cur)));
                        in_rules = false;
                    }
                    cur_agents.push(v.to_lowercase());
                }
                "allow" | "disallow" => {
                    in_rules = true;
                    if !v.is_empty() {
                        cur.rules.push((k == "allow", v));
                    }
                }
                "crawl-delay" => {
                    in_rules = true;
                    cur.crawl_delay = v.parse().ok();
                }
                _ => {}
            }
        }
        if !cur_agents.is_empty() {
            groups.push((cur_agents, cur));
        }
        groups
            .iter()
            .find(|(a, _)| a.iter().any(|x| x != "*" && token.contains(x.as_str())))
            .or_else(|| groups.iter().find(|(a, _)| a.iter().any(|x| x == "*")))
            .map(|(_, r)| r.clone())
            .unwrap_or_default()
    }

    pub fn allowed(&self, path: &str) -> bool {
        let best = self.rules.iter().filter(|(_, p)| matches(p, path)).max_by_key(|(allow, p)| (p.len(), *allow));
        best.is_none_or(|(allow, _)| *allow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_longest_match() {
        let r = Robots::parse(
            "User-agent: *\nDisallow: /private\nAllow: /private/open\nCrawl-delay: 2\n\nUser-agent: badbot\nDisallow: /\n",
            "Vortex/0.1",
        );
        assert!(!r.allowed("/private/x"));
        assert!(r.allowed("/private/open/y"));
        assert!(r.allowed("/public"));
        assert_eq!(r.crawl_delay, Some(2.0));
        let b = Robots::parse("User-agent: badbot\nDisallow: /\n", "badbot");
        assert!(!b.allowed("/x"));
        let w = Robots::parse("User-agent: *\nDisallow: /*.pdf$\n", "vortex");
        assert!(!w.allowed("/a/b.pdf") && w.allowed("/a/b.pdf?x=1"));
    }
}
