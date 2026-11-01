//! Git worktrees: a separate checkout (own folder, own branch) per chat, so agents working
//! in parallel never edit the same files. They're plain `git worktree`s, so `git worktree list`,
//! the terminal and any editor see them as usual.
//!
//! Layout: `<app data>/worktrees/<repo>/<slug>` on branch `hrns/<slug>`, branched from a base
//! branch of the project. Gitignored files the project needs (`.env`, …) are copied in when the
//! project lists them in a `.worktreeinclude` file (one literal path per line, same convention as
//! Orca and Claude Code).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Stored on a chat that runs in a worktree.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// The project's main checkout (what the sidebar groups by).
    pub root: String,
    pub branch: String,
    /// Branch it was created from.
    pub base: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub root: String,
    /// Checked-out branch of the main checkout (None when detached).
    pub current: Option<String>,
    /// Local branches, most recently committed first.
    pub branches: Vec<String>,
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.trim().trim_start_matches("fatal: ").to_owned())
    }
}

/// None when `dir` isn't in a git repository with at least one commit.
pub fn info(dir: &str) -> Option<RepoInfo> {
    let dir = Path::new(dir);
    // The main checkout, even when `dir` is itself a worktree.
    let common = git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok()?;
    let root = Path::new(&common).parent()?.to_string_lossy().into_owned();
    git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok()?;
    let current = git(Path::new(&root), &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok();
    let branches = git(
        Path::new(&root),
        &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads"],
    )
    .map(|s| s.lines().map(str::to_owned).collect())
    .unwrap_or_default();
    Some(RepoInfo { root, current, branches })
}

/// Short, branch-safe name from a chat's first message: "fix-the-login-redirect".
fn slug(text: &str) -> String {
    let mut s = String::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(5) {
        if s.len() + word.len() > 32 {
            break;
        }
        if !s.is_empty() {
            s.push('-');
        }
        s.push_str(&word.to_ascii_lowercase());
    }
    if s.is_empty() { "chat".into() } else { s }
}

/// Create a worktree of the repo at `project` on a new branch from `base`.
pub fn create(data: &Path, project: &str, base: &str, hint: &str) -> Result<(PathBuf, Worktree), String> {
    let info = info(project).ok_or("This project isn't a git repository with commits")?;
    let root = PathBuf::from(&info.root);
    let repo = root.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
    let dir = data.join("worktrees").join(repo);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let base_slug = slug(hint);
    let (mut name, mut n) = (base_slug.clone(), 1);
    // Unique among folders and branches.
    while dir.join(&name).exists() || git(&root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/hrns/{name}")]).is_ok() {
        n += 1;
        name = format!("{base_slug}-{n}");
    }
    let path = dir.join(&name);
    let branch = format!("hrns/{name}");
    git(&root, &["worktree", "add", "-b", &branch, &path.to_string_lossy(), base])?;
    copy_included(&root, &path);
    Ok((path, Worktree { root: info.root, branch, base: base.to_owned() }))
}

/// Copy the gitignored files listed in the project's `.worktreeinclude`.
fn copy_included(root: &Path, to: &Path) {
    let Ok(list) = std::fs::read_to_string(root.join(".worktreeinclude")) else { return };
    for line in list.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.contains("..") || line.contains(['*', '?', '[', '!']) {
            continue;
        }
        let rel = line.trim_matches('/');
        let (src, dst) = (root.join(rel), to.join(rel));
        if dst.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // `cp -c` makes an APFS clone (instant, no extra space); plain copy elsewhere.
        let cloned = cfg!(target_os = "macos")
            && Command::new("cp").args(["-cR"]).arg(&src).arg(&dst).status().is_ok_and(|s| s.success());
        if !cloned && src.is_file() {
            let _ = std::fs::copy(&src, &dst);
        }
    }
}

/// What removing a worktree would throw away.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub exists: bool,
    /// Uncommitted files (tracked edits and untracked files).
    pub dirty: usize,
    /// Commits on the branch that aren't on its base.
    pub ahead: usize,
}

pub fn check(path: &str, wt: &Worktree) -> Check {
    let p = Path::new(path);
    if !p.is_dir() {
        return Check { exists: false, dirty: 0, ahead: 0 };
    }
    let dirty = git(p, &["status", "--porcelain"]).map(|s| s.lines().count()).unwrap_or(0);
    let ahead = git(p, &["rev-list", "--count", &format!("{}..HEAD", wt.base)])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Check { exists: true, dirty, ahead }
}

/// Remove the worktree folder. Its branch is deleted only when nothing on it is unmerged, so
/// committed work is never lost; returns whether the branch was kept.
pub fn remove(path: &str, wt: &Worktree) -> Result<bool, String> {
    let root = Path::new(&wt.root);
    if Path::new(path).exists() {
        git(root, &["worktree", "remove", "--force", path])?;
    } else {
        let _ = git(root, &["worktree", "prune"]);
    }
    let merged = git(root, &["rev-list", "--count", &format!("{}..{}", wt.base, wt.branch)]).is_ok_and(|n| n == "0");
    if merged {
        let _ = git(root, &["branch", "-D", &wt.branch]);
    }
    Ok(!merged && git(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{}", wt.branch)]).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_remove() {
        let tmp = std::env::temp_dir().join(format!("hrns-wt-{}", std::process::id()));
        let repo = tmp.join("myrepo");
        std::fs::create_dir_all(&repo).unwrap();
        let g = |dir: &Path, args: &[&str]| git(dir, args).unwrap();
        g(&repo, &["init", "-q", "-b", "main"]);
        g(&repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join(".env"), "SECRET=1").unwrap();
        std::fs::write(repo.join(".git/info/exclude"), ".env\n").unwrap();
        std::fs::write(repo.join(".worktreeinclude"), ".env\n*.glob\n").unwrap();
        let project = repo.to_string_lossy().into_owned();

        let info = info(&project).unwrap();
        assert_eq!(info.current.as_deref(), Some("main"));

        // Untouched worktree: removing it deletes the branch too.
        let (path, wt) = create(&tmp, &project, "main", "Fix the thing").unwrap();
        assert_eq!(wt.branch, "hrns/fix-the-thing");
        assert_eq!(std::fs::read_to_string(path.join(".env")).unwrap(), "SECRET=1");
        // Info from inside a worktree points back at the main checkout.
        assert_eq!(super::info(&path.to_string_lossy()).unwrap().root, info.root);
        assert!(!remove(&path.to_string_lossy(), &wt).unwrap());
        assert!(!path.exists());

        // Same name again is fine once the first is gone; a live one gets a suffix.
        let (p1, _) = create(&tmp, &project, "main", "Fix the thing").unwrap();
        let (p2, wt2) = create(&tmp, &project, "main", "Fix the thing").unwrap();
        assert_ne!(p1, p2);
        assert_eq!(wt2.branch, "hrns/fix-the-thing-2");

        // A commit on the branch: the folder goes, the branch stays.
        std::fs::write(p2.join("a.txt"), "x").unwrap();
        g(&p2, &["add", "a.txt"]);
        g(&p2, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "work"]);
        std::fs::write(p2.join("b.txt"), "dirty").unwrap();
        let c = check(&p2.to_string_lossy(), &wt2);
        assert_eq!((c.dirty, c.ahead), (1, 1));
        assert!(remove(&p2.to_string_lossy(), &wt2).unwrap());
        assert!(!p2.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Fix the login redirect, please!"), "fix-the-login-redirect-please");
        assert_eq!(slug("¿¿"), "chat");
        assert!(slug("a very long message about refactoring everything in the codebase").len() <= 32);
    }
}
