//! Finds project folders on disk for the in-app project finder.

use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Serialize)]
pub struct Project {
    pub path: String,
    /// Last modified, ms since epoch (ranks the list before anything is typed).
    pub modified: u64,
}

/// Where people keep code. Scanned breadth-first; missing ones are skipped.
const ROOTS: &[&str] = &[
    "Desktop", "Documents", "Developer", "Projects", "projects", "Code", "code", "dev", "Dev", "src",
    "repos", "Repos", "work", "Work", "GitHub", "github", "git", "Sites", "workspace", "Workspace",
];
/// A folder holding any of these is a project; we don't descend further into it.
const MARKERS: &[&str] = &[
    ".git", "package.json", "Cargo.toml", "go.mod", "pyproject.toml", "requirements.txt", "Gemfile",
    "composer.json", "pom.xml", "build.gradle", "Package.swift", "deno.json", "pubspec.yaml", "mix.exs", "Makefile",
];
const SKIP: &[&str] = &["node_modules", "target", "dist", "build", "vendor", "Library", "Applications", "Pods", "venv", "__pycache__"];
const MAX_DEPTH: usize = 5;
const MAX_RESULTS: usize = 3000;
/// Bounds the walk on huge trees (Documents full of photos, etc.).
const MAX_VISITS: usize = 40_000;

pub fn find() -> Vec<Project> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return vec![] };
    let mut out = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = ROOTS.iter().map(|r| (home.join(r), 1)).filter(|(p, _)| p.is_dir()).collect();
    let mut visits = 0;

    while let Some((dir, depth)) = queue.pop_front() {
        visits += 1;
        if visits > MAX_VISITS || out.len() >= MAX_RESULTS {
            break;
        }
        if depth > 1 && is_project(&dir) {
            out.push(Project { path: dir.to_string_lossy().into_owned(), modified: modified(&dir) });
            continue;
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || SKIP.contains(&name.as_ref()) || name.ends_with(".app") {
                continue;
            }
            // file_type() doesn't follow symlinks, so linked folders can't loop the walk.
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                queue.push_back((entry.path(), depth + 1));
            }
        }
    }
    out
}

fn is_project(dir: &Path) -> bool {
    MARKERS.iter().any(|m| dir.join(m).exists())
}

fn modified(dir: &Path) -> u64 {
    std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Serialize)]
pub struct Icon {
    /// The project's own favicon/logo as a data URL.
    pub image: Option<String>,
    /// Framework or main language ("svelte", "rust", …), for a logo when there's no image.
    pub tech: Option<&'static str>,
}

pub fn icon(dir: &str) -> Icon {
    let dir = Path::new(dir);
    Icon { image: image(dir), tech: framework(dir).or_else(|| language(dir)) }
}

// Same lookup as T3 Code: well-known icon files, then the <link rel="icon"> of the entry HTML/root route.
const ICON_FILES: &[&str] = &[
    "favicon.svg", "favicon.ico", "favicon.png", "public/favicon.svg", "public/favicon.ico", "public/favicon.png",
    "app/favicon.ico", "app/favicon.png", "app/icon.svg", "app/icon.png", "app/icon.ico", "src/favicon.ico",
    "src/favicon.svg", "src/app/favicon.ico", "src/app/icon.svg", "src/app/icon.png", "assets/icon.svg",
    "assets/icon.png", "assets/logo.svg", "assets/logo.png", ".idea/icon.svg", "static/favicon.png",
    "static/favicon.svg", "src-tauri/icons/icon.png",
];
const ICON_SOURCES: &[&str] = &[
    "index.html", "public/index.html", "src/index.html", "src/app.html", "app/routes/__root.tsx",
    "src/routes/__root.tsx", "app/root.tsx", "src/root.tsx",
];
const MAX_IMAGE: u64 = 512 * 1024;

fn image(dir: &Path) -> Option<String> {
    ICON_FILES
        .iter()
        .find_map(|f| data_url(&dir.join(f)))
        .or_else(|| ICON_SOURCES.iter().find_map(|f| icon_href(&std::fs::read_to_string(dir.join(f)).ok()?)).and_then(|href| {
            let clean = href.trim_start_matches('/');
            ["public", "static", ""].iter().find_map(|base| data_url(&dir.join(base).join(clean)))
        }))
}

/// href of the first `<link rel="icon">` (or `{ rel: "icon", href: … }` in JS route heads).
fn icon_href(source: &str) -> Option<String> {
    let lower = source.to_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find("icon") {
        let i = at + i;
        at = i + 4;
        // The surrounding tag or object literal.
        let start = lower[..i].rfind(['<', '{']).unwrap_or(0);
        let end = lower[i..].find(['>', '}']).map_or(lower.len(), |e| i + e);
        let chunk = &source[start..end];
        let chunk_lower = &lower[start..end];
        if !chunk_lower.contains("rel") || chunk_lower.contains("apple-touch") || chunk_lower.contains("mask-icon") {
            continue;
        }
        let Some(h) = chunk_lower.find("href") else { continue };
        let rest = chunk[h + 4..].trim_start_matches([' ', '=', ':']);
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) else { continue };
        let value = &rest[1..];
        let Some(close) = value.find(quote) else { continue };
        let value = &value[..close];
        let value = value.split(['?', '#']).next().unwrap_or(value);
        if !value.is_empty() && !value.starts_with("http") && !value.starts_with("data:") && !value.contains('{') {
            return Some(value.to_owned());
        }
    }
    None
}

fn data_url(path: &Path) -> Option<String> {
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    };
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_IMAGE {
        return None;
    }
    Some(format!("data:{mime};base64,{}", base64(&std::fs::read(path).ok()?)))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn read(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file)).ok()
}

/// What the manifests say the project is built with. App shells win over UI frameworks
/// (a Tauri app with a Svelte UI is a Tauri app), frameworks over plain languages.
fn framework(dir: &Path) -> Option<&'static str> {
    if let Some(pkg) = read(dir, "package.json").and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
        let has = |name: &str| ["dependencies", "devDependencies"].iter().any(|k| pkg[k].get(name).is_some());
        const JS: &[(&str, &str)] = &[
            ("@tauri-apps/api", "tauri"), ("@tauri-apps/cli", "tauri"), ("electron", "electron"), ("expo", "expo"),
            ("next", "next"), ("nuxt", "nuxt"), ("astro", "astro"), ("@remix-run/react", "remix"),
            ("@sveltejs/kit", "svelte"), ("svelte", "svelte"), ("@angular/core", "angular"), ("solid-js", "solid"),
            ("@builder.io/qwik", "qwik"), ("vue", "vue"), ("react-native", "react"), ("react", "react"),
        ];
        if let Some((_, t)) = JS.iter().find(|(dep, _)| has(dep)) {
            return Some(t);
        }
    }
    if dir.join("src-tauri").is_dir() {
        return Some("tauri");
    }
    if let Some(cargo) = read(dir, "Cargo.toml") {
        return Some(if cargo.contains("tauri") { "tauri" } else { "rust" });
    }
    if dir.join("go.mod").exists() {
        return Some("go");
    }
    if let Some(pubspec) = read(dir, "pubspec.yaml") {
        return Some(if pubspec.contains("flutter") { "flutter" } else { "dart" });
    }
    if dir.join("Package.swift").exists() {
        return Some("swift");
    }
    if let Some(gems) = read(dir, "Gemfile") {
        return Some(if gems.contains("rails") { "rails" } else { "ruby" });
    }
    if let Some(composer) = read(dir, "composer.json") {
        return Some(if composer.contains("laravel/framework") { "laravel" } else { "php" });
    }
    if dir.join("mix.exs").exists() {
        return Some("elixir");
    }
    if dir.join("deno.json").exists() || dir.join("deno.jsonc").exists() {
        return Some("deno");
    }
    if dir.join("project.godot").exists() {
        return Some("godot");
    }
    if dir.join("manage.py").exists() || read(dir, "requirements.txt").is_some_and(|r| r.to_lowercase().contains("django")) {
        return Some("django");
    }
    if dir.join("pyproject.toml").exists() || dir.join("requirements.txt").exists() {
        return Some("python");
    }
    if dir.join("build.gradle.kts").exists() || read(dir, "build.gradle").is_some_and(|g| g.contains("kotlin")) {
        return Some("kotlin");
    }
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() {
        return Some("java");
    }
    if dir.join("package.json").exists() {
        return Some(if dir.join("tsconfig.json").exists() { "typescript" } else { "javascript" });
    }
    None
}

/// Most common source language by file count (shallow, bounded walk).
fn language(dir: &Path) -> Option<&'static str> {
    let mut counts: std::collections::HashMap<&'static str, usize> = Default::default();
    let mut queue = VecDeque::from([(dir.to_path_buf(), 0)]);
    let mut seen = 0;
    while let Some((d, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > 4000 {
                break;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || SKIP.contains(&name.as_ref()) {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if depth < 4 {
                    queue.push_back((entry.path(), depth + 1));
                }
            } else if let Some(lang) = name.rsplit_once('.').and_then(|(_, ext)| lang_of(ext)) {
                *counts.entry(lang).or_default() += 1;
            }
        }
    }
    counts.into_iter().max_by_key(|&(lang, n)| (n, lang)).map(|(lang, _)| lang)
}

fn lang_of(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "go" => "go",
        "py" => "python",
        "ts" | "tsx" | "mts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "svelte" => "svelte",
        "vue" => "vue",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "java" => "java",
        "dart" => "dart",
        "rb" => "ruby",
        "php" => "php",
        "ex" | "exs" => "elixir",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cplusplus",
        "cs" | "fs" => "dotnet",
        "lua" => "lua",
        "zig" => "zig",
        "hs" => "haskell",
        "scala" => "scala",
        "clj" | "cljs" => "clojure",
        "ml" => "ocaml",
        "jl" => "julia",
        "r" => "r",
        "pl" => "perl",
        "sh" | "bash" | "zsh" => "bash",
        "gleam" => "gleam",
        "html" => "html",
        "css" | "scss" | "sass" => "css",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_icon_links() {
        let html = r#"<link rel="apple-touch-icon" href="/apple.png"><link rel="stylesheet" href="/a.css"><link href="/favicon.svg?v=2" rel="icon">"#;
        assert_eq!(icon_href(html).as_deref(), Some("/favicon.svg"));
        let tsx = r#"links: [{ rel: "stylesheet", href: css }, { rel: "icon", href: "/logo.png" }]"#;
        assert_eq!(icon_href(tsx).as_deref(), Some("/logo.png"));
        assert_eq!(icon_href(r#"<link rel="icon" href="https://cdn.x/i.png">"#), None);
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    #[ignore = "reads the local disk; run with --ignored to eyeball results"]
    fn local_projects() {
        let found = find();
        eprintln!("{} projects", found.len());
        for p in found.iter().take(25) {
            let i = icon(&p.path);
            eprintln!("{:<60} image={:<5} tech={:?}", p.path, i.image.is_some(), i.tech);
        }
    }
}
