//! Project-level configuration (hk.yaml): toolchains, targets and build profiles.

use super::{Naming, PostAction};
use crate::error::{HkError, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn d_version() -> u32 {
    1
}
fn d_build_dir() -> String {
    "build".to_string()
}
fn d_default_target() -> String {
    "host".to_string()
}
fn d_default_mode() -> String {
    "debug".to_string()
}
fn d_true() -> bool {
    true
}

/// Project-level configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    #[serde(default = "d_version")]
    pub version: u32,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default = "d_build_dir")]
    pub build_dir: String,
    #[serde(default = "d_default_target")]
    pub default_target: String,
    #[serde(default = "d_default_mode")]
    pub default_mode: String,
    /// Parallel jobs; 0 means one per CPU core.
    #[serde(default)]
    pub jobs: usize,
    /// Temp directory handed to the compiler through TMPDIR/TMP/TEMP.
    /// Defaults to <module>/<build_dir>/tmp; a relative path is resolved against
    /// the project root.
    #[serde(default)]
    pub temp_dir: Option<String>,
    #[serde(default)]
    pub toolchains: IndexMap<String, Toolchain>,
    #[serde(default)]
    pub targets: IndexMap<String, Target>,
    #[serde(default)]
    pub profiles: IndexMap<String, Profile>,
}

/// A toolchain: a set of executables plus artifact naming conventions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchain {
    /// C compiler, e.g. arm-none-eabi-gcc.
    #[serde(default)]
    pub cc: Option<String>,
    /// C++ compiler, used when a module contains C++ sources; required in that case.
    #[serde(default)]
    pub cxx: Option<String>,
    /// Static library archiver.
    #[serde(default)]
    pub ar: Option<String>,
    #[serde(default)]
    pub objcopy: Option<String>,
    #[serde(default)]
    pub objdump: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub static_prefix: Option<String>,
    #[serde(default)]
    pub static_suffix: Option<String>,
    #[serde(default)]
    pub shared_prefix: Option<String>,
    #[serde(default)]
    pub shared_suffix: Option<String>,
    #[serde(default)]
    pub exe_suffix: Option<String>,
    #[serde(default)]
    pub obj_suffix: Option<String>,
}

impl Toolchain {
    pub fn cc(&self) -> Result<&str> {
        self.cc
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| HkError::new("toolchain has no cc configured (add it under toolchains in hk.yaml)"))
    }

    pub fn ar(&self) -> Result<&str> {
        self.ar
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| HkError::new("toolchain has no ar configured; static library modules need it (add it in hk.yaml)"))
    }

    pub fn cxx(&self) -> Result<&str> {
        match self.cxx.as_deref().filter(|s| !s.is_empty()) {
            Some(cxx) => Ok(cxx),
            None => Err(HkError::new(
                "toolchain has no cxx configured but the module contains C++ sources (add cxx under toolchains in hk.yaml)",
            )),
        }
    }

    pub fn objcopy(&self) -> Result<&str> {
        self.objcopy.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| {
            HkError::new("toolchain has no objcopy configured; producing bin/hex needs it (add it in hk.yaml)")
        })
    }

    pub fn objdump(&self) -> Result<&str> {
        self.objdump.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| {
            HkError::new("toolchain has no objdump configured; the disassembly listing needs it (add it in hk.yaml)")
        })
    }

    pub fn size(&self) -> Result<&str> {
        self.size
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| HkError::new("toolchain has no size configured; the size report needs it (add it in hk.yaml)"))
    }
}

/// A cross-compilation target.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub toolchain: String,
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub cxxflags: Vec<String>,
    #[serde(default)]
    pub ldflags: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    /// Extra system libraries for the link step (-lxxx).
    #[serde(default)]
    pub libs: Vec<String>,
    #[serde(default)]
    pub lib_dirs: Vec<String>,
    /// -specs= entries (nano.specs / nosys.specs / rdimon.specs ...).
    #[serde(default)]
    pub specs: Vec<String>,
    #[serde(default)]
    pub sysroot: Option<String>,
    /// Linker script (-T).
    #[serde(default)]
    pub linker_script: Option<String>,
    /// Entry symbol.
    #[serde(default)]
    pub entry: Option<String>,
    /// Artifacts produced after linking.
    #[serde(default)]
    pub post: Vec<PostAction>,
    /// Pass -Wl,--print-memory-usage and report FLASH/RAM usage (GCC 10+).
    #[serde(default)]
    pub memory_usage: bool,
    #[serde(default)]
    pub nostdlib: bool,
    #[serde(default)]
    pub nostartfiles: bool,
    #[serde(default)]
    pub strip: bool,
    /// Can the artifact run on the build host? False for MCU targets.
    #[serde(default = "d_true")]
    pub runnable: bool,
    #[serde(default)]
    pub static_suffix: Option<String>,
    #[serde(default)]
    pub shared_suffix: Option<String>,
    #[serde(default)]
    pub exe_suffix: Option<String>,
    #[serde(default)]
    pub jobs: Option<usize>,
}

impl Target {
    pub fn naming(&self, toolchain: &Toolchain) -> Naming {
        let base = Naming::platform_default();
        Naming {
            static_prefix: toolchain.static_prefix.clone().unwrap_or(base.static_prefix),
            static_suffix: self
                .static_suffix
                .clone()
                .or_else(|| toolchain.static_suffix.clone())
                .unwrap_or(base.static_suffix),
            shared_prefix: toolchain.shared_prefix.clone().unwrap_or(base.shared_prefix),
            shared_suffix: self
                .shared_suffix
                .clone()
                .or_else(|| toolchain.shared_suffix.clone())
                .unwrap_or(base.shared_suffix),
            exe_suffix: self
                .exe_suffix
                .clone()
                .or_else(|| toolchain.exe_suffix.clone())
                .unwrap_or(base.exe_suffix),
            obj_suffix: toolchain.obj_suffix.clone().unwrap_or(base.obj_suffix),
        }
    }
}

/// A build profile (selected with --mode).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub cxxflags: Vec<String>,
    #[serde(default)]
    pub ldflags: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub post: Vec<PostAction>,
    #[serde(default)]
    pub strip: bool,
}

impl GlobalConfig {
    /// The built-in default config (src/defaults/hk.yaml).
    pub fn builtin() -> Result<Self> {
        super::parse_yaml_str(super::DEFAULT_GLOBAL_YAML)
            .map_err(|e| e.context("failed to parse the built-in default config (this is a bug in hk)"))
    }

    /// Look for a project config file in this directory.
    pub fn find_file(dir: &Path) -> Option<PathBuf> {
        super::GLOBAL_FILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|p| p.is_file())
    }

    /// Walk upwards from `start` looking for the project root (a directory that
    /// contains hk.yaml or project.yaml).
    pub fn find_root(start: &Path) -> Option<PathBuf> {
        let mut current = Some(start.to_path_buf());
        while let Some(dir) = current {
            if Self::find_file(&dir).is_some() {
                return Some(dir);
            }
            current = dir.parent().map(|p| p.to_path_buf());
        }
        None
    }

    /// Built-in defaults overridden by the project file.
    pub fn load(root: Option<&Path>) -> Result<Self> {
        let base = Self::builtin()?;
        let cfg = match root {
            Some(dir) => match Self::find_file(dir) {
                Some(file) => {
                    let over: GlobalConfig = super::parse_yaml_file(&file)?;
                    base.merged_with(over)
                }
                None => base,
            },
            None => base,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Entries with the same name are replaced as a whole (simple and predictable),
    /// scalars come from the override. To tweak a built-in entry, run
    /// `hk config -o hk.yaml` first and edit the exported copy.
    pub fn merged_with(&self, over: GlobalConfig) -> GlobalConfig {
        let mut out = self.clone();
        out.version = over.version;
        out.project = over.project.or_else(|| out.project.clone());
        out.build_dir = over.build_dir;
        out.default_target = over.default_target;
        out.default_mode = over.default_mode;
        out.jobs = over.jobs;
        for (name, item) in over.toolchains {
            out.toolchains.insert(name, item);
        }
        for (name, item) in over.targets {
            out.targets.insert(name, item);
        }
        for (name, item) in over.profiles {
            out.profiles.insert(name, item);
        }
        out
    }

    pub fn validate(&self) -> Result<()> {
        if self.targets.is_empty() {
            crate::hk_bail!("no targets defined in hk.yaml");
        }
        if self.profiles.is_empty() {
            crate::hk_bail!("no profiles defined in hk.yaml");
        }
        if !self.targets.contains_key(&self.default_target) {
            crate::hk_bail!(
                "default target {:?} does not exist; available: {}",
                self.default_target,
                self.target_names()
            );
        }
        if !self.profiles.contains_key(&self.default_mode) {
            crate::hk_bail!(
                "default mode {:?} does not exist; available: {}",
                self.default_mode,
                self.profile_names()
            );
        }
        for (name, target) in &self.targets {
            if !self.toolchains.contains_key(&target.toolchain) {
                crate::hk_bail!(
                    "target {name} references unknown toolchain {:?}; available: {}",
                    target.toolchain,
                    self.toolchain_names()
                );
            }
        }
        for (name, toolchain) in &self.toolchains {
            if toolchain.cc.as_deref().unwrap_or("").is_empty() {
                crate::hk_bail!("toolchain {name} has no cc configured");
            }
        }
        Ok(())
    }

    pub fn target(&self, name: &str) -> Result<&Target> {
        self.targets.get(name).ok_or_else(|| {
            HkError::new(format!("unknown target {name:?}; available: {}", self.target_names()))
        })
    }

    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profiles.get(name).ok_or_else(|| {
            HkError::new(format!("unknown mode {name:?}; available: {}", self.profile_names())).context(
                "-m/--mode selects a build mode (a profile name in hk.yaml); \
                 use --module NAME to build a single module",
            )
        })
    }

    pub fn toolchain(&self, name: &str) -> Result<&Toolchain> {
        self.toolchains.get(name).ok_or_else(|| {
            HkError::new(format!("unknown toolchain {name:?}; available: {}", self.toolchain_names()))
        })
    }

    pub fn target_names(&self) -> String {
        self.targets.keys().cloned().collect::<Vec<_>>().join(", ")
    }

    pub fn profile_names(&self) -> String {
        self.profiles.keys().cloned().collect::<Vec<_>>().join(", ")
    }

    pub fn toolchain_names(&self) -> String {
        self.toolchains.keys().cloned().collect::<Vec<_>>().join(", ")
    }

    /// Serialize the config (used by hk init / hk config).
    pub fn dump(&self) -> Result<String> {
        let header = "# Exported by hk; edit it freely. Entries you remove fall back to the\n\
                      # built-in defaults. Layout: toolchains / targets / profiles.\n";
        let body = serde_yaml::to_string(self)
            .map_err(|e| HkError::with_source("failed to serialize the config", e))?;
        Ok(format!("{header}{body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> GlobalConfig {
        GlobalConfig::builtin().unwrap()
    }

    #[test]
    fn builtin_config_is_valid_and_has_mcu_targets() {
        let cfg = base();
        cfg.validate().unwrap();
        assert!(cfg.targets.contains_key("cortex-m4f"));
        assert!(!cfg.target("cortex-m4f").unwrap().runnable);
        assert!(cfg.profile("mcu-release").unwrap().post.contains(&PostAction::Hex));
        // commas separate items inside a YAML flow sequence, so such flags must be quoted
        assert_eq!(cfg.target("cortex-m4f").unwrap().ldflags, vec!["-Wl,--gc-sections".to_string()]);
        assert_eq!(cfg.target("cortex-m4f").unwrap().specs, vec!["nano.specs".to_string(), "nosys.specs".to_string()]);
    }

    #[test]
    fn project_file_overrides_by_name_and_keeps_others() {
        let base = base();
        let over: GlobalConfig = super::super::parse_yaml_str(
            "targets:\n  host:\n    toolchain: clang\n",
        )
        .unwrap();
        let merged = base.merged_with(over);
        assert_eq!(merged.target("host").unwrap().toolchain, "clang");
        assert!(merged.targets.contains_key("cortex-m4"));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let err = super::super::parse_yaml_str::<GlobalConfig>("targets: {}\nwhatever: 1\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("whatever"), "the error should name the unknown field: {err}");
    }

    #[test]
    fn naming_prefers_target_then_toolchain() {
        let cfg = base();
        let tc = cfg.toolchain("arm-none-eabi").unwrap();
        let target = cfg.target("cortex-m4").unwrap();
        let naming = target.naming(tc);
        assert_eq!(naming.executable_name("app"), "app.elf");
    }
}
