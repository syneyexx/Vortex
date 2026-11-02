//! Regel-diff (unified) voor previews van schrijfacties (W08.04).

/// Unified diff met `ctx` regels context. Grote bestanden: LCS op het gewijzigde middenstuk.
pub fn unified(path: &str, old: &str, new: &str, ctx: usize) -> String {
    if old == new {
        return String::new();
    }
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Gemeenschappelijk begin/eind eraf → LCS alleen op het middenstuk.
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf] {
        suf += 1;
    }
    let am = &a[pre..a.len() - suf];
    let bm = &b[pre..b.len() - suf];
    let mut ops: Vec<(char, &str)> = Vec::new();
    for l in &a[..pre] {
        ops.push((' ', l));
    }
    if am.len() * bm.len() > 4_000_000 {
        for l in am {
            ops.push(('-', l));
        }
        for l in bm {
            ops.push(('+', l));
        }
    } else {
        let (n, m) = (am.len(), bm.len());
        let mut dp = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                dp[i][j] = if am[i] == bm[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && am[i] == bm[j] {
                ops.push((' ', am[i]));
                i += 1;
                j += 1;
            } else if i < n && (j == m || dp[i + 1][j] >= dp[i][j + 1]) {
                ops.push(('-', am[i]));
                i += 1;
            } else {
                ops.push(('+', bm[j]));
                j += 1;
            }
        }
    }
    for l in &a[a.len() - suf..] {
        ops.push((' ', l));
    }
    // Hunks bouwen.
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, o)| o.0 != ' ').map(|(i, _)| i).collect();
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(ctx);
        let mut end = changed[k];
        while k < changed.len() && changed[k] <= end + 2 * ctx {
            end = changed[k];
            k += 1;
        }
        let end = (end + ctx + 1).min(ops.len());
        let (mut ao, mut bo) = (1, 1);
        for o in &ops[..start] {
            if o.0 != '+' {
                ao += 1;
            }
            if o.0 != '-' {
                bo += 1;
            }
        }
        let al = ops[start..end].iter().filter(|o| o.0 != '+').count();
        let bl = ops[start..end].iter().filter(|o| o.0 != '-').count();
        out.push_str(&format!("@@ -{ao},{al} +{bo},{bl} @@\n"));
        for o in &ops[start..end] {
            out.push(o.0);
            out.push_str(o.1);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn small_diff() {
        let d = super::unified("x.txt", "a\nb\nc\nd\n", "a\nB\nc\nd\ne\n", 1);
        assert!(d.contains("-b\n+B"), "{d}");
        assert!(d.contains("+e"), "{d}");
        assert!(d.starts_with("--- a/x.txt"));
        assert_eq!(super::unified("x", "same", "same", 3), "");
    }
}
