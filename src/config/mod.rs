//! Configuration model.
//!
//! * module.rs : module-level config.yaml (one per module)
//! * global.rs : project-level hk.yaml (toolchains / targets / profiles)
//!
//! Every compiler name and every compiler/linker flag comes from YAML; nothing of
//! that kind is hard-coded in Rust.

pub mod global;
pub mod module;

pub use global::{GlobalConfig, Profile, Target, Toolchain};
pub use module::{Config, ConfigView};

use crate::error::{HkError, Result};
use serde::de::DeserializeOwned;

/// Project config file names, tried in order.
pub const GLOBAL_FILE_NAMES: [&str; 2] = ["hk.yaml", "project.yaml"];
/// Module config file name.
pub const MODULE_FILE_NAME: &str = "config.yaml";
/// Built-in default project config.
pub const DEFAULT_GLOBAL_YAML: &str = include_str!("../defaults/hk.yaml");

/// Placeholder prefix; built with concat! to keep a literal dollar-brace out of the source.
pub const PH_OPEN: &str = concat!("$", "{");

/// Extra artifacts produced after linking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PostAction {
    /// Keep the link output as .elf
    Elf,
    /// objcopy -O binary
    Bin,
    /// objcopy -O ihex
    Hex,
    /// Linker map file
    Map,
    /// size report
    Size,
    /// objdump disassembly listing
    Lst,
}

/// Kind of artifact a module produces.
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

    /// Does this kind run a link step (as opposed to only objects, or nothing)?
    pub fn links(self) -> bool {
        matches!(self, Kind::Executable | Kind::SharedLib)
    }
}

/// Host platform name (usable as the OS placeholder in config files).
pub fn host_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Artifact naming, overridable per toolchain and per target; the fallback follows
/// the host platform.
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

/// Placeholder table so one YAML file works on Windows / Linux / macOS.
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

/// Read and deserialize YAML, expanding platform placeholders first.
pub fn parse_yaml_file<T: DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let text = crate::util::read_to_string(path)?;
    parse_yaml_str(&text).map_err(|e| e.context(format!("config file {}", path.display())))
}

pub fn parse_yaml_str<T: DeserializeOwned>(text: &str) -> Result<T> {
    if text.contains(PH_OPEN) {
        let mut value: serde_yaml::Value =
            serde_yaml::from_str(text).map_err(|e| HkError::with_source("YAML syntax error", e))?;
        let mut unknown = Vec::new();
        substitute(&mut value, &mut unknown);
        for name in unknown {
            crate::log_warn!(
                "unknown placeholder {}{name}}} in config; left as-is",
                crate::config::PH_OPEN
            );
        }
        serde_yaml::from_value(value).map_err(|e| HkError::with_source("invalid YAML fields", e))
    } else {
        serde_yaml::from_str(text).map_err(|e| HkError::with_source("YAML parse error", e))
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
