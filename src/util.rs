//! Path / process / file helpers.

use crate::error::{HkError, Result};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const VERBATIM_UNC: &str = "\\\\?\\UNC\\";
const VERBATIM: &str = "\\\\?\\";

/// Strip the Windows \\?\ prefix: such paths cannot be handed to tools like gcc/ar.
pub fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().to_string();
    if let Some(rest) = text.strip_prefix(VERBATIM_UNC) {
        return PathBuf::from(format!("\\\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(VERBATIM) {
        return PathBuf::from(rest.to_string());
    }
    path
}

/// Make a path absolute and normalized (a form external tools accept).
pub fn normalize(path: &Path) -> std::io::Result<PathBuf> {
    fs::canonicalize(path).map(strip_verbatim)
}

pub fn is_path_like(value: &str) -> bool {
    if value.contains('/') || value.contains('\\') {
        return true;
    }
    let lower = value.to_ascii_lowercase();
    [".a", ".so", ".dylib", ".lib", ".dll", ".o", ".obj"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Path of `path` relative to `base`, with / separators (used for glob matching
/// and for the object-file directory layout).
pub fn rel_to(base: &Path, path: &Path) -> String {
    let b: Vec<Component<'_>> = base.components().collect();
    let p: Vec<Component<'_>> = path.components().collect();
    let common = b.iter().zip(p.iter()).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &p[common..] {
        out.push(c.as_os_str());
    }
    out.to_string_lossy().replace('\\', "/")
}

pub fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| HkError::io("create directory", path, e))
}

/// Write only when the content differs, so a stale mtime cannot break
/// incremental builds.
pub fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if let Ok(old) = fs::read_to_string(path) {
        if old == content {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, content).map_err(|e| HkError::io("write", path, e))?;
    Ok(true)
}

pub fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| HkError::io("read", path, e))
}

pub fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| HkError::io("remove directory", path, e))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| HkError::io("remove file", path, e))?;
    }
    Ok(())
}

/// Should this directory be skipped (hidden, or explicitly excluded)?
fn is_pruned(path: &Path, skip_names: &[&str]) -> bool {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.starts_with('.') || skip_names.contains(&name),
        None => false,
    }
}

/// Collect every file below `dir` (sorted, so results are reproducible).
pub fn walk_files_pruned(dir: &Path, skip_names: &[&str]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).map_err(|e| HkError::io("read directory", &current, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| HkError::io("read directory entry", &current, e))?;
            let path = entry.path();
            if path.is_dir() {
                if !is_pruned(&path, skip_names) {
                    stack.push(path);
                }
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Collect `dir` and all of its subdirectories (for -I include paths).
pub fn walk_dirs_pruned(dir: &Path, skip_names: &[&str]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    out.push(dir.to_path_buf());
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).map_err(|e| HkError::io("read directory", &current, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| HkError::io("read directory entry", &current, e))?;
            let path = entry.path();
            if path.is_dir() {
                if is_pruned(&path, skip_names) {
                    continue;
                }
                out.push(path.clone());
                stack.push(path);
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Above this length the command line is passed through an @response file,
/// which avoids the Windows ~32K command line limit.
pub const RESPONSE_FILE_LIMIT: usize = 7000;

#[derive(Debug, Clone)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    /// Extra environment variables passed to the child process.
    pub envs: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub command: String,
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Cmd {
    pub fn new(program: impl Into<String>) -> Self {
        Cmd { program: program.into(), args: Vec::new(), cwd: None, envs: Vec::new() }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    /// Compilers, archivers and objcopy all need a writable temp directory (gcc writes
    /// its intermediates into TMPDIR). Point them at the build directory so the build
    /// never depends on whether the caller's TEMP is writable.
    pub fn temp_env(self, dir: &Path) -> Self {
        let value = dir.to_string_lossy().to_string();
        self.env("TMPDIR", value.clone())
            .env("TMP", value.clone())
            .env("TEMP", value)
    }

    /// Full command line, for reading and copying.
    pub fn display(&self) -> String {
        let mut out = shell_quote(&self.program);
        for a in &self.args {
            out.push(' ');
            out.push_str(&shell_quote(&a.to_string_lossy()));
        }
        out
    }

    fn total_len(&self) -> usize {
        self.program.len() + self.args.iter().map(|a| a.len() + 1).sum::<usize>()
    }

    /// Rewrite the command line as an @response file when it is too long.
    pub fn with_response_file(mut self, rsp: &Path) -> Result<Self> {
        if self.total_len() <= RESPONSE_FILE_LIMIT {
            return Ok(self);
        }
        let content = self
            .args
            .iter()
            .map(|a| response_quote(&a.to_string_lossy()))
            .collect::<Vec<_>>()
            .join("\n");
        write_if_changed(rsp, &content)?;
        self.args = vec![OsString::from(format!("@{}", rsp.display()))];
        Ok(self)
    }

    pub fn run(&self) -> Result<CmdOutput> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd.envs(self.envs.iter().map(|(k, v)| (k, v)));
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        let output = cmd.output().map_err(|e| self.spawn_error(e))?;
        Ok(CmdOutput {
            command: self.display(),
            success: output.status.success(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Run the command; on failure return an error carrying the command line.
    pub fn run_checked(&self, what: &str) -> Result<CmdOutput> {
        let out = self.run().map_err(|e| e.context(what.to_string()))?;
        if out.success {
            Ok(out)
        } else {
            Err(HkError::command(&out.command, out.code, &out.stderr).context(what.to_string()))
        }
    }

    /// Run with inherited stdio (used by `hk run` to execute the built program).
    pub fn run_inherit(&self) -> Result<Option<i32>> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd.envs(self.envs.iter().map(|(k, v)| (k, v)));
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        let status = cmd.status().map_err(|e| self.spawn_error(e))?;
        Ok(status.code())
    }

    fn spawn_error(&self, err: std::io::Error) -> HkError {
        if err.kind() == std::io::ErrorKind::NotFound {
            HkError::with_source(
                format!(
                    "executable not found: {:?} — check the toolchains section of hk.yaml, or make sure it is on PATH",
                    self.program
                ),
                err,
            )
        } else {
            HkError::with_source(format!("failed to spawn {}", self.command_line()), err)
        }
    }

    fn command_line(&self) -> String {
        self.display()
    }
}

fn shell_quote(text: &str) -> String {
    if text.is_empty() {
        return "\"\"".to_string();
    }
    if text.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("\"{}\"", text.replace('"', "\\\""))
    } else {
        text.to_string()
    }
}

/// Argument quoting for a GCC response file.
fn response_quote(text: &str) -> String {
    if text.is_empty() {
        return "\"\"".to_string();
    }
    if text.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '\\') {
        let mut out = String::with_capacity(text.len() + 2);
        out.push('"');
        for ch in text.chars() {
            if ch == '"' || ch == '\\' {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push('"');
        out
    } else {
        text.to_string()
    }
}

/// One-line shell form of a command, used by --print-plan.
pub fn plan_line(cmd: &Cmd) -> String {
    let mut line = cmd.display();
    if let Some(dir) = &cmd.cwd {
        line = format!("(cd {} && {})", dir.display(), line);
    }
    line
}

/// Write the plan to a file (--print-plan --output FILE).
pub fn write_plan(path: &Path, lines: &[String]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            ensure_dir(parent)?;
        }
    }
    let mut file = fs::File::create(path).map_err(|e| HkError::io("create plan file", path, e))?;
    for line in lines {
        writeln!(file, "{line}").map_err(|e| HkError::io("write plan file", path, e))?;
    }
    Ok(())
}

/// Short path form for log output.
pub fn short_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_to_handles_nesting() {
        let base = Path::new("/a/b");
        assert_eq!(rel_to(base, Path::new("/a/b/src/x.c")), "src/x.c");
        assert_eq!(rel_to(base, Path::new("/a/c/x.c")), "../c/x.c");
    }

    #[test]
    fn strip_verbatim_removes_prefix() {
        let p = PathBuf::from("\\\\?\\C:\\work\\a.c");
        assert_eq!(strip_verbatim(p), PathBuf::from("C:\\work\\a.c"));
    }

    #[test]
    fn is_path_like_detects_files() {
        assert!(is_path_like("libs/libfoo.a"));
        assert!(is_path_like("foo.dll"));
        assert!(!is_path_like("pthread"));
    }
}
