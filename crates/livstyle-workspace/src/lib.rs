//! Workspace isolation (ADR-012). Every agent run that may write files gets
//! its own `git worktree` on a `livstyle/<run>` branch, so concurrent runs,
//! a cancelled run, or a runtime that ignores its cwd cannot corrupt the
//! user's checkout. Changes reach the user's branch only through an explicit
//! review-and-merge step.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

const LOCK_FILE: &str = ".livstyle-lock";
const BRANCH_PREFIX: &str = "livstyle/";

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("{0} is not inside a git repository")]
    NotARepo(PathBuf),
    #[error("invalid run id {0:?}: use letters, digits, '-' or '_'")]
    InvalidRunId(String),
    #[error("worktree for run {0:?} already exists")]
    Exists(String),
    #[error("no worktree for run {0:?}")]
    NotFound(String),
    #[error("worktree {path} is locked by {holder}")]
    Locked { path: PathBuf, holder: String },
    #[error("git {args}: {stderr}")]
    Git { args: String, stderr: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WorkspaceError>;

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !out.status.success() {
        return Err(WorkspaceError::Git {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn valid_run_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub run_id: String,
    pub path: PathBuf,
    pub branch: String,
    /// Commit the worktree was created from; diffs are relative to it.
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeSummary {
    /// `git status --porcelain` lines relative to the worktree, including
    /// uncommitted and untracked files.
    pub status: Vec<String>,
    /// Commits on the run branch since `base`.
    pub commits: Vec<String>,
    /// Full diff against `base` (working tree included).
    pub diff: String,
}

impl ChangeSummary {
    pub fn is_empty(&self) -> bool {
        self.status.is_empty() && self.commits.is_empty()
    }
}

/// Exclusive writer lock on a worktree, released on drop. Uses `create_new`
/// so two processes cannot both acquire it.
#[derive(Debug)]
pub struct WriteLock {
    path: PathBuf,
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct WorkspaceManager {
    repo: PathBuf,
    root: PathBuf,
}

impl WorkspaceManager {
    /// `repo` is any path inside the user's repository; worktrees live under
    /// `root` (outside the repository, so tools that scan the repo do not
    /// see them).
    pub fn new(repo: impl AsRef<Path>, root: impl Into<PathBuf>) -> Result<Self> {
        let repo = repo.as_ref();
        let top = git(repo, &["rev-parse", "--show-toplevel"])
            .map_err(|_| WorkspaceError::NotARepo(repo.to_owned()))?;
        Ok(Self {
            repo: PathBuf::from(top.trim()),
            root: root.into(),
        })
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    fn path_for(&self, run_id: &str) -> PathBuf {
        self.root.join(run_id)
    }

    pub fn create(&self, run_id: &str, base_ref: &str) -> Result<Worktree> {
        if !valid_run_id(run_id) {
            return Err(WorkspaceError::InvalidRunId(run_id.into()));
        }
        let path = self.path_for(run_id);
        if path.exists() {
            return Err(WorkspaceError::Exists(run_id.into()));
        }
        fs::create_dir_all(&self.root)?;
        let base = git(
            &self.repo,
            &["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
        )?
        .trim()
        .to_owned();
        let branch = format!("{BRANCH_PREFIX}{run_id}");
        let path_str = path.to_string_lossy().into_owned();
        git(
            &self.repo,
            &["worktree", "add", "-b", &branch, &path_str, &base],
        )?;
        Ok(Worktree {
            run_id: run_id.into(),
            path,
            branch,
            base,
        })
    }

    pub fn get(&self, run_id: &str) -> Result<Worktree> {
        self.list()?
            .into_iter()
            .find(|w| w.run_id == run_id)
            .ok_or_else(|| WorkspaceError::NotFound(run_id.into()))
    }

    /// livstyle-managed worktrees (branch prefix `livstyle/`).
    pub fn list(&self) -> Result<Vec<Worktree>> {
        let out = git(&self.repo, &["worktree", "list", "--porcelain"])?;
        let mut result = Vec::new();
        for block in out.split("\n\n") {
            let mut path = None;
            let mut branch = None;
            for line in block.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(p));
                } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                    branch = Some(b.to_owned());
                }
            }
            let (Some(path), Some(branch)) = (path, branch) else {
                continue;
            };
            let Some(run_id) = branch.strip_prefix(BRANCH_PREFIX) else {
                continue;
            };
            let base = git(&self.repo, &["merge-base", "HEAD", &branch])
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            result.push(Worktree {
                run_id: run_id.to_owned(),
                path,
                branch: branch.clone(),
                base,
            });
        }
        Ok(result)
    }

    pub fn lock(&self, worktree: &Worktree, holder: &str) -> Result<WriteLock> {
        let path = worktree.path.join(LOCK_FILE);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                writeln!(f, "{holder} pid={}", std::process::id())?;
                f.sync_all()?;
                Ok(WriteLock { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(WorkspaceError::Locked {
                    path: worktree.path.clone(),
                    holder: fs::read_to_string(&path)
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn changes(&self, worktree: &Worktree) -> Result<ChangeSummary> {
        let status = git(
            &worktree.path,
            &["status", "--porcelain", "--untracked-files=all"],
        )?
        .lines()
        .filter(|l| !l.ends_with(LOCK_FILE))
        .map(str::to_owned)
        .collect();
        let commits = git(
            &worktree.path,
            &["log", "--oneline", &format!("{}..HEAD", worktree.base)],
        )?
        .lines()
        .map(str::to_owned)
        .collect();
        // Intent-to-add makes untracked files show up in the diff without staging content.
        let _ = git(
            &worktree.path,
            &[
                "add",
                "--intent-to-add",
                "--all",
                "--",
                ".",
                &format!(":!{LOCK_FILE}"),
            ],
        );
        let diff = git(&worktree.path, &["diff", &worktree.base])?;
        Ok(ChangeSummary {
            status,
            commits,
            diff,
        })
    }

    /// Removes the worktree. The branch is kept unless `delete_branch`, so a
    /// discarded run can still be inspected.
    pub fn remove(&self, worktree: &Worktree, delete_branch: bool) -> Result<()> {
        let lock = worktree.path.join(LOCK_FILE);
        if lock.exists() {
            return Err(WorkspaceError::Locked {
                path: worktree.path.clone(),
                holder: fs::read_to_string(&lock)
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            });
        }
        let path_str = worktree.path.to_string_lossy().into_owned();
        git(&self.repo, &["worktree", "remove", "--force", &path_str])?;
        if delete_branch {
            git(&self.repo, &["branch", "-D", &worktree.branch])?;
        }
        Ok(())
    }

    /// Drops administrative entries for worktrees whose directories vanished.
    pub fn prune(&self) -> Result<()> {
        git(&self.repo, &["worktree", "prune"])?;
        Ok(())
    }
}

/// Creates the lock file atomically; exposed for runners that work outside
/// a git repository (e.g. a scratch directory).
pub fn lock_dir(dir: &Path, holder: &str) -> Result<WriteLock> {
    let path = dir.join(LOCK_FILE);
    let mut f: File = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                WorkspaceError::Locked {
                    path: dir.to_owned(),
                    holder: fs::read_to_string(&path)
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                }
            } else {
                e.into()
            }
        })?;
    writeln!(f, "{holder} pid={}", std::process::id())?;
    Ok(WriteLock { path })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Repo {
        dir: PathBuf,
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn repo(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("livstyle-ws-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "t"],
            &["config", "commit.gpgsign", "false"],
        ] {
            git(&src, args).unwrap();
        }
        fs::write(src.join("main.c"), "int main(void) { return 0; }\n").unwrap();
        git(&src, &["add", "."]).unwrap();
        git(&src, &["commit", "-q", "-m", "init"]).unwrap();
        Repo { dir }
    }

    #[test]
    fn runs_are_isolated_from_each_other_and_the_checkout() {
        let r = repo("iso");
        let ws = WorkspaceManager::new(r.dir.join("src"), r.dir.join("trees")).unwrap();
        let a = ws.create("run-a", "HEAD").unwrap();
        let b = ws.create("run-b", "HEAD").unwrap();
        fs::write(a.path.join("main.c"), "changed by a\n").unwrap();
        fs::write(a.path.join("new.c"), "new\n").unwrap();

        assert_eq!(
            fs::read_to_string(b.path.join("main.c")).unwrap(),
            "int main(void) { return 0; }\n"
        );
        assert_eq!(
            fs::read_to_string(r.dir.join("src/main.c")).unwrap(),
            "int main(void) { return 0; }\n"
        );

        let ca = ws.changes(&a).unwrap();
        assert_eq!(ca.status.len(), 2, "{:?}", ca.status);
        assert!(ca.diff.contains("changed by a") && ca.diff.contains("new.c"));
        assert!(ws.changes(&b).unwrap().is_empty());

        let ids: Vec<_> = ws.list().unwrap().into_iter().map(|w| w.run_id).collect();
        assert_eq!(ids, ["run-a", "run-b"]);
    }

    #[test]
    fn single_writer_lock() {
        let r = repo("lock");
        let ws = WorkspaceManager::new(r.dir.join("src"), r.dir.join("trees")).unwrap();
        let w = ws.create("run-1", "main").unwrap();
        let lock = ws.lock(&w, "cursor-agent").unwrap();
        match ws.lock(&w, "octos") {
            Err(WorkspaceError::Locked { holder, .. }) => {
                assert!(holder.starts_with("cursor-agent"))
            }
            other => panic!("expected Locked, got {other:?}"),
        }
        assert!(matches!(
            ws.remove(&w, true),
            Err(WorkspaceError::Locked { .. })
        ));
        // The lock file never shows up as a change.
        assert!(ws.changes(&w).unwrap().is_empty());
        drop(lock);
        let _again = ws.lock(&w, "octos").unwrap();
    }

    #[test]
    fn remove_keeps_branch_unless_asked() {
        let r = repo("rm");
        let ws = WorkspaceManager::new(r.dir.join("src"), r.dir.join("trees")).unwrap();
        let w = ws.create("run-x", "HEAD").unwrap();
        ws.remove(&w, false).unwrap();
        assert!(!w.path.exists());
        assert!(git(ws.repo(), &["rev-parse", "--verify", "livstyle/run-x"]).is_ok());
        assert!(matches!(ws.get("run-x"), Err(WorkspaceError::NotFound(_))));
        let w2 = ws.create("run-y", "HEAD").unwrap();
        ws.remove(&w2, true).unwrap();
        assert!(git(ws.repo(), &["rev-parse", "--verify", "livstyle/run-y"]).is_err());
    }

    #[test]
    fn rejects_bad_ids_and_non_repos() {
        let r = repo("bad");
        let ws = WorkspaceManager::new(r.dir.join("src"), r.dir.join("trees")).unwrap();
        for bad in ["", "../escape", "a b", "x/y"] {
            assert!(matches!(
                ws.create(bad, "HEAD"),
                Err(WorkspaceError::InvalidRunId(_))
            ));
        }
        ws.create("dup", "HEAD").unwrap();
        assert!(matches!(
            ws.create("dup", "HEAD"),
            Err(WorkspaceError::Exists(_))
        ));
        let outside = r.dir.join("plain");
        fs::create_dir_all(&outside).unwrap();
        assert!(matches!(
            WorkspaceManager::new(&outside, r.dir.join("t")),
            Err(WorkspaceError::NotARepo(_))
        ));
    }
}
