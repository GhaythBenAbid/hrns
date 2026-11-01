//! Finishing a chat's work: commit, push, open a pull request, and merge a worktree's branch
//! back into its base. Everything is plain git (and the GitHub CLI for PRs when installed), run
//! in the chat's folder, so the result is the same as doing it by hand in a terminal.

use crate::worktree::Worktree;
use serde::Serialize;
use std::path::Path;
use std::process::Command;

fn run(bin: &str, dir: &Path, args: &[&str], path_env: &str) -> Result<String, String> {
    let out = Command::new(bin)
        .current_dir(dir)
        .args(args)
        .env("PATH", path_env)
        // Never wait on a credential prompt nobody can see.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .output()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { format!("{bin} isn't installed") } else { e.to_string() })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        Err(if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_owned() } else { err.trim_start_matches("fatal: ").to_owned() })
    }
}

pub struct Ctx<'a> {
    pub dir: &'a Path,
    pub worktree: Option<&'a Worktree>,
    pub path_env: &'a str,
}

impl Ctx<'_> {
    fn git(&self, args: &[&str]) -> Result<String, String> {
        run("git", self.dir, args, self.path_env)
    }
    fn git_in(&self, dir: &Path, args: &[&str]) -> Result<String, String> {
        run("git", dir, args, self.path_env)
    }
    fn gh(&self, args: &[&str]) -> Result<String, String> {
        run("gh", self.dir, args, self.path_env)
    }
    fn branch(&self) -> Option<String> {
        self.git(&["symbolic-ref", "--quiet", "--short", "HEAD"]).ok()
    }
    /// The worktree's base, else the remote's default branch, else main/master.
    fn base(&self) -> Option<String> {
        if let Some(wt) = self.worktree {
            return Some(wt.base.clone());
        }
        if let Ok(r) = self.git(&["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]) {
            return r.strip_prefix("origin/").map(str::to_owned);
        }
        ["main", "master"].into_iter().find(|b| self.git(&["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")]).is_ok()).map(Into::into)
    }
    fn count(&self, range: &str) -> usize {
        self.git(&["rev-list", "--count", range]).ok().and_then(|n| n.parse().ok()).unwrap_or(0)
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub branch: Option<String>,
    pub base: Option<String>,
    /// Uncommitted files.
    pub dirty: usize,
    /// Commits on this branch that aren't on the base.
    pub ahead: usize,
    /// Commits on the base this branch doesn't have yet.
    pub behind: usize,
    /// Commits not pushed yet (None when the branch was never pushed).
    pub unpushed: Option<usize>,
    pub has_remote: bool,
    pub is_worktree: bool,
}

pub fn status(c: &Ctx) -> Option<Status> {
    c.git(&["rev-parse", "--verify", "--quiet", "HEAD"]).ok()?;
    let branch = c.branch();
    let base = c.base();
    let dirty = c.git(&["status", "--porcelain"]).map(|s| s.lines().count()).unwrap_or(0);
    let (ahead, behind) = match &base {
        Some(b) if branch.as_deref() != Some(b.as_str()) => (c.count(&format!("{b}..HEAD")), c.count(&format!("HEAD..{b}"))),
        _ => (0, 0),
    };
    let unpushed = c.git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).ok().map(|_| c.count("@{u}..HEAD"));
    let has_remote = c.git(&["remote"]).is_ok_and(|r| r.lines().any(|l| l == "origin"));
    Some(Status { branch, base, dirty, ahead, behind, unpushed, has_remote, is_worktree: c.worktree.is_some() })
}

/// Stage everything and commit.
pub fn commit(c: &Ctx, message: &str) -> Result<String, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Write a commit message".into());
    }
    c.git(&["add", "-A"])?;
    c.git(&["commit", "--quiet", "-m", message])?;
    c.git(&["rev-parse", "--short", "HEAD"])
}

pub fn push(c: &Ctx) -> Result<(), String> {
    let branch = c.branch().ok_or("Not on a branch")?;
    c.git(&["push", "--quiet", "-u", "origin", &branch]).map(|_| ())
}

/// `https://github.com/owner/repo` from an ssh or https remote URL (GitHub-style hosts).
fn web_url(remote: &str) -> Option<String> {
    let r = remote.trim().trim_end_matches(".git");
    if let Some(rest) = r.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some(format!("https://{host}/{path}"));
    }
    if let Some(rest) = r.strip_prefix("ssh://git@") {
        return Some(format!("https://{}", rest.replacen(':', "/", 1)));
    }
    let rest = r.strip_prefix("https://")?;
    // Drop credentials in https://user:token@host/...
    let rest = match (rest.find('@'), rest.find('/')) {
        (Some(at), Some(slash)) if at < slash => &rest[at + 1..],
        _ => rest,
    };
    Some(format!("https://{rest}"))
}

pub enum Pr {
    /// Created or already open; the URL of the PR.
    Opened(String),
    /// No GitHub CLI: a compare page to open in the browser.
    Compare(String),
}

/// Push, then open a pull request into the base with the GitHub CLI, or fall back to the
/// host's compare page.
pub fn pull_request(c: &Ctx, title: &str, body: &str) -> Result<Pr, String> {
    let branch = c.branch().ok_or("Not on a branch")?;
    let base = c.base().ok_or("Couldn't tell which branch to open the PR against")?;
    if branch == base {
        return Err(format!("You're on {base}; a PR needs its own branch"));
    }
    push(c)?;
    match c.gh(&["pr", "view", &branch, "--json", "url,state", "-q", "select(.state == \"OPEN\") | .url"]) {
        Ok(url) if !url.is_empty() => return Ok(Pr::Opened(url)),
        _ => {}
    }
    match c.gh(&["pr", "create", "--base", &base, "--head", &branch, "--title", title, "--body", body]) {
        Ok(out) => Ok(Pr::Opened(out.lines().rev().find(|l| l.starts_with("http")).unwrap_or(&out).to_owned())),
        Err(e) if e.contains("isn't installed") || e.contains("gh auth login") => {
            let remote = c.git(&["remote", "get-url", "origin"])?;
            let web = web_url(&remote).ok_or("Unrecognized remote URL")?;
            Ok(Pr::Compare(format!("{web}/compare/{base}...{branch}?expand=1")))
        }
        Err(e) => Err(e),
    }
}

/// Merge a worktree's branch into its base in the project's main checkout. Refuses rather than
/// leaving anything half-done: uncommitted work, a main checkout on another branch or with its
/// own changes, and conflicts (the merge is aborted) all come back as errors.
pub fn merge(c: &Ctx) -> Result<String, String> {
    let wt = c.worktree.ok_or("Only chats in a worktree can be merged back")?;
    if c.git(&["status", "--porcelain"]).is_ok_and(|s| !s.is_empty()) {
        return Err("Commit the changes first".into());
    }
    let root = Path::new(&wt.root);
    if c.count(&format!("{}..{}", wt.base, wt.branch)) == 0 {
        return Err(format!("Nothing to merge: {} has no commits that aren't on {}", wt.branch, wt.base));
    }
    let on = c.git_in(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default();
    if on != wt.base {
        return Err(format!("The project folder is on {}. Switch it to {} to merge into it.", if on.is_empty() { "a detached HEAD" } else { &on }, wt.base));
    }
    if c.git_in(root, &["status", "--porcelain", "--untracked-files=no"]).is_ok_and(|s| !s.is_empty()) {
        return Err(format!("The project folder has uncommitted changes on {}. Commit or stash them first.", wt.base));
    }
    let msg = format!("Merge {} into {}", wt.branch, wt.base);
    if let Err(e) = c.git_in(root, &["merge", "--no-ff", "-m", &msg, &wt.branch]) {
        let _ = c.git_in(root, &["merge", "--abort"]);
        return Err(if e.contains("CONFLICT") || e.contains("conflict") || e.contains("Automatic merge failed") {
            format!("{} conflicts with {}. Nothing was changed. Ask the agent to merge {} into this branch and resolve the conflicts, then try again.", wt.branch, wt.base, wt.base)
        } else {
            e
        });
    }
    c.git_in(root, &["rev-parse", "--short", "HEAD"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_urls() {
        assert_eq!(web_url("git@github.com:a/b.git").unwrap(), "https://github.com/a/b");
        assert_eq!(web_url("https://github.com/a/b.git").unwrap(), "https://github.com/a/b");
        assert_eq!(web_url("https://x:tok@github.com/a/b").unwrap(), "https://github.com/a/b");
        assert_eq!(web_url("ssh://git@github.com/a/b.git").unwrap(), "https://github.com/a/b");
    }

    #[test]
    fn commit_and_merge() {
        let tmp = std::env::temp_dir().join(format!("hrns-ship-{}", std::process::id()));
        let repo = tmp.join("r");
        std::fs::create_dir_all(&repo).unwrap();
        let path = std::env::var("PATH").unwrap();
        let g = |d: &Path, a: &[&str]| run("git", d, a, &path).unwrap();
        g(&repo, &["init", "-q", "-b", "main"]);
        g(&repo, &["config", "user.name", "t"]);
        g(&repo, &["config", "user.email", "t@t"]);
        g(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let (wpath, wt) = crate::worktree::create(&tmp, &repo.to_string_lossy(), "main", "feature").unwrap();
        let c = Ctx { dir: &wpath, worktree: Some(&wt), path_env: &path };

        std::fs::write(wpath.join("f.txt"), "hi").unwrap();
        assert!(merge(&c).unwrap_err().contains("Commit"));
        let s = status(&c).unwrap();
        assert_eq!((s.dirty, s.ahead, s.base.as_deref()), (1, 0, Some("main")));
        commit(&c, "Add f").unwrap();
        let s = status(&c).unwrap();
        assert_eq!((s.dirty, s.ahead, s.unpushed), (0, 1, None));

        merge(&c).unwrap();
        assert!(repo.join("f.txt").exists());
        assert!(merge(&c).unwrap_err().contains("Nothing to merge"));

        // Conflict: both sides change the same file; the merge is aborted cleanly.
        std::fs::write(repo.join("f.txt"), "main side").unwrap();
        g(&repo, &["commit", "-qam", "main edit"]);
        std::fs::write(wpath.join("f.txt"), "branch side").unwrap();
        commit(&c, "branch edit").unwrap();
        assert!(merge(&c).unwrap_err().contains("conflicts"));
        assert!(g(&repo, &["status", "--porcelain"]).is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
