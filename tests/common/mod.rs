//! Helpers shared by the integration tests (tests/common/mod.rs is not a test target).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn hk_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hk"))
}

pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn has_tool(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Is a usable C toolchain available on this host? (precondition for e2e tests)
pub fn gcc_available() -> bool {
    has_tool("gcc") && has_tool("ar")
}

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("hk-e2e-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create directory");
    }
    fs::write(path, content).expect("write file");
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Copy a directory tree (skipping build output) so an example can be built in a
/// temp directory without polluting the repository.
pub fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create destination directory");
    for entry in fs::read_dir(src).unwrap_or_else(|e| panic!("failed to read {}: {e}", src.display())) {
        let path = entry.expect("directory entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "build" || name == "target" || name == ".git" {
            continue;
        }
        let target = dst.join(&name);
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(&path, &target)
                .unwrap_or_else(|e| panic!("failed to copy {}: {e}", path.display()));
        }
    }
}

pub struct Run {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    pub fn assert_ok(&self, what: &str) -> &Self {
        assert!(self.ok, "{what} should succeed; output:\n{}", self.all());
        self
    }

    pub fn assert_failed(&self, what: &str) -> &Self {
        assert!(!self.ok, "{what} should fail; output:\n{}", self.all());
        self
    }

    pub fn assert_contains(&self, needle: &str) -> &Self {
        assert!(self.all().contains(needle), "output is missing {needle:?}:\n{}", self.all());
        self
    }
}

pub fn run_hk(dir: &Path, args: &[&str]) -> Run {
    run_hk_with_env(dir, args, &[])
}

/// Run hk with extra environment variables (used to simulate e.g. an unusable TEMP).
pub fn run_hk_with_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Run {
    let mut command = Command::new(hk_bin());
    command.args(args).current_dir(dir);
    for (key, value) in envs {
        command.env(key, value);
    }
    let out = command.output().expect("failed to run hk");
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}
