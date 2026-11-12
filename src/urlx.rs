//! Link parsing: finding an address in copied text, the domain, a readable form, the page title from HTML.

/// Finds the first link in the text. Understands `https://...`, `http://...`, `www....`,
/// and if the whole text is a single "domain/path" (`github.com/user/repo`), prepends `https://`.
pub fn extract_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 16 * 1024 {
        return None;
    }
    let tokens: Vec<&str> = text.split(|c: char| c.is_whitespace()).filter(|t| !t.is_empty()).collect();
    for raw in &tokens {
        let t = clean_token(raw);
        let lower = t.to_ascii_lowercase();
        // The link may be glued to text: "look:https://...".
        for scheme in ["https://", "http://"] {
            if let Some(pos) = lower.find(scheme) {
                let cand = &t[pos..];
                if valid_host(host_part(&cand[scheme.len()..])) {
                    return Some(cand.to_string());
                }
            }
        }
        if lower.starts_with("www.") && valid_host(host_part(t)) {
            return Some(format!("https://{t}"));
        }
    }
    if tokens.len() == 1 {
        let t = clean_token(tokens[0]);
        if !t.contains('@') && !t.contains("://") && looks_like_domain(host_part(t)) {
            return Some(format!("https://{t}"));
        }
    }
    None
}

/// Strips quotes, brackets and punctuation around a link.
fn clean_token(t: &str) -> &str {
    let mut t = t.trim_matches(|c: char| matches!(c, '"' | '\'' | '<' | '>' | '«' | '»' | '“' | '”' | '`' | '[' | ']' | '{' | '}'));
    loop {
        let before = t.len();
        t = t.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        // Keep a closing bracket if the link contains an opening one (Wikipedia).
        if t.ends_with(')') && t.matches('(').count() < t.matches(')').count() {
            t = &t[..t.len() - 1];
        }
        t = t.trim_start_matches('(');
        if t.len() == before {
            return t;
        }
    }
}

/// The part after the scheme up to the path: `user@host:port`.
fn host_part(rest: &str) -> &str {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let auth = &rest[..end];
    let host = auth.rsplit('@').next().unwrap_or(auth);
    // Cut the port off, leave bracketed IPv6 alone.
    if host.starts_with('[') {
        return host;
    }
    host.split(':').next().unwrap_or(host)
}

fn valid_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || (host.starts_with('[') && host.ends_with(']')) {
        return true;
    }
    !host.is_empty()
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.')
}

/// Stricter than `valid_host`: the last label is letters (not "1.5" and not "file.txt" with version digits).
fn looks_like_domain(host: &str) -> bool {
    if !valid_host(host) || host.eq_ignore_ascii_case("localhost") || host.starts_with('[') {
        return false;
    }
    let tld = host.rsplit('.').next().unwrap_or("");
    let tld_ok = tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic);
    // Common "non-domains": file names.
    const FILE_EXT: [&str; 14] = ["txt", "exe", "png", "jpg", "jpeg", "gif", "pdf", "doc", "docx", "zip", "rs", "js", "md", "json"];
    tld_ok && !FILE_EXT.contains(&tld.to_ascii_lowercase().as_str())
}

/// Host without `www.`: `https://www.github.com/x` -> `github.com`.
pub fn domain(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = host_part(rest).to_lowercase();
    host.strip_prefix("www.").map(str::to_string).unwrap_or(host)
}

/// Address for display: without the scheme, `www.` and a trailing `/`.
pub fn pretty(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.strip_suffix('/').unwrap_or(rest).to_string()
}

/// Default title until the page is downloaded: the domain and the last piece of the path.
pub fn fallback_title(url: &str) -> String {
    let d = domain(url);
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("");
    let path = path.split(['?', '#']).next().unwrap_or("");
    let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if last.is_empty() || last.len() > 60 { d } else { format!("{d} — {}", percent_decode(last)) }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

// ───────────────────────── page title ─────────────────────────

/// Decodes the server response: UTF-8, otherwise Windows-1251 (if the header or meta says so), otherwise Latin-1.
pub fn decode_html(bytes: &[u8], content_type: &str) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
    let ct = content_type.to_ascii_lowercase();
    let is_1251 = ["windows-1251", "cp1251", "win-1251"].iter().any(|c| ct.contains(c) || head.contains(c));
    let declares_utf8 = ct.contains("utf-8") || head.contains("charset=\"utf-8") || head.contains("charset=utf-8");
    if is_1251 {
        bytes.iter().map(|&b| cp1251(b)).collect()
    } else if declares_utf8 {
        // A character cut in half at the end of the buffer is no reason to lose the whole text.
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn cp1251(b: u8) -> char {
    match b {
        0x00..=0x7F => b as char,
        0xC0..=0xFF => char::from_u32(0x0410 + (b as u32 - 0xC0)).unwrap_or('?'),
        0xA8 => 'Ё',
        0xB8 => 'ё',
        0xA0 => ' ',
        0x96 => '–',
        0x97 => '—',
        0xAB => '«',
        0xBB => '»',
        0xB9 => '№',
        0x93 | 0x94 => '"',
        0x91 | 0x92 => '\'',
        0x85 => '…',
        _ => '?',
    }
}

/// Page title: `<title>`, or `og:title` if there is none.
pub fn parse_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let from_title = (|| {
        let mut from = 0;
        // Skip <title> inside <svg> and the like: take the first non-empty one.
        while let Some(i) = lower[from..].find("<title") {
            let start = from + i;
            let after = &lower[start + 6..];
            // It must be exactly the <title> tag, not <titlebar>.
            if !after.starts_with(['>', ' ', '\t', '\n', '\r']) {
                from = start + 6;
                continue;
            }
            let gt = start + lower[start..].find('>')? + 1;
            let end = gt + lower[gt..].find("</title")?;
            let t = clean_text(&html[gt..end]);
            if !t.is_empty() {
                return Some(t);
            }
            from = end;
        }
        None
    })();
    from_title.or_else(|| meta_content(html, &lower, "og:title")).or_else(|| meta_content(html, &lower, "twitter:title"))
}

fn meta_content(html: &str, lower: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = lower[from..].find("<meta") {
        let start = from + i;
        let end = start + lower[start..].find('>')?;
        let tag_l = &lower[start..end];
        let tag = &html[start..end];
        let name_ok = [format!("property=\"{key}\""), format!("name=\"{key}\""), format!("property='{key}'"), format!("name='{key}'")]
            .iter()
            .any(|p| tag_l.contains(p.as_str()));
        if name_ok && let Some(c) = attr(tag, tag_l, "content") {
            let t = clean_text(&c);
            if !t.is_empty() {
                return Some(t);
            }
        }
        from = end;
    }
    None
}

fn attr(tag: &str, tag_l: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = tag_l[from..].find(name) {
        let pos = from + i;
        let rest = tag_l[pos + name.len()..].trim_start();
        let prev_ok = pos == 0 || tag_l.as_bytes()[pos - 1].is_ascii_whitespace();
        if prev_ok && let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            let q = rest.chars().next()?;
            let off = tag_l.len() - rest.len();
            if q == '"' || q == '\'' {
                let close = tag[off + 1..].find(q)?;
                return Some(tag[off + 1..off + 1 + close].to_string());
            }
        }
        from = pos + name.len();
    }
    None
}

/// Collapses whitespace, expands HTML entities, limits the length.
pub fn clean_text(s: &str) -> String {
    let decoded = decode_entities(s);
    let mut out = String::new();
    for w in decoded.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    if out.chars().count() > 200 {
        out = out.chars().take(199).collect::<String>() + "…";
    }
    out
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest.as_bytes().iter().take(12).position(|&c| c == b';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            "hellip" => Some('…'),
            "middot" => Some('·'),
            "bull" => Some('•'),
            "copy" => Some('©'),
            _ => {
                let num = ent.strip_prefix('#');
                num.and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => n.parse().ok(),
                })
                .and_then(char::from_u32)
            }
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_urls() {
        assert_eq!(extract_url("https://github.com/emilk/egui"), Some("https://github.com/emilk/egui".into()));
        assert_eq!(extract_url("  смотри: https://example.com/a?b=1.  "), Some("https://example.com/a?b=1".into()));
        assert_eq!(extract_url("(https://example.com)"), Some("https://example.com".into()));
        assert_eq!(
            extract_url("https://en.wikipedia.org/wiki/Rust_(programming_language)"),
            Some("https://en.wikipedia.org/wiki/Rust_(programming_language)".into())
        );
        assert_eq!(extract_url("www.rust-lang.org"), Some("https://www.rust-lang.org".into()));
        assert_eq!(extract_url("github.com/user/repo"), Some("https://github.com/user/repo".into()));
        assert_eq!(extract_url("t.me/durov"), Some("https://t.me/durov".into()));
        assert_eq!(extract_url("текст:https://habr.com/ru/"), Some("https://habr.com/ru/".into()));
        assert_eq!(extract_url("http://localhost:3000/x"), Some("http://localhost:3000/x".into()));
        assert_eq!(extract_url("https://сайт.рф/путь"), Some("https://сайт.рф/путь".into()));
    }

    #[test]
    fn rejects_non_urls() {
        assert_eq!(extract_url(""), None);
        assert_eq!(extract_url("просто текст"), None);
        assert_eq!(extract_url("версия 1.5"), None);
        assert_eq!(extract_url("1.5"), None);
        assert_eq!(extract_url("readme.txt"), None);
        assert_eq!(extract_url("me@mail.ru"), None);
        assert_eq!(extract_url("https://"), None);
        assert_eq!(extract_url("два слова example.com"), None);
    }

    #[test]
    fn domains_and_pretty() {
        assert_eq!(domain("https://www.GitHub.com:443/x"), "github.com");
        assert_eq!(domain("https://user@host.io/x"), "host.io");
        assert_eq!(pretty("https://www.example.com/"), "example.com");
        assert_eq!(pretty("http://a.b/c?d"), "a.b/c?d");
        assert_eq!(fallback_title("https://github.com/emilk/egui/"), "github.com — egui");
        assert_eq!(fallback_title("https://example.com"), "example.com");
        assert_eq!(fallback_title("https://ru.wikipedia.org/wiki/%D0%A0%D0%B6%D0%B0%D0%B2%D1%87%D0%B8%D0%BD%D0%B0"), "ru.wikipedia.org — Ржавчина");
    }

    #[test]
    fn titles() {
        assert_eq!(parse_title("<html><head><TITLE>\n  Hello &amp; world  </TITLE>").as_deref(), Some("Hello & world"));
        assert_eq!(parse_title("<title data-x=1>A&#39;B &#x2014; C</title>").as_deref(), Some("A'B — C"));
        assert_eq!(
            parse_title(r#"<meta property="og:title" content="OG &quot;title&quot;"><title></title>"#).as_deref(),
            Some("OG \"title\"")
        );
        assert_eq!(parse_title("<titlebar>x</titlebar>"), None);
        assert_eq!(parse_title(r#"<meta name='og:title' content='Single q'>"#).as_deref(), Some("Single q"));
        assert_eq!(decode_html(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2], "text/html; charset=windows-1251"), "Привет");
    }
}
