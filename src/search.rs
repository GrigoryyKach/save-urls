//! Library search: matches in the title, description and address,
//! typos, skipped letters and a query typed in the wrong keyboard layout ("пшерги" -> "github").

/// Lower case, with "ё" treated as "е".
fn norm_char(c: char) -> char {
    let l = c.to_lowercase().next().unwrap_or(c);
    if l == 'ё' { 'е' } else { l }
}

fn norm(s: &str) -> Vec<char> {
    s.chars().map(norm_char).collect()
}

const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
/// Russian ЙЦУКЕН, key by key in the same order as `EN`.
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";
/// Standard Windows Ukrainian layout, key by key in the same order as `EN`.
const UK: &str = "йцукенгшщзхїфівапролджєячсмитьбюґ";

/// The same text as if it had been typed in the other layout. Latin text becomes Russian and Ukrainian
/// variants; Cyrillic text (either layout) becomes Latin. Empty when there is nothing to convert.
pub fn swap_layouts(s: &str) -> Vec<String> {
    let en: Vec<char> = EN.chars().collect();
    let ru: Vec<char> = RU.chars().collect();
    let uk: Vec<char> = UK.chars().collect();
    let lower: Vec<char> = s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
    let has_latin_key = lower.iter().any(|c| en.contains(c));
    let has_cyr_key = lower.iter().any(|c| ru.contains(c) || uk.contains(c));
    let to_en = |c: char| -> char {
        match ru.iter().position(|&r| r == c).or_else(|| uk.iter().position(|&u| u == c)) {
            Some(i) => en[i],
            None => c,
        }
    };
    let to_cyr = |c: char, table: &[char]| -> char {
        match en.iter().position(|&e| e == c) {
            Some(i) => table[i],
            None => c,
        }
    };
    let mut out = Vec::new();
    if has_latin_key {
        out.push(lower.iter().map(|&c| to_cyr(c, &ru)).collect::<String>());
        out.push(lower.iter().map(|&c| to_cyr(c, &uk)).collect::<String>());
    }
    if has_cyr_key {
        out.push(lower.iter().map(|&c| to_en(c)).collect::<String>());
    }
    out
}

pub struct Query {
    /// For every query word: variants (as typed, in the other layout).
    tokens: Vec<Vec<Vec<char>>>,
}

impl Query {
    pub fn new(q: &str) -> Self {
        let tokens = q
            .split_whitespace()
            .map(|t| {
                let mut v = vec![norm(t)];
                for s in swap_layouts(t) {
                    let s = norm(&s);
                    if !v.contains(&s) {
                        v.push(s);
                    }
                }
                v
            })
            .collect();
        Self { tokens }
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Match score, or `None` if at least one query word is found nowhere.
    pub fn score(&self, title: &str, desc: &str, url: &str) -> Option<i32> {
        let fields = [(norm(title), 100.0), (norm(url), 70.0), (norm(desc), 55.0)];
        let mut total = 0;
        for variants in &self.tokens {
            let mut best = 0f32;
            for (k, t) in variants.iter().enumerate() {
                let penalty = if k == 0 { 1.0 } else { 0.9 };
                for (hay, w) in &fields {
                    best = best.max(token_score(hay, t, *w) * penalty);
                }
            }
            if best <= 0.0 {
                return None;
            }
            total += best.round() as i32;
        }
        Some(total)
    }

    /// Byte ranges of exact matches in `text`, for highlighting.
    pub fn highlights(&self, text: &str) -> Vec<(usize, usize)> {
        let chars: Vec<(usize, char)> = text.char_indices().map(|(i, c)| (i, norm_char(c))).collect();
        let hay: Vec<char> = chars.iter().map(|c| c.1).collect();
        let mut ranges = Vec::new();
        for variants in &self.tokens {
            for t in variants {
                if t.is_empty() || t.len() > hay.len() {
                    continue;
                }
                let mut from = 0;
                while let Some(p) = find(&hay[from..], t) {
                    let s = from + p;
                    let e = s + t.len();
                    let end_byte = if e < chars.len() { chars[e].0 } else { text.len() };
                    ranges.push((chars[s].0, end_byte));
                    from = e;
                }
            }
        }
        ranges.sort();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for r in ranges {
            match merged.last_mut() {
                Some(last) if r.0 <= last.1 => last.1 = last.1.max(r.1),
                _ => merged.push(r),
            }
        }
        merged
    }
}

fn token_score(hay: &[char], t: &[char], w: f32) -> f32 {
    if t.is_empty() || hay.is_empty() {
        return 0.0;
    }
    if let Some(pos) = find(hay, t) {
        let word_start = pos == 0 || !hay[pos - 1].is_alphanumeric();
        return w + if word_start { 15.0 } else { 0.0 };
    }
    if t.len() >= 3 {
        let allowed = if t.len() <= 5 { 1 } else { 2 };
        for word in hay.split(|c| !c.is_alphanumeric()).filter(|w| w.len() >= 2) {
            let prefix = &word[..word.len().min(t.len())];
            if levenshtein(t, word) <= allowed || (word.len() > t.len() && levenshtein(t, prefix) <= allowed) {
                return w * 0.45;
            }
        }
        if let Some(s) = subsequence(hay, t) {
            return w * 0.25 * s;
        }
    }
    0.0
}

fn find(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// The query letters occur in order and close to each other: "gthb" in "github".
/// Returns 0.5..1.0: the denser the match, the higher.
fn subsequence(hay: &[char], t: &[char]) -> Option<f32> {
    let mut best: Option<usize> = None;
    for start in 0..hay.len() {
        if hay[start] != t[0] {
            continue;
        }
        let mut j = 1;
        let mut i = start + 1;
        while j < t.len() && i < hay.len() {
            if hay[i] == t[j] {
                j += 1;
            }
            i += 1;
        }
        if j == t.len() {
            let span = i - start;
            best = Some(best.map_or(span, |b| b.min(span)));
        }
    }
    let span = best?;
    if span > t.len() * 2 + 1 {
        return None;
    }
    Some(1.0 - 0.5 * (span - t.len()) as f32 / (t.len() + 1) as f32)
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, title: &str, desc: &str, url: &str) -> Option<i32> {
        Query::new(q).score(title, desc, url)
    }

    #[test]
    fn exact_and_fields() {
        assert!(s("egui", "egui — GUI на Rust", "", "github.com/emilk/egui").is_some());
        assert!(s("гуи", "", "Простой ГУИ", "example.com").is_some());
        assert!(s("emilk", "x", "", "github.com/emilk/egui").is_some());
        assert!(s("нет", "x", "y", "z.com").is_none());
        // A match in the title matters more than one in the address.
        assert!(s("rust", "Rust book", "", "a.com").unwrap() > s("rust", "Book", "", "rust.org").unwrap());
    }

    #[test]
    fn all_words_must_match() {
        assert!(s("rust book", "The Rust book", "", "x.org").is_some());
        assert!(s("rust python", "The Rust book", "", "x.org").is_none());
    }

    #[test]
    fn yo_and_case() {
        assert!(s("ЕЛКА", "Ёлка новогодняя", "", "").is_some());
    }

    #[test]
    fn typos_and_layout() {
        assert!(s("githab", "GitHub", "", "github.com").is_some());
        assert!(s("пшерги", "GitHub", "", "github.com").is_some());
        assert!(s("ghbdtn", "Привет мир", "", "").is_some());
        assert!(s("gthb", "", "", "github.com").is_some());
        assert!(s("прогрмма", "Полезная программа", "", "").is_some());
    }

    #[test]
    fn ukrainian_layout() {
        // i, ], ' and ` are the keys of і, ї, є and ґ on the Ukrainian layout (ы, ъ, э, ё on the Russian one).
        assert_eq!(swap_layouts("s]'`"), vec!["ыъэё".to_string(), "іїєґ".to_string()]);
        assert_eq!(swap_layouts("іїєґ"), vec!["s]'`".to_string()]);
        // "привіт" typed on the Ukrainian layout is "ghbdsn" on the Latin keys, and the other way round.
        assert!(s("привіт", "", "", "ghbdsn.example.com").is_some());
        assert!(s("ghbdsn", "Привіт, світе", "", "").is_some());
        // The Russian behaviour is unchanged.
        assert!(s("ghbdtn", "Привет мир", "", "").is_some());
        assert!(s("пшерги", "GitHub", "", "github.com").is_some());
    }

    #[test]
    fn highlight_ranges() {
        let q = Query::new("hub");
        assert_eq!(q.highlights("GitHub hub"), vec![(3, 6), (7, 10)]);
        let q = Query::new("ёлк");
        let t = "Ёлка";
        let h = q.highlights(t);
        assert_eq!(&t[h[0].0..h[0].1], "Ёлк");
    }
}
