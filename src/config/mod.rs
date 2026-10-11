//! 配置模型。
//!
//! * module.rs  : 模块级 config.yaml（每个模块一个）
//! * global.rs  : 工程级 hk.yaml（工具链 / 目标 / 配置档）
//!
//! 所有编译器、架构参数、优化参数都来自 YAML，Rust 代码中不写死任何工具名与编译参数。

pub mod global;
pub mod module;

pub use global::{GlobalConfig, Profile, Target, Toolchain};
pub use module::{Config, ConfigView};

use crate::error::{HkError, Result};
use serde::de::DeserializeOwned;

/// 工程级配置文件名（按顺序查找）。
pub const GLOBAL_FILE_NAMES: [&str; 2] = ["hk.yaml", "project.yaml"];
/// 模块级配置文件名。
pub const MODULE_FILE_NAME: &str = "config.yaml";
/// 内置默认工程配置。
pub const DEFAULT_GLOBAL_YAML: &str = include_str!("../defaults/hk.yaml");

/// 占位符前缀，写作 concat 是为了避免源码里直接出现美元加大括号。
pub const PH_OPEN: &str = concat!("$", "{");

/// 链接后的额外产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PostAction {
    /// 保留/命名为 .elf（链接产物本身）
    Elf,
    /// objcopy -O binary
    Bin,
    /// objcopy -O ihex
    Hex,
    /// 链接 map 文件
    Map,
    /// size 报告
    Size,
    /// objdump 反汇编清单
    Lst,
}

/// 产物类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    #[default]
    Executable,
    StaticLib,
    SharedLib,
    Object,
    HeaderOnly,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Executable => "executable",
            Kind::StaticLib => "static-lib",
            Kind::SharedLib => "shared-lib",
            Kind::Object => "object",
            Kind::HeaderOnly => "header-only",
        }
    }

    /// 是否需要链接（而不是只产出 .o 或什么都不产出）。
    pub fn links(self) -> bool {
        matches!(self, Kind::Executable | Kind::SharedLib)
    }
}

/// 宿主平台名（可用于配置中的 OS 占位符）。
pub fn host_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// 文件名约定：可由 toolchain / target 覆盖，缺省按宿主平台推断。
#[derive(Debug, Clone)]
pub struct Naming {
    pub static_prefix: String,
    pub static_suffix: String,
    pub shared_prefix: String,
    pub shared_suffix: String,
    pub exe_suffix: String,
    pub obj_suffix: String,
}

impl Naming {
    pub fn platform_default() -> Self {
        let shared_suffix = if cfg!(target_os = "windows") {
            ".dll"
        } else if cfg!(target_os = "macos") {
            ".dylib"
        } else {
            ".so"
        };
        Naming {
            static_prefix: "lib".to_string(),
            static_suffix: ".a".to_string(),
            shared_prefix: "lib".to_string(),
            shared_suffix: shared_suffix.to_string(),
            exe_suffix: if cfg!(target_os = "windows") { ".exe".to_string() } else { String::new() },
            obj_suffix: ".o".to_string(),
        }
    }

    pub fn static_lib_name(&self, name: &str) -> String {
        format!("{}{}{}", self.static_prefix, name, self.static_suffix)
    }

    pub fn shared_lib_name(&self, name: &str) -> String {
        format!("{}{}{}", self.shared_prefix, name, self.shared_suffix)
    }

    pub fn executable_name(&self, name: &str) -> String {
        format!("{name}{}", self.exe_suffix)
    }
}

/// 占位符表：让同一份 YAML 在 Windows / Linux / macOS 都能用。
fn placeholder_table() -> Vec<(&'static str, String)> {
    let shared_suffix = if cfg!(target_os = "windows") {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    };
    vec![
        ("OS", host_os().to_string()),
        ("EXE_SUFFIX", if cfg!(target_os = "windows") { ".exe".into() } else { String::new() }),
        ("SHARED_SUFFIX", shared_suffix.to_string()),
        ("STATIC_SUFFIX", ".a".to_string()),
        ("OBJ_SUFFIX", ".o".to_string()),
        ("LIB_PREFIX", "lib".to_string()),
    ]
}

fn substitute(value: &mut serde_yaml::Value, unknown: &mut Vec<String>) {
    match value {
        serde_yaml::Value::String(text) => {
            if let Some(replaced) = apply_placeholders(text, unknown) {
                *text = replaced;
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                substitute(item, unknown);
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (_key, item) in map.iter_mut() {
                substitute(item, unknown);
            }
        }
        _ => {}
    }
}

fn apply_placeholders(text: &str, unknown: &mut Vec<String>) -> Option<String> {
    if !text.contains(PH_OPEN) {
        return None;
    }
    let mut out = text.to_string();
    for (key, value) in placeholder_table() {
        out = out.replace(&format!("{PH_OPEN}{key}}}"), &value);
    }
    let mut rest = out.as_str();
    while let Some(start) = rest.find(PH_OPEN) {
        if let Some(end) = rest[start..].find('}') {
            let name = &rest[start + PH_OPEN.len()..start + end];
            if !unknown.iter().any(|u| u == name) {
                unknown.push(name.to_string());
            }
            rest = &rest[start + end + 1..];
        } else {
            break;
        }
    }
    Some(out)
}

/// 读取 YAML 并反序列化；含占位符时先做平台替换。
pub fn parse_yaml_file<T: DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let text = crate::util::read_to_string(path)?;
    parse_yaml_str(&text).map_err(|e| e.context(format!("配置文件 {}", path.display())))
}

pub fn parse_yaml_str<T: DeserializeOwned>(text: &str) -> Result<T> {
    if text.contains(PH_OPEN) {
        let mut value: serde_yaml::Value =
            serde_yaml::from_str(text).map_err(|e| HkError::with_source("YAML 语法错误", e))?;
        let mut unknown = Vec::new();
        substitute(&mut value, &mut unknown);
        for name in unknown {
            crate::log_warn!(
                "配置中出现未知占位符 {}{name}}}，已原样保留",
                crate::config::PH_OPEN
            );
        }
        serde_yaml::from_value(value).map_err(|e| HkError::with_source("YAML 字段错误", e))
    } else {
        serde_yaml::from_str(text).map_err(|e| HkError::with_source("YAML 解析失败", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_replaced() {
        let text = format!("a: x{PH_OPEN}STATIC_SUFFIX}}");
        let parsed: serde_yaml::Value = parse_yaml_str(&text).unwrap();
        assert_eq!(parsed["a"].as_str().unwrap(), "x.a");
    }

    #[test]
    fn kind_parses_kebab_case() {
        let kind: Kind = serde_yaml::from_str("static-lib").unwrap();
        assert_eq!(kind, Kind::StaticLib);
    }

    #[test]
    fn naming_uses_platform_defaults() {
        let naming = Naming::platform_default();
        assert_eq!(naming.static_lib_name("hal"), "libhal.a");
    }

    #[test]
    fn builtin_global_config_parses() {
        let cfg: GlobalConfig = parse_yaml_str(DEFAULT_GLOBAL_YAML).unwrap();
        assert!(cfg.targets.contains_key("host"));
        assert!(cfg.profiles.contains_key("debug"));
        assert!(cfg.targets.values().all(|t| cfg.toolchains.contains_key(&t.toolchain)));
    }
}
