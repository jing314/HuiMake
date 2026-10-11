//! 统一错误类型：带上下文链，出错时能直接看出"哪个模块 / 哪个阶段 / 哪条命令"。

use std::fmt;
use std::path::Path;

pub type Result<T> = std::result::Result<T, HkError>;

#[derive(Debug)]
pub struct HkError {
    message: String,
    context: Vec<String>,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl HkError {
    pub fn new(message: impl Into<String>) -> Self {
        HkError { message: message.into(), context: Vec::new(), source: None }
    }

    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        HkError { message: message.into(), context: Vec::new(), source: Some(Box::new(source)) }
    }

    pub fn io(action: &str, path: &Path, err: std::io::Error) -> Self {
        HkError::with_source(format!("{action} {} 失败", path.display()), err)
    }

    /// 命令执行失败：附带完整命令行、退出码与 stderr 尾部。
    pub fn command(cmd: &str, code: Option<i32>, stderr: &str) -> Self {
        let tail = stderr.trim();
        let detail = if tail.is_empty() { "（无错误输出）".to_string() } else { tail.to_string() };
        let mut e = HkError::new(match code {
            Some(c) => format!("命令执行失败（exit code {c}）:\n  {cmd}\n{detail}"),
            None => format!("命令被信号终止:\n  {cmd}\n{detail}"),
        });
        e.context.push("构建命令".to_string());
        e
    }

    /// 追加上下文；显示时最外层在前。
    pub fn context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }
}

impl fmt::Display for HkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 最外层上下文在最上面，根本原因在最后，便于顺着日志定位。
        let mut lines: Vec<String> = self.context.iter().rev().cloned().collect();
        lines.push(self.message.clone());
        if let Some(source) = &self.source {
            lines.push(format!("原因: {source}"));
        }
        write!(f, "{}", lines.join("\n  · "))
    }
}

impl std::error::Error for HkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|b| b.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for HkError {
    fn from(s: String) -> Self {
        HkError::new(s)
    }
}

impl From<&str> for HkError {
    fn from(s: &str) -> Self {
        HkError::new(s.to_string())
    }
}

impl From<std::io::Error> for HkError {
    fn from(e: std::io::Error) -> Self {
        HkError::with_source("IO 错误", e)
    }
}

impl From<serde_yaml::Error> for HkError {
    fn from(e: serde_yaml::Error) -> Self {
        HkError::with_source("YAML 解析失败", e)
    }
}

/// 给 Result 追加上下文，链式书写。
pub trait ResultExt<T> {
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

impl<T, E: Into<HkError>> ResultExt<T> for std::result::Result<T, E> {
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|e| e.into().context(f()))
    }
}

#[macro_export]
macro_rules! hk_bail {
    ($($arg:tt)*) => {
        return Err($crate::error::HkError::new(format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! hk_err {
    ($($arg:tt)*) => {
        $crate::error::HkError::new(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_chain_is_displayed_outermost_first() {
        let r: Result<()> = Err(HkError::new("内层"))
            .with_context(|| "中层".to_string())
            .with_context(|| "外层".to_string());
        let msg = r.unwrap_err().to_string();
        let outer = msg.find("外层").unwrap();
        let middle = msg.find("中层").unwrap();
        let inner = msg.find("内层").unwrap();
        assert!(outer < middle && middle < inner, "上下文链顺序错误: {msg}");
    }

    #[test]
    fn io_error_keeps_source() {
        use std::error::Error;
        let e = HkError::io(
            "读取",
            Path::new("/tmp/x"),
            std::io::Error::new(std::io::ErrorKind::NotFound, "no file"),
        );
        assert!(e.to_string().contains("/tmp/x"));
        assert!(e.source().is_some());
    }
}
