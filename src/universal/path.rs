use std::path::{Path, PathBuf};
use std::env;

fn resolve_to_absolute(relative_path: &Path) -> std::io::Result<PathBuf> {
    // 1. 获取当前工作目录
    let current_dir: PathBuf = env::current_dir()?;
    // 2. 将相对路径拼接上去
    Ok(current_dir.join(relative_path))
}
