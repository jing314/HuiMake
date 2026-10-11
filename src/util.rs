//! 路径 / 进程 / 文件工具。

use crate::error::{HkError, Result};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const VERBATIM_UNC: &str = "\\\\?\\UNC\\";
const VERBATIM: &str = "\\\\?\\";

/// 去掉 Windows 的 \\?\ 前缀：这类路径不能交给 gcc/ar 等外部工具。
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

/// 绝对化 + 归一化（外部工具安全的路径形式）。
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

/// 计算 path 相对 base 的路径，统一使用 / 分隔（用于匹配 glob、生成 obj 目录结构）。
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
    fs::create_dir_all(path).map_err(|e| HkError::io("创建目录", path, e))
}

/// 内容相同则不动文件（避免无意义地刷新 mtime，破坏增量构建）。
pub fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if let Ok(old) = fs::read_to_string(path) {
        if old == content {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, content).map_err(|e| HkError::io("写入", path, e))?;
    Ok(true)
}

pub fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| HkError::io("读取", path, e))
}

pub fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| HkError::io("删除目录", path, e))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| HkError::io("删除文件", path, e))?;
    }
    Ok(())
}

/// 是否跳过该目录（隐藏目录 / 显式排除名）。
fn is_pruned(path: &Path, skip_names: &[&str]) -> bool {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.starts_with('.') || skip_names.contains(&name),
        None => false,
    }
}

/// 递归收集目录下所有文件（不含目录自身），结果已排序，保证可复现。
pub fn walk_files_pruned(dir: &Path, skip_names: &[&str]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).map_err(|e| HkError::io("读取目录", &current, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| HkError::io("读取目录项", &current, e))?;
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

/// 递归收集目录及其所有子目录（用于 -I 头文件搜索路径）。
pub fn walk_dirs_pruned(dir: &Path, skip_names: &[&str]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    out.push(dir.to_path_buf());
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).map_err(|e| HkError::io("读取目录", &current, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| HkError::io("读取目录项", &current, e))?;
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
// 命令
// ---------------------------------------------------------------------------

/// 命令行超过该长度时改用 @响应文件，规避 Windows 32K 命令行上限。
pub const RESPONSE_FILE_LIMIT: usize = 7000;

#[derive(Debug, Clone)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    /// 额外传给子进程的环境变量。
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

    /// 编译器/归档器/objcopy 都需要一个可写的临时目录（gcc 会把中间文件写进 TMPDIR）。
    /// 这里统一指向构建目录下的 tmp，避免依赖调用者环境里的 TEMP 是否可写。
    pub fn temp_env(self, dir: &Path) -> Self {
        let value = dir.to_string_lossy().to_string();
        self.env("TMPDIR", value.clone())
            .env("TMP", value.clone())
            .env("TEMP", value)
    }

    /// 供人阅读 / 复制的完整命令行。
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

    /// 命令行过长时改写为 @rsp 形式。
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

    /// 成功则返回输出；失败则返回带命令行的错误。
    pub fn run_checked(&self, what: &str) -> Result<CmdOutput> {
        let out = self.run().map_err(|e| e.context(what.to_string()))?;
        if out.success {
            Ok(out)
        } else {
            Err(HkError::command(&out.command, out.code, &out.stderr).context(what.to_string()))
        }
    }

    /// 继承 stdio 运行（用于 hk run 执行用户程序）。
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
                    "找不到可执行文件 {:?}，请检查 hk.yaml 的 toolchains 配置或确认它已加入 PATH",
                    self.program
                ),
                err,
            )
        } else {
            HkError::with_source(format!("无法启动命令 {}", self.command_line()), err)
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

/// GCC 响应文件的参数转义。
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

/// 供 --print-plan 使用：把命令写成可直接执行的一行 shell。
pub fn plan_line(cmd: &Cmd) -> String {
    let mut line = cmd.display();
    if let Some(dir) = &cmd.cwd {
        line = format!("(cd {} && {})", dir.display(), line);
    }
    line
}

/// 把计划写入文件（--print-plan --output xxx）。
pub fn write_plan(path: &Path, lines: &[String]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            ensure_dir(parent)?;
        }
    }
    let mut file = fs::File::create(path).map_err(|e| HkError::io("创建计划文件", path, e))?;
    for line in lines {
        writeln!(file, "{line}").map_err(|e| HkError::io("写入计划文件", path, e))?;
    }
    Ok(())
}

/// 供日志使用的短路径显示。
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
