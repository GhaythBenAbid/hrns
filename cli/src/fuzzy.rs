//! Small fzf-like scorer for the pickers and @ file mentions: characters in order, with bonuses
//! for word starts and runs, and matches in the last path segment counting more.

pub fn score(query: &str, text: &str) -> Option<i64> {
    let q: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    if lower.len() != t.len() {
        return contains(&q, &lower);
    }
    let name_start = t.iter().rposition(|&c| c == '/').map_or(0, |p| p + 1);
    let mut score = 0i64;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, &c) in lower.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            let mut s = 16;
            let prev = if i == 0 { None } else { Some(t[i - 1]) };
            match prev {
                None => s += 10,
                Some(p) if "/-_. ".contains(p) => s += 9,
                Some(p) if p.is_lowercase() && t[i].is_uppercase() => s += 7,
                _ => {}
            }
            if let Some(l) = last {
                if l + 1 == i {
                    s += 6;
                } else {
                    s -= ((i - l - 1) as i64).min(8);
                }
            }
            if i >= name_start {
                s += 4;
            }
            score += s;
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| score - (t.len() as i64) / 8)
}

fn contains(q: &[char], text: &[char]) -> Option<i64> {
    let mut qi = 0;
    for &c in text {
        if qi < q.len() && c == q[qi] {
            qi += 1;
        }
    }
    (qi == q.len()).then_some(1)
}

/// Indices of `items` matching `query`, best first.
pub fn rank<T>(query: &str, items: &[T], key: impl Fn(&T) -> &str) -> Vec<usize> {
    let mut scored: Vec<(i64, usize)> = items.iter().enumerate().filter_map(|(i, it)| score(query, key(it)).map(|s| (s, i))).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}
