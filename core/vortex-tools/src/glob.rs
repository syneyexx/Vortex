//! Glob-matcher (`*`, `**`, `?`, `[abc]`, `{a,b}`) voor bestanden zoeken en Cursor-globs.

/// Past `path` (met `/`) op `pattern`? Patronen zonder `/` matchen ook op de bestandsnaam.
pub fn matches(pattern: &str, path: &str) -> bool {
    let path = path.replace('\\', "/");
    for p in expand_braces(pattern) {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let hit = if p.contains('/') {
            m(p.trim_start_matches("./").as_bytes(), path.trim_start_matches("./").as_bytes())
                || m(format!("**/{p}").as_bytes(), path.as_bytes())
        } else {
            let name = path.rsplit('/').next().unwrap_or(&path);
            m(p.as_bytes(), name.as_bytes())
        };
        if hit {
            return true;
        }
    }
    false
}

fn expand_braces(p: &str) -> Vec<String> {
    if let Some(i) = p.find('{')
        && let Some(j) = p[i..].find('}')
    {
        let (pre, inner, post) = (&p[..i], &p[i + 1..i + j], &p[i + j + 1..]);
        return inner.split(',').flat_map(|alt| expand_braces(&format!("{pre}{alt}{post}"))).collect();
    }
    vec![p.to_string()]
}

fn m(p: &[u8], s: &[u8]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    if p.starts_with(b"**") {
        let rest = p[2..].strip_prefix(b"/").unwrap_or(&p[2..]);
        if rest.is_empty() {
            return true;
        }
        // ** = nul of meer volledige segmenten.
        if m(rest, s) {
            return true;
        }
        for (i, c) in s.iter().enumerate() {
            if *c == b'/' && m(rest, &s[i + 1..]) {
                return true;
            }
        }
        return false;
    }
    match p[0] {
        b'*' => {
            for i in 0..=s.len() {
                if i > 0 && s[i - 1] == b'/' {
                    break;
                }
                if m(&p[1..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        b'?' => !s.is_empty() && s[0] != b'/' && m(&p[1..], &s[1..]),
        b'[' => {
            let Some(end) = p.iter().position(|c| *c == b']') else {
                return !s.is_empty() && s[0] == b'[' && m(&p[1..], &s[1..]);
            };
            if s.is_empty() {
                return false;
            }
            let set = &p[1..end];
            let (neg, set) = if set.first() == Some(&b'!') { (true, &set[1..]) } else { (false, set) };
            let mut hit = false;
            let mut k = 0;
            while k < set.len() {
                if k + 2 < set.len() && set[k + 1] == b'-' {
                    hit |= set[k] <= s[0] && s[0] <= set[k + 2];
                    k += 3;
                } else {
                    hit |= set[k] == s[0];
                    k += 1;
                }
            }
            hit != neg && m(&p[end + 1..], &s[1..])
        }
        c => !s.is_empty() && s[0] == c && m(&p[1..], &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn glob_cases() {
        assert!(matches("*.rs", "src/lib.rs"));
        assert!(matches("src/**/*.ts", "src/a/b/c.ts"));
        assert!(matches("src/**/*.ts", "src/c.ts"));
        assert!(!matches("src/*.ts", "src/a/c.ts"));
        assert!(matches("**/*.{ts,tsx}", "app/src/App.tsx"));
        assert!(matches("file?.[ch]", "file1.c"));
        assert!(!matches("*.md", "README.txt"));
        assert!(matches("app/**", "app/src/x.ts"));
    }
}
