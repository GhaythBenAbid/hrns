//! Drawing. Same look as the desktop: quiet chrome, violet accent, steps folded into one line.

use crate::app::{short_home, tok, App, Chat, Overlay, COMMANDS};
use crate::md;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

pub mod theme {
    use ratatui::style::Color;
    pub const ACCENT: Color = Color::Rgb(167, 139, 250);
    pub const TEXT: Color = Color::Rgb(232, 232, 236);
    pub const MUTED: Color = Color::Rgb(133, 133, 143);
    pub const FAINT: Color = Color::Rgb(85, 85, 93);
    pub const BORDER: Color = Color::Rgb(50, 50, 56);
    pub const GREEN: Color = Color::Rgb(74, 222, 128);
    pub const RED: Color = Color::Rgb(248, 113, 113);
    pub const AMBER: Color = Color::Rgb(251, 191, 36);
    pub const CODE: Color = Color::Rgb(212, 212, 216);
    pub const BUBBLE: Color = Color::Rgb(26, 26, 29);
}
use theme::*;

const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    if area.width < 20 || area.height < 8 {
        f.render_widget(Paragraph::new(Span::styled("hrns · make the window bigger", Style::new().fg(MUTED))), area);
        return;
    }
    let input_lines = composer_lines(app, area.width.saturating_sub(6)).len().clamp(1, 8) as u16;
    let approval_h = app.c.live.as_ref().and_then(|l| l.approvals.first()).map(|a| approval_lines(a, area.width.saturating_sub(4)).len() as u16 + 3).unwrap_or(0);
    let [header, body, approval, composer, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(approval_h),
        Constraint::Length(input_lines + 2),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, app, header);
    let (details, tick, store) = (app.details, app.tick, app.store.clone());
    let focused_right = app.right;
    match app.panes_mut() {
        (only, None) => draw_chat(f, only, &store, details, tick, body, None),
        (left, Some(right)) => {
            // Side by side, a thin rule between; the focused pane's title bar wears the accent.
            let [l, rule, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Length(1), Constraint::Min(10)]).areas(body);
            draw_chat(f, left, &store, details, tick, l, Some(!focused_right));
            f.render_widget(Paragraph::new(vec![Line::from(Span::styled("│", Style::new().fg(BORDER))); rule.height as usize]), rule);
            draw_chat(f, right, &store, details, tick, r, Some(focused_right));
        }
    }
    if approval_h > 0 {
        draw_approval(f, app, approval);
    }
    draw_composer(f, app, composer);
    draw_footer(f, app, footer);
    if !app.suggest.is_empty() && app.overlay.is_none() {
        draw_suggest(f, app, composer);
    }
    match &app.overlay {
        Some(Overlay::Picker(_)) => draw_picker(f, app, area),
        Some(Overlay::Pager { .. }) => draw_pager(f, app, area),
        None => {}
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![
        Span::styled(" ▍", Style::new().fg(ACCENT)),
        Span::styled("hrns ", Style::new().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(short_home(&app.c.cwd), Style::new().fg(MUTED)),
    ];
    if let Some(b) = &app.c.branch {
        left.push(Span::styled(format!("  ⎇ {b}"), Style::new().fg(FAINT)));
    }
    let model = app.current_model().map(|m| m.name.clone()).unwrap_or_else(|| "…".into());
    let access = match app.c.access.as_str() {
        "full" => Span::styled("Full access", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        "read-only" => Span::styled("Read only", Style::new().fg(MUTED)),
        _ => Span::styled("Auto", Style::new().fg(MUTED)),
    };
    let mut right = vec![Span::styled(model, Style::new().fg(TEXT))];
    if let Some(e) = &app.c.effort {
        right.push(Span::styled(format!(" {e}"), Style::new().fg(FAINT)));
    }
    right.push(Span::styled(" · ", Style::new().fg(FAINT)));
    right.push(access);
    if let Some(e) = &app.c.email {
        right.push(Span::styled(format!(" · {}", e.split('@').next().unwrap_or(e)), Style::new().fg(FAINT)));
    }
    right.push(Span::raw(" "));
    let rw: usize = right.iter().map(|s| s.content.width()).sum();
    let lw: usize = left.iter().map(|s| s.content.width()).sum();
    let gap = (area.width as usize).saturating_sub(lw + rw);
    if gap > 0 {
        left.push(Span::raw(" ".repeat(gap)));
        left.extend(right);
    }
    f.render_widget(Paragraph::new(Line::from(left)), area);
}

/// One pane: a title bar when split (`focused` is Some), then the chat or the welcome screen.
fn draw_chat(f: &mut Frame, chat: &mut Chat, store: &crate::store::Store, details: bool, tick: usize, area: Rect, focused: Option<bool>) {
    let mut area = area;
    if let Some(on) = focused {
        let [bar, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        area = rest;
        let title = chat.thread.as_ref().map(|t| t.title.clone()).unwrap_or_else(|| "New chat".into());
        let folder = chat.cwd.rsplit('/').next().unwrap_or(&chat.cwd).to_owned();
        let model = chat.current_model().map(|m| m.name.clone()).unwrap_or_default();
        let mut spans = vec![
            Span::styled(if on { " ● " } else { " ○ " }, Style::new().fg(if on { ACCENT } else { FAINT })),
            Span::styled(format!("{folder} "), Style::new().fg(if on { TEXT } else { MUTED }).add_modifier(Modifier::BOLD)),
            Span::styled(truncate(&title, (bar.width as usize).saturating_sub(folder.width() + model.width() + 10)), Style::new().fg(if on { MUTED } else { FAINT })),
        ];
        // The unfocused pane can't show its approval card: say so in its title bar.
        if !on && chat.live.as_ref().is_some_and(|l| !l.approvals.is_empty()) {
            spans.push(Span::styled("  ⚠ needs approval · alt+arrow", Style::new().fg(AMBER).add_modifier(Modifier::BOLD)));
        } else if chat.busy() {
            spans.push(Span::styled(format!("  {}", SPIN[tick % SPIN.len()]), Style::new().fg(ACCENT)));
        }
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if bar.width as usize > used + model.width() + 1 {
            spans.push(Span::raw(" ".repeat(bar.width as usize - used - model.width() - 1)));
            spans.push(Span::styled(model, Style::new().fg(FAINT)));
        }
        let bg = if on { BUBBLE } else { ratatui::style::Color::Reset };
        f.render_widget(Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)), bar);
    }
    if chat.thread.as_ref().is_none_or(|t| t.messages.is_empty()) && chat.live.is_none() {
        draw_welcome(f, chat, store, area);
    } else {
        draw_transcript(f, chat, details, tick, area, focused.is_none());
    }
}

fn draw_welcome(f: &mut Frame, chat: &Chat, store: &crate::store::Store, area: Rect) {
    let mut lines = vec![
        Line::default(),
        Line::from(vec![Span::styled("  >_ ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::styled("hrns", Style::new().fg(TEXT).add_modifier(Modifier::BOLD))]),
        Line::from(Span::styled("  The real Codex agent on your ChatGPT plan.", Style::new().fg(MUTED))),
        Line::default(),
        Line::from(vec![Span::styled("  folder   ", Style::new().fg(FAINT)), Span::styled(short_home(&chat.cwd), Style::new().fg(TEXT))]),
        Line::from(vec![Span::styled("  model    ", Style::new().fg(FAINT)), Span::styled(chat.current_model().map(|m| m.name.clone()).unwrap_or_else(|| "loading…".into()), Style::new().fg(TEXT))]),
        Line::from(vec![Span::styled("  account  ", Style::new().fg(FAINT)), Span::styled(chat.email.clone().unwrap_or_else(|| "default".into()), Style::new().fg(TEXT))]),
        Line::default(),
    ];
    for (k, d) in [("/", "commands"), ("@", "mention a file"), ("ctrl+r", "resume a chat"), ("ctrl+\\", "split screen"), ("ctrl+o", "show every step")] {
        lines.push(Line::from(vec![Span::styled(format!("  {k:<8} "), Style::new().fg(ACCENT)), Span::styled(d, Style::new().fg(MUTED))]));
    }
    let recent: Vec<_> = store.list().into_iter().filter(|s| s.cwd.as_deref() == Some(chat.cwd.as_str())).take(3).collect();
    if !recent.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled("  Recent here", Style::new().fg(FAINT))));
        for s in recent {
            lines.push(Line::from(vec![Span::styled("  · ", Style::new().fg(FAINT)), Span::styled(s.title, Style::new().fg(MUTED)), Span::styled(format!("  {}", crate::app::ago(s.updated_at)), Style::new().fg(FAINT))]));
        }
    }
    f.render_widget(Paragraph::new(lines), area);
}

// ---------- transcript ----------

fn draw_transcript(f: &mut Frame, chat: &mut Chat, details: bool, tick: usize, area: Rect, show_title: bool) {
    let width = area.width.saturating_sub(2);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(t) = &chat.thread {
        // When split, the pane's title bar already names the chat.
        if show_title {
            lines.push(Line::from(vec![Span::styled(" ", Style::new()), Span::styled(t.title.clone(), Style::new().fg(MUTED).add_modifier(Modifier::BOLD))]));
        }
        for m in &t.messages {
            render_message(&mut lines, m.role == "user", &m.text, m.parts.as_deref(), m.error.as_deref(), width, details, false, 0);
        }
    }
    if let Some(l) = &chat.live {
        render_message(&mut lines, false, "", Some(&l.parts), None, width, details, true, tick);
        if l.parts.iter().all(|p| p["type"] == "command" || p["type"] == "tool" || p["type"] == "reasoning") {
            let note = l.notice.clone().unwrap_or_else(|| if l.parts.is_empty() { "Thinking".into() } else { String::new() });
            if !note.is_empty() {
                lines.push(Line::from(vec![Span::styled(format!(" {} ", SPIN[tick % SPIN.len()]), Style::new().fg(ACCENT)), Span::styled(note, Style::new().fg(MUTED))]));
            }
        }
    }
    lines.push(Line::default());

    let h = area.height as usize;
    let max_scroll = lines.len().saturating_sub(h);
    chat.scroll = chat.scroll.min(max_scroll);
    let top = max_scroll - chat.scroll;
    let shown: Vec<Line> = lines.into_iter().skip(top).take(h).collect();
    f.render_widget(Paragraph::new(shown), area.inner(ratatui::layout::Margin { horizontal: 1, vertical: 0 }));
    if chat.scroll > 0 {
        let label = format!(" ↓ {} more · pgdn ", chat.scroll);
        let w = label.width() as u16;
        let r = Rect { x: area.x + area.width.saturating_sub(w + 1), y: area.y + area.height - 1, width: w, height: 1 };
        f.render_widget(Paragraph::new(Span::styled(label, Style::new().fg(TEXT).bg(BUBBLE))), r);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_message(out: &mut Vec<Line<'static>>, user: bool, text: &str, parts: Option<&[Value]>, error: Option<&str>, width: u16, details: bool, live: bool, tick: usize) {
    out.push(Line::default());
    if user {
        let w = width as usize;
        for l in text.lines() {
            for wl in md::wrap_prefixed(vec![Span::styled(l.to_owned(), Style::new().fg(TEXT))], w, Span::styled("› ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)), Span::raw("  ")) {
                out.push(wl.style(Style::new().bg(BUBBLE)));
            }
        }
        return;
    }
    let owned;
    let parts: &[Value] = match parts {
        Some(p) => p,
        None => {
            owned = vec![serde_json::json!({ "type": "text", "text": text })];
            &owned
        }
    };
    // Steps (commands, tools, reasoning) fold into one line per run of work.
    let mut i = 0;
    while i < parts.len() {
        let kind = parts[i]["type"].as_str().unwrap_or_default();
        if matches!(kind, "command" | "tool" | "reasoning") {
            let start = i;
            while i < parts.len() && matches!(parts[i]["type"].as_str(), Some("command" | "tool" | "reasoning")) {
                i += 1;
            }
            render_steps(out, &parts[start..i], width, details, live && i == parts.len(), tick);
            continue;
        }
        match kind {
            "text" | "plan" => {
                let t = parts[i]["text"].as_str().unwrap_or_default();
                if !t.is_empty() {
                    // Same left edge as the steps and edits around it.
                    out.extend(md::render(t, width.saturating_sub(1)).into_iter().map(|mut l| {
                        l.spans.insert(0, Span::raw(" "));
                        l
                    }));
                }
            }
            "fileChange" => {
                for c in parts[i]["changes"].as_array().into_iter().flatten() {
                    let (path, ck, diff) = (c["path"].as_str().unwrap_or_default(), c["kind"].as_str().unwrap_or_default(), c["diff"].as_str().unwrap_or_default());
                    let (add, del) = diff_stat(ck, diff);
                    let verb = match ck {
                        "add" => "Created",
                        "delete" => "Deleted",
                        _ => "Edited",
                    };
                    out.push(Line::from(vec![
                        Span::styled(" ✎ ", Style::new().fg(ACCENT)),
                        Span::styled(format!("{verb} "), Style::new().fg(MUTED).add_modifier(Modifier::BOLD)),
                        Span::styled(rel(path), Style::new().fg(TEXT)),
                        Span::styled(format!(" +{add}"), Style::new().fg(GREEN)),
                        Span::styled(format!(" −{del}"), Style::new().fg(RED)),
                    ]));
                    if details {
                        render_diff(out, ck, diff, width);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(e) = error {
        let style = if e == "Stopped" { Style::new().fg(MUTED) } else { Style::new().fg(RED) };
        out.push(Line::from(Span::styled(format!(" {}", if e == "Stopped" { "■ Stopped" } else { e }), style)));
    }
}

fn render_steps(out: &mut Vec<Line<'static>>, steps: &[Value], width: u16, details: bool, live: bool, tick: usize) {
    let commands = steps.iter().filter(|p| p["type"] == "command").count();
    let tools = steps.iter().filter(|p| p["type"] == "tool").count();
    let failed = steps.iter().filter(|p| p["type"] == "command" && p["exitCode"].as_i64().is_some_and(|c| c != 0)).count();
    let mut summary = vec![];
    if commands > 0 {
        summary.push(format!("{commands} step{}", if commands > 1 { "s" } else { "" }));
    }
    if tools > 0 {
        summary.push(format!("{tools} tool call{}", if tools > 1 { "s" } else { "" }));
    }
    let mut head = vec![
        if live { Span::styled(format!(" {} ", SPIN[tick % SPIN.len()]), Style::new().fg(ACCENT)) } else { Span::styled(" • ", Style::new().fg(FAINT)) },
        Span::styled(if live { "Working" } else if commands + tools > 0 { "Worked" } else { "Thought" }, Style::new().fg(MUTED).add_modifier(Modifier::BOLD)),
    ];
    if !summary.is_empty() {
        head.push(Span::styled(format!(" · {}", summary.join(", ")), Style::new().fg(FAINT)));
    }
    if failed > 0 {
        head.push(Span::styled(format!(" · {failed} failed"), Style::new().fg(RED)));
    }
    // While working, show what it's doing right now.
    if live && !details {
        if let Some(last) = steps.iter().rev().find(|p| p["type"] != "reasoning") {
            head.push(Span::styled(format!("  {}", truncate(&describe(last), width as usize / 2)), Style::new().fg(FAINT)));
        }
    }
    out.push(Line::from(head));
    if !details {
        return;
    }
    for p in steps {
        match p["type"].as_str() {
            Some("reasoning") => {
                let t = p["text"].as_str().unwrap_or_default().replace("**", "");
                if !t.trim().is_empty() {
                    out.extend(md::wrap(vec![Span::styled(t, Style::new().fg(FAINT).add_modifier(Modifier::ITALIC))], width as usize, "   │ ", "   │ "));
                }
            }
            Some("command") => {
                let code = p["exitCode"].as_i64();
                let mark = match (p["status"].as_str(), code) {
                    (Some("inProgress"), _) => Span::styled("…", Style::new().fg(ACCENT)),
                    (_, Some(0)) => Span::styled("✓", Style::new().fg(GREEN)),
                    (_, Some(c)) => Span::styled(format!("✗ {c}"), Style::new().fg(RED)),
                    _ => Span::styled("·", Style::new().fg(FAINT)),
                };
                out.push(Line::from(vec![Span::styled("   ", Style::new()), mark, Span::styled(format!(" {}", truncate(&describe(p), width as usize - 8)), Style::new().fg(MUTED))]));
                if let Some(o) = p["output"].as_str().filter(|o| !o.trim().is_empty()) {
                    let tail: Vec<&str> = o.trim_end().lines().collect();
                    let from = tail.len().saturating_sub(6);
                    for l in &tail[from..] {
                        out.push(Line::from(Span::styled(format!("     {}", truncate(l, width as usize - 6)), Style::new().fg(FAINT))));
                    }
                }
            }
            Some("tool") => out.push(Line::from(Span::styled(format!("   ⚙ {}", p["title"].as_str().unwrap_or("tool")), Style::new().fg(MUTED)))),
            _ => {}
        }
    }
}

fn render_diff(out: &mut Vec<Line<'static>>, kind: &str, diff: &str, width: u16) {
    let lines: Vec<String> = match kind {
        "add" => diff.lines().map(|l| format!("+{l}")).collect(),
        "delete" => diff.lines().map(|l| format!("-{l}")).collect(),
        _ => diff.lines().map(str::to_owned).collect(),
    };
    let shown = lines.len().min(60);
    for l in &lines[..shown] {
        let style = if l.starts_with("@@") {
            Style::new().fg(ACCENT)
        } else if l.starts_with('+') && !l.starts_with("+++") {
            Style::new().fg(GREEN)
        } else if l.starts_with('-') && !l.starts_with("---") {
            Style::new().fg(RED)
        } else {
            Style::new().fg(FAINT)
        };
        out.push(Line::from(Span::styled(format!("   {}", truncate(l, width as usize - 4)), style)));
    }
    if lines.len() > shown {
        out.push(Line::from(Span::styled(format!("   … {} more lines (/diff)", lines.len() - shown), Style::new().fg(FAINT))));
    }
}

/// Same wording as the desktop: "Read foo.rs", "Searched “x”", else the command itself.
fn describe(p: &Value) -> String {
    if p["type"] == "tool" {
        return format!("Called {}", p["title"].as_str().unwrap_or("tool"));
    }
    let actions: Vec<&Value> = p["actions"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if !actions.is_empty() && actions.iter().all(|a| a["type"] != "unknown") {
        return actions
            .iter()
            .map(|a| match a["type"].as_str() {
                Some("read") => format!("Read {}", a["name"].as_str().map(str::to_owned).unwrap_or_else(|| base(a["path"].as_str()))),
                Some("listFiles") => format!("Listed {}", Some(base(a["path"].as_str())).filter(|s| !s.is_empty()).unwrap_or_else(|| ".".into())),
                _ => format!("Searched {}", a["query"].as_str().map(|q| format!("“{q}”")).unwrap_or_else(|| clean(a["command"].as_str().unwrap_or_default()))),
            })
            .collect::<Vec<_>>()
            .join(" · ");
    }
    format!("Ran {}", clean(p["command"].as_str().unwrap_or_default()))
}

fn clean(cmd: &str) -> String {
    let mut c = cmd.trim().to_owned();
    for shell in ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "zsh -lc ", "sh -c ", "/bin/sh -c "] {
        if let Some(rest) = c.strip_prefix(shell) {
            c = rest.trim().to_owned();
            if (c.starts_with('\'') && c.ends_with('\'')) || (c.starts_with('"') && c.ends_with('"')) {
                c = c[1..c.len() - 1].to_owned();
            }
            break;
        }
    }
    c.replace("rtk proxy ", "").replace("rtk ", "")
}

fn base(p: Option<&str>) -> String {
    p.map(|p| p.rsplit('/').find(|s| !s.is_empty()).unwrap_or(p).to_owned()).unwrap_or_default()
}

fn rel(p: &str) -> String {
    std::env::current_dir().ok().and_then(|d| p.strip_prefix(&format!("{}/", d.display())).map(str::to_owned)).unwrap_or_else(|| short_home(p))
}

fn truncate(s: &str, w: usize) -> String {
    let s = s.replace('\n', " ");
    if s.width() <= w {
        return s;
    }
    let mut out = String::new();
    for c in s.chars() {
        if out.width() + 2 > w {
            break;
        }
        out.push(c);
    }
    out + "…"
}

fn diff_stat(kind: &str, diff: &str) -> (usize, usize) {
    let lines = diff.trim_end_matches('\n').lines();
    match kind {
        "add" => (lines.count(), 0),
        "delete" => (0, lines.count()),
        _ => lines.fold((0, 0), |(a, d), l| if l.starts_with('+') && !l.starts_with("+++") { (a + 1, d) } else if l.starts_with('-') && !l.starts_with("---") { (a, d + 1) } else { (a, d) }),
    }
}

// ---------- approval, composer, footer ----------

fn approval_lines(a: &Value, width: u16) -> Vec<Line<'static>> {
    let mut l = vec![Line::from(Span::styled(
        match a["kind"].as_str() {
            Some("command") => "Allow Codex to run this command?",
            Some("fileChange") => "Allow Codex to make these changes?",
            _ => "Allow extra permissions?",
        },
        Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
    ))];
    if let Some(r) = a["reason"].as_str() {
        l.extend(md::wrap(vec![Span::styled(r.to_owned(), Style::new().fg(MUTED))], width as usize, "", ""));
    }
    if let Some(c) = a["command"].as_str() {
        l.push(Line::from(vec![Span::styled("$ ", Style::new().fg(ACCENT)), Span::styled(truncate(c, width as usize - 3), Style::new().fg(TEXT))]));
    }
    for ch in a["changes"].as_array().into_iter().flatten().take(6) {
        l.push(Line::from(Span::styled(format!("{}  {}", ch["kind"].as_str().unwrap_or("edit"), rel(ch["path"].as_str().unwrap_or_default())), Style::new().fg(MUTED))));
    }
    l.push(Line::from(vec![
        Span::styled("y", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(" approve   ", Style::new().fg(MUTED)),
        Span::styled("a", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(" always in this chat   ", Style::new().fg(MUTED)),
        Span::styled("n", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(" deny", Style::new().fg(MUTED)),
    ]));
    l
}

fn draw_approval(f: &mut Frame, app: &App, area: Rect) {
    let Some(a) = app.c.live.as_ref().and_then(|l| l.approvals.first()) else { return };
    let block = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::new().fg(AMBER)).title(Span::styled(" approval ", Style::new().fg(AMBER)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(approval_lines(a, inner.width)), inner.inner(ratatui::layout::Margin { horizontal: 1, vertical: 0 }));
}

/// The prompt, wrapped, with the cursor's (row, col).
fn composer_lines(app: &App, width: u16) -> Vec<String> {
    let w = width.max(4) as usize;
    let mut out = vec![];
    for line in app.input.split('\n') {
        let pieces = md::hard_wrap(line, w);
        out.extend(pieces);
    }
    out
}

fn cursor_pos(app: &App, width: u16) -> (u16, u16) {
    let w = width.max(4) as usize;
    let before: String = app.input.chars().take(app.cursor).collect();
    let mut row = 0usize;
    let mut col = 0usize;
    for (i, line) in before.split('\n').enumerate() {
        if i > 0 {
            row += 1;
        }
        let lw = line.width();
        row += lw / w;
        col = lw % w;
        if lw > 0 && lw % w == 0 && i == before.split('\n').count() - 1 {
            // Exactly at a wrap point: stay at the end of the line rather than the next.
            row -= 1;
            col = w;
        }
    }
    (row as u16, col as u16)
}

fn draw_composer(f: &mut Frame, app: &App, area: Rect) {
    let busy = app.busy();
    // Accent border in Full access (like the desktop composer), dimmer while Codex works.
    let border = if app.c.access == "full" { ACCENT } else if busy { BORDER } else { ratatui::style::Color::Rgb(78, 78, 88) };
    let block = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::new().fg(border));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [prompt, text] = Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
    f.render_widget(Paragraph::new(Span::styled("›", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))), Rect { x: prompt.x + 0, ..prompt });
    let width = text.width;
    if app.input.is_empty() {
        let ph = if busy { "Codex is working · esc to stop · you can type the next message" } else if app.c.thread.is_some() { "Reply · / commands · @ files" } else { "Ask Codex anything · / commands · @ files" };
        f.render_widget(Paragraph::new(Span::styled(ph, Style::new().fg(FAINT))), text);
    } else {
        let lines = composer_lines(app, width);
        let (row, col) = cursor_pos(app, width);
        let skip = (row as usize + 1).saturating_sub(text.height as usize);
        let shown: Vec<Line> = lines.into_iter().skip(skip).map(|l| Line::from(Span::styled(l, Style::new().fg(TEXT)))).collect();
        f.render_widget(Paragraph::new(shown), text);
        if app.overlay.is_none() {
            f.set_cursor_position(Position { x: text.x + col.min(width.saturating_sub(1)), y: text.y + row.saturating_sub(skip as u16).min(text.height.saturating_sub(1)) });
        }
        return;
    }
    if app.overlay.is_none() {
        f.set_cursor_position(Position { x: text.x, y: text.y });
    }
}

fn draw_suggest(f: &mut Frame, app: &App, composer: Rect) {
    let n = app.suggest.len().min(8) as u16;
    let h = n + 2;
    let w = composer.width.min(70);
    let area = Rect { x: composer.x, y: composer.y.saturating_sub(h), width: w, height: h };
    f.render_widget(Clear, area);
    let block = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::new().fg(BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let start = app.suggest_sel.saturating_sub(n as usize - 1);
    let lines: Vec<Line> = app
        .suggest
        .iter()
        .enumerate()
        .skip(start)
        .take(n as usize)
        .map(|(i, (v, d))| {
            let sel = i == app.suggest_sel;
            let style = if sel { Style::new().fg(TEXT).bg(BUBBLE).add_modifier(Modifier::BOLD) } else { Style::new().fg(MUTED) };
            let mut spans = vec![Span::styled(format!(" {v} "), if sel { style.fg(ACCENT) } else { style })];
            if !d.is_empty() {
                spans.push(Span::styled(format!(" {d}"), Style::new().fg(FAINT).bg(if sel { BUBBLE } else { ratatui::style::Color::Reset })));
            }
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![];
    if let Some((msg, t)) = &app.toast {
        if t.elapsed().as_secs() < 4 {
            left.push(Span::styled(format!(" {msg}"), Style::new().fg(AMBER)));
        }
    }
    if left.is_empty() {
        if let Some(s) = app.c.started {
            left.push(Span::styled(format!(" {} ", SPIN[app.tick % SPIN.len()]), Style::new().fg(ACCENT)));
            left.push(Span::styled(format!("Working {}s", s.elapsed().as_secs()), Style::new().fg(MUTED)));
            left.push(Span::styled("  esc stop", Style::new().fg(FAINT)));
        } else {
            left.push(Span::styled(" enter send · alt+enter newline · ctrl+o steps · /help", Style::new().fg(FAINT)));
        }
    }
    let mut right = vec![];
    if let Some(t) = &app.c.thread {
        let used: u64 = t.usage.values().map(|u| u.input + u.output).sum();
        if used > 0 {
            right.push(Span::styled(format!("{} tokens", tok(used)), Style::new().fg(FAINT)));
        }
    }
    if let Some(p) = app.c.limit {
        if !right.is_empty() {
            right.push(Span::styled(" · ", Style::new().fg(FAINT)));
        }
        right.push(Span::styled(format!("5h {}%", p.round()), Style::new().fg(if p > 80.0 { RED } else { MUTED })));
    }
    right.push(Span::raw(" "));
    let lw: usize = left.iter().map(|s| s.content.width()).sum();
    let rw: usize = right.iter().map(|s| s.content.width()).sum();
    let gap = (area.width as usize).saturating_sub(lw + rw);
    left.push(Span::raw(" ".repeat(gap)));
    left.extend(right);
    f.render_widget(Paragraph::new(Line::from(left)), area);
}

// ---------- overlays ----------

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(2));
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 3, width: w, height: h }
}

fn draw_picker(f: &mut Frame, app: &App, area: Rect) {
    let Some(Overlay::Picker(p)) = &app.overlay else { return };
    let rows = p.shown.len().clamp(1, 14) as u16;
    let r = centered(area, 76, rows + 4);
    f.render_widget(Clear, r);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(Span::styled(format!(" {} ", p.title), Style::new().fg(TEXT).add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(" ↑↓ choose · enter pick · esc close ", Style::new().fg(FAINT)));
    let inner = block.inner(r);
    f.render_widget(block, r);
    let [search, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
    let q = if p.query.is_empty() { Span::styled("type to filter", Style::new().fg(FAINT)) } else { Span::styled(p.query.clone(), Style::new().fg(TEXT)) };
    f.render_widget(Paragraph::new(vec![Line::from(vec![Span::styled(" ⌕ ", Style::new().fg(ACCENT)), q]), Line::from(Span::styled("─".repeat(search.width as usize), Style::new().fg(BORDER)))]), search);
    let h = list.height as usize;
    let start = p.sel.saturating_sub(h.saturating_sub(1));
    let lines: Vec<Line> = p
        .shown
        .iter()
        .enumerate()
        .skip(start)
        .take(h)
        .map(|(row, &i)| {
            let it = &p.items[i];
            let sel = row == p.sel;
            let w = list.width as usize;
            let label = truncate(&it.label, w.saturating_sub(it.detail.width().min(w / 2) + 4));
            let pad = w.saturating_sub(label.width() + it.detail.width().min(w / 2) + 3);
            let bg = if sel { BUBBLE } else { ratatui::style::Color::Reset };
            Line::from(vec![
                Span::styled(if sel { " ▸ " } else { "   " }, Style::new().fg(ACCENT).bg(bg)),
                Span::styled(label, Style::new().fg(if sel { TEXT } else { MUTED }).bg(bg).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() })),
                Span::styled(" ".repeat(pad), Style::new().bg(bg)),
                Span::styled(truncate(&it.detail, w / 2), Style::new().fg(FAINT).bg(bg)),
            ])
        })
        .collect();
    if lines.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("   Nothing matches", Style::new().fg(FAINT))), list);
    } else {
        f.render_widget(Paragraph::new(lines), list);
    }
    f.set_cursor_position(Position { x: search.x + 3 + p.query.width() as u16, y: search.y });
}

fn draw_pager(f: &mut Frame, app: &App, area: Rect) {
    let Some(Overlay::Pager { title, lines, scroll }) = &app.overlay else { return };
    let r = Rect { x: area.x + 2, y: area.y + 1, width: area.width.saturating_sub(4), height: area.height.saturating_sub(2) };
    f.render_widget(Clear, r);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(Span::styled(format!(" {title} "), Style::new().fg(TEXT).add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(format!(" {}/{} · ↑↓ space scroll · esc close ", (scroll + 1).min(lines.len()), lines.len()), Style::new().fg(FAINT)));
    let inner = block.inner(r);
    f.render_widget(block, r);
    let w = inner.width as usize - 2;
    let shown: Vec<Line> = lines
        .iter()
        .skip(*scroll)
        .take(inner.height as usize)
        .map(|l| {
            let style = if l.starts_with("@@") {
                Style::new().fg(ACCENT)
            } else if l.starts_with("+++") || l.starts_with("---") || l.starts_with("diff ") || l.starts_with("index ") {
                Style::new().fg(TEXT).add_modifier(Modifier::BOLD)
            } else if l.starts_with('+') {
                Style::new().fg(GREEN)
            } else if l.starts_with('-') {
                Style::new().fg(RED)
            } else if COMMANDS.iter().any(|(c, _)| l.trim_start().starts_with(c)) || l == "Keys" || l == "Commands" {
                Style::new().fg(TEXT)
            } else {
                Style::new().fg(MUTED)
            };
            Line::from(Span::styled(format!(" {}", truncate(l, w)), style))
        })
        .collect();
    f.render_widget(Paragraph::new(shown), inner);
}
