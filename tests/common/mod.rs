//! 集成测试共用的辅助函数（tests/common/mod.rs 不会被当成独立测试目标）。

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

/// 本机是否有可用的 C 工具链（端到端用例的前置条件）。
pub fn gcc_available() -> bool {
    has_tool("gcc") && has_tool("ar")
}

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("hk-e2e-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("创建临时目录");
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
        fs::create_dir_all(parent).expect("创建目录");
    }
    fs::write(path, content).expect("写入文件");
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()))
}

/// 递归复制目录树（跳过构建产物），用于在临时目录里构建示范工程，
/// 避免污染仓库工作区。
pub fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("创建目标目录");
    for entry in fs::read_dir(src).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", src.display())) {
        let path = entry.expect("目录项").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "build" || name == "target" || name == ".git" {
            continue;
        }
        let target = dst.join(&name);
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(&path, &target)
                .unwrap_or_else(|e| panic!("复制 {} 失败: {e}", path.display()));
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
        assert!(self.ok, "{what} 应成功，实际输出:\n{}", self.all());
        self
    }

    pub fn assert_failed(&self, what: &str) -> &Self {
        assert!(!self.ok, "{what} 应失败，实际输出:\n{}", self.all());
        self
    }

    pub fn assert_contains(&self, needle: &str) -> &Self {
        assert!(self.all().contains(needle), "输出中缺少 {needle:?}:\n{}", self.all());
        self
    }
}

pub fn run_hk(dir: &Path, args: &[&str]) -> Run {
    run_hk_with_env(dir, args, &[])
}

/// 带额外环境变量运行 hk（用于模拟 TEMP 不可用等环境）。
pub fn run_hk_with_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Run {
    let mut command = Command::new(hk_bin());
    command.args(args).current_dir(dir);
    for (key, value) in envs {
        command.env(key, value);
    }
    let out = command.output().expect("执行 hk 失败");
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}
