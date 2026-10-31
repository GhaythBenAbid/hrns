//! Just enough Markdown for agent replies (headings, lists, quotes, code fences, **bold**,
//! *italic*, `code`), plus word wrapping of styled text to the terminal width.

use crate::ui::theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// Render Markdown into wrapped lines of at most `width` columns.
pub fn render(src: &str, width: u16) -> Vec<Line<'static>> {
    let width = width.max(10) as usize;
    let mut out = Vec::new();
    let mut in_code = false;
    for raw in src.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            if in_code {
                let lang = line.trim_start().trim_start_matches('`').trim();
                if !lang.is_empty() {
                    out.push(Line::from(Span::styled(format!("  {lang}"), Style::new().fg(theme::FAINT))));
                }
            }
            continue;
        }
        if in_code {
            // Code keeps its spacing; long lines wrap hard.
            let style = Style::new().fg(theme::CODE);
            for chunk in hard_wrap(&line.replace('\t', "    "), width.saturating_sub(4)) {
                out.push(Line::from(vec![Span::styled("  │ ", Style::new().fg(theme::BORDER)), Span::styled(chunk, style)]));
            }
            continue;
        }
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.is_empty() {
            out.push(Line::default());
        } else if let Some(h) = heading(trimmed) {
            out.extend(wrap(inline(h, Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD)), width, "", ""));
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
            let pad = " ".repeat(indent.min(8));
            out.extend(wrap(inline(rest, Style::new().fg(theme::TEXT)), width, &format!("{pad}• "), &format!("{pad}  ")));
        } else if let Some((num, rest)) = numbered(trimmed) {
            let pad = " ".repeat(indent.min(8));
            let first = format!("{pad}{num}. ");
            let next = " ".repeat(first.chars().count());
            out.extend(wrap(inline(rest, Style::new().fg(theme::TEXT)), width, &first, &next));
        } else if let Some(rest) = trimmed.strip_prefix("> ").or_else(|| trimmed.strip_prefix('>')) {
            out.extend(wrap(inline(rest, Style::new().fg(theme::MUTED).add_modifier(Modifier::ITALIC)), width, "▏ ", "▏ "));
        } else if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') && trimmed.len() >= 3 {
            out.push(Line::from(Span::styled("─".repeat(width.min(40)), Style::new().fg(theme::BORDER))));
        } else {
            out.extend(wrap(inline(trimmed, Style::new().fg(theme::TEXT)), width, "", ""));
        }
    }
    // No trailing blank lines.
    while out.last().is_some_and(|l: &Line| l.spans.iter().all(|s| s.content.trim().is_empty())) {
        out.pop();
    }
    out
}

fn heading(s: &str) -> Option<&str> {
    let hashes = s.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes).then(|| s[hashes..].trim_start()).filter(|_| s[hashes..].starts_with(' '))
}

fn numbered(s: &str) -> Option<(&str, &str)> {
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = &s[digits..];
    rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")).map(|r| (&s[..digits], r))
}

/// Inline styles: **bold**, *italic* / _italic_, `code`, and [text](url) shown as its text.
pub fn inline(s: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, spans: &mut Vec<Span<'static>>, style: Style| {
        if !buf.is_empty() {
            spans.push(Span::styled(std::mem::take(buf), style));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == '`') {
                flush(&mut buf, &mut spans, base);
                let code: String = chars[i + 1..i + 1 + end].iter().collect();
                spans.push(Span::styled(code, Style::new().fg(theme::ACCENT)));
                i += end + 2;
                continue;
            }
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find(&chars, i + 2, &['*', '*']) {
                flush(&mut buf, &mut spans, base);
                let inner: String = chars[i + 2..end].iter().collect();
                spans.extend(inline(&inner, base.add_modifier(Modifier::BOLD)));
                i = end + 2;
                continue;
            }
        }
        if (c == '*' || c == '_') && chars.get(i + 1).is_some_and(|n| !n.is_whitespace()) && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == c).map(|p| p + i + 1) {
                if end > i + 1 {
                    flush(&mut buf, &mut spans, base);
                    let inner: String = chars[i + 1..end].iter().collect();
                    spans.extend(inline(&inner, base.add_modifier(Modifier::ITALIC)));
                    i = end + 1;
                    continue;
                }
            }
        }
        if c == '[' {
            if let Some(close) = chars[i + 1..].iter().position(|&x| x == ']').map(|p| p + i + 1) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = chars[close + 2..].iter().position(|&x| x == ')').map(|p| p + close + 2) {
                        flush(&mut buf, &mut spans, base);
                        let text: String = chars[i + 1..close].iter().collect();
                        spans.push(Span::styled(text, base.fg(theme::ACCENT).add_modifier(Modifier::UNDERLINED)));
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut spans, base);
    spans
}

fn find(chars: &[char], from: usize, pat: &[char; 2]) -> Option<usize> {
    (from..chars.len().saturating_sub(1)).find(|&j| chars[j] == pat[0] && chars[j + 1] == pat[1])
}

fn width_of(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Break a plain string into pieces of at most `width` columns.
pub fn hard_wrap(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = vec![String::new()];
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > width {
            out.push(String::new());
            w = 0;
        }
        out.last_mut().unwrap().push(c);
        w += cw;
    }
    out
}

/// Word-wrap styled spans. `first` prefixes the first line, `rest` the following ones.
pub fn wrap(spans: Vec<Span<'static>>, width: usize, first: &str, rest: &str) -> Vec<Line<'static>> {
    wrap_prefixed(spans, width, Span::styled(first.to_owned(), Style::new().fg(theme::MUTED)), Span::styled(rest.to_owned(), Style::new().fg(theme::MUTED)))
}

pub fn wrap_prefixed(spans: Vec<Span<'static>>, width: usize, first: Span<'static>, rest: Span<'static>) -> Vec<Line<'static>> {
    // Split into words (keeping the spaces attached to the word before them) with their styles.
    let mut words: Vec<(String, Style)> = Vec::new();
    for span in spans {
        let style = span.style;
        let mut cur = String::new();
        for c in span.content.chars() {
            cur.push(c);
            if c == ' ' {
                words.push((std::mem::take(&mut cur), style));
            }
        }
        if !cur.is_empty() {
            words.push((cur, style));
        }
    }
    let mut lines = Vec::new();
    let mut line: Vec<Span<'static>> = vec![first.clone()];
    let mut used = width_of(&first.content);
    let rest_w = width_of(&rest.content);
    for (word, style) in words {
        let ww = width_of(word.trim_end());
        if used + ww > width && used > rest_w.max(width_of(&first.content)) {
            lines.push(Line::from(std::mem::take(&mut line)));
            line.push(rest.clone());
            used = rest_w;
        }
        if used + ww > width {
            // A single word longer than the line: break it.
            for (k, piece) in hard_wrap(&word, width.saturating_sub(used).max(1)).into_iter().enumerate() {
                if k > 0 {
                    lines.push(Line::from(std::mem::take(&mut line)));
                    line.push(rest.clone());
                    used = rest_w;
                }
                used += width_of(&piece);
                line.push(Span::styled(piece, style));
            }
            continue;
        }
        used += width_of(&word);
        line.push(Span::styled(word, style));
    }
    lines.push(Line::from(line));
    lines
}
