//! Unified error type with a context chain, so a failure shows which module,
//! which stage and which command went wrong.

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

    /// Build an error for a failed file system operation.
    pub fn io(action: &str, path: &Path, err: std::io::Error) -> Self {
        HkError::with_source(format!("{action} failed: {}", path.display()), err)
    }

    /// Build an error for a failed command: keeps the full command line, the
    /// exit code and the tail of stderr.
    pub fn command(cmd: &str, code: Option<i32>, stderr: &str) -> Self {
        let tail = stderr.trim();
        let detail = if tail.is_empty() { "(no error output)".to_string() } else { tail.to_string() };
        let mut e = HkError::new(match code {
            Some(c) => format!("command failed (exit code {c}):\n  {cmd}\n{detail}"),
            None => format!("command terminated by a signal:\n  {cmd}\n{detail}"),
        });
        e.context.push("build command".to_string());
        e
    }

    /// Add context. The outermost context is displayed first.
    pub fn context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }
}

impl fmt::Display for HkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Outermost context first, root cause last.
        let mut lines: Vec<String> = self.context.iter().rev().cloned().collect();
        lines.push(self.message.clone());
        if let Some(source) = &self.source {
            lines.push(format!("caused by: {source}"));
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
        HkError::with_source("I/O error", e)
    }
}

impl From<serde_yaml::Error> for HkError {
    fn from(e: serde_yaml::Error) -> Self {
        HkError::with_source("YAML parse error", e)
    }
}

/// Attach context to any Result.
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
        let r: Result<()> = Err(HkError::new("inner"))
            .with_context(|| "middle".to_string())
            .with_context(|| "outer".to_string());
        let msg = r.unwrap_err().to_string();
        let outer = msg.find("outer").unwrap();
        let middle = msg.find("middle").unwrap();
        let inner = msg.find("inner").unwrap();
        assert!(outer < middle && middle < inner, "wrong context order: {msg}");
    }

    #[test]
    fn io_error_keeps_source() {
        use std::error::Error;
        let e = HkError::io(
            "read",
            Path::new("/tmp/x"),
            std::io::Error::new(std::io::ErrorKind::NotFound, "no file"),
        );
        assert!(e.to_string().contains("/tmp/x"));
        assert!(e.source().is_some());
    }
}
