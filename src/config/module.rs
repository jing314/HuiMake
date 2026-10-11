//! Module-level configuration (config.yaml).
//!
//! Supports five artifact kinds (executable / static-lib / shared-lib / object /
//! header-only), global macros, include directories, multi-source globs, per-target
//! overrides, static/shared/system library linking, linker scripts, rpath and more.

use super::Kind;
use crate::error::{HkError, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Module name; defaults to the directory name.
    #[serde(default)]
    pub name: Option<String>,
    /// Kind of artifact to produce.
    #[serde(default)]
    pub kind: Kind,
    /// C standard, e.g. c99 / c11 / gnu11.
    #[serde(default)]
    pub std: Option<String>,
    /// C++ standard, e.g. c++17.
    #[serde(default)]
    pub cxx_std: Option<String>,
    /// Source globs relative to the module directory; defaults to scanning src/ and bin/.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Globs to exclude.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Header directories exposed to dependents (relative to the module directory);
    /// defaults to include/.
    #[serde(default)]
    pub include: Option<Vec<String>>,
    /// Extra header directories used by this module only.
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// Global macro definitions, e.g. VERSION="1.0" or DEBUG.
    #[serde(default)]
    pub defines: Vec<String>,
    /// Macros to undefine.
    #[serde(default)]
    pub undefines: Vec<String>,
    /// Dependencies.
    #[serde(default)]
    pub deps: Dependencies,
    /// Public interface propagated to dependents.
    #[serde(default)]
    pub public: Public,
    #[serde(default)]
    pub compile: CompileOpts,
    #[serde(default)]
    pub link: LinkOpts,
    /// Artifact name; defaults to the module name.
    #[serde(default)]
    pub output: Option<String>,
    /// Per-target overrides, e.g. targets: { cortex-m4f: {...} }.
    #[serde(default)]
    pub targets: IndexMap<String, TargetOverride>,

    // ---- 0.1 compatibility fields (folded into the new ones, with a warning) ----
    #[serde(default)]
    pub premacro: Vec<String>,
    #[serde(default)]
    pub dep: Option<LegacyDep>,
    #[serde(default)]
    pub compiler: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dependencies {
    /// Modules of this project (sibling directories).
    #[serde(default)]
    pub mods: Vec<String>,
    /// System libraries, rendered as -lxxx.
    #[serde(default)]
    pub system: Vec<String>,
    /// Static libraries: a path is linked directly, a bare name becomes -l<name>.
    #[serde(default, rename = "static")]
    pub static_libs: Vec<String>,
    /// Shared libraries: same rules as above.
    #[serde(default, rename = "shared")]
    pub shared_libs: Vec<String>,
    /// -L search directories.
    #[serde(default)]
    pub lib_dirs: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Public {
    /// Header directories exposed to dependents.
    #[serde(default)]
    pub include: Vec<String>,
    /// Extra header directories exposed to dependents (plain include_dirs are private).
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// Macros propagated to dependents (e.g. the MCU part macro STM32F407xx).
    #[serde(default)]
    pub defines: Vec<String>,
    /// Compile flags propagated to dependents.
    #[serde(default)]
    pub cflags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileOpts {
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub cxx_flags: Vec<String>,
    /// Per-file extra flags, e.g. src/slow.c: ["-O0"].
    #[serde(default)]
    pub per_file: IndexMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkOpts {
    #[serde(default)]
    pub flags: Vec<String>,
    /// -Wl,-rpath,<dir>
    #[serde(default)]
    pub rpath: Vec<String>,
    /// Libraries linked with --whole-archive.
    #[serde(default)]
    pub whole_archive: Vec<String>,
    /// Extra object files / archives passed straight to the link step.
    #[serde(default)]
    pub extra_objects: Vec<String>,
    #[serde(default)]
    pub linker_script: Option<String>,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub nostdlib: bool,
    #[serde(default)]
    pub nostartfiles: bool,
    /// Wrap bare library names in -Wl,-Bstatic to force static linking.
    #[serde(default)]
    pub static_link: bool,
    /// Extra flags used only for shared libraries.
    #[serde(default)]
    pub shared_flags: Vec<String>,
}

/// Per-target module overrides.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetOverride {
    #[serde(default)]
    pub cflags: Vec<String>,
    #[serde(default)]
    pub cxxflags: Vec<String>,
    #[serde(default)]
    pub ldflags: Vec<String>,
    #[serde(default)]
    pub defines: Vec<String>,
    #[serde(default)]
    pub undefines: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub include_dirs: Vec<String>,
    #[serde(default)]
    pub compile: CompileOpts,
    #[serde(default)]
    pub link: LinkOpts,
    #[serde(default)]
    pub deps: Dependencies,
}

/// 0.1 compatibility fields.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyDep {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub mod_deps: Vec<String>,
    #[serde(default)]
    pub lib: Vec<String>,
}

impl Config {
    pub fn load_file(path: &Path) -> Result<Config> {
        let mut cfg: Config = super::parse_yaml_file(path)?;
        cfg.normalize(&path.display().to_string())?;
        Ok(cfg)
    }

    /// Fold the 0.1 fields into the new ones and tell the user to migrate.
    pub fn normalize(&mut self, source: &str) -> Result<()> {
        if !self.premacro.is_empty() {
            crate::log_warn!(
                "{source}: premacro was renamed to defines; both are honoured, please migrate"
            );
            let old = std::mem::take(&mut self.premacro);
            self.defines.extend(old);
        }
        if let Some(legacy) = self.dep.take() {
            crate::log_warn!(
                "{source}: dep.mod_deps / dep.lib / dep.include moved to deps.mods / deps.system / include_dirs"
            );
            self.deps.mods.extend(legacy.mod_deps);
            self.deps.system.extend(legacy.lib);
            let mut include_dirs = legacy.include;
            self.include_dirs.append(&mut include_dirs);
        }
        if self.compiler.is_some() {
            crate::log_warn!(
                "{source}: the compiler section is obsolete; compiler and cross-compilation settings\n                 belong to toolchains/targets in the project hk.yaml"
            );
            self.compiler = None;
        }
        Ok(())
    }

    pub fn effective_name(&self, dir: &Path) -> Result<String> {
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                return Ok(name.trim().to_string());
            }
        }
        dir.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_string())
            .ok_or_else(|| HkError::new(format!("cannot infer a module name from {}", dir.display())))
    }

    pub fn output_name(&self) -> Option<&str> {
        self.output.as_deref().filter(|s| !s.trim().is_empty())
    }

    /// Default source scan.
    pub fn default_sources(&self) -> Vec<String> {
        if self.sources.is_empty() {
            vec![
                "src/**/*.c".into(),
                "src/**/*.cc".into(),
                "src/**/*.cpp".into(),
                "src/**/*.cxx".into(),
                "src/**/*.S".into(),
                "src/**/*.s".into(),
                "bin/**/*.c".into(),
                "bin/**/*.cc".into(),
                "bin/**/*.cpp".into(),
            ]
        } else {
            self.sources.clone()
        }
    }

    /// Header directories exposed to dependents.
    pub fn public_includes(&self) -> Vec<String> {
        let mut dirs = match &self.include {
            Some(list) => list.clone(),
            None => vec!["include".to_string()],
        };
        if dirs.is_empty() {
            dirs.push("include".to_string());
        }
        dirs.extend(self.public.include.iter().cloned());
        dirs
    }

    pub fn validate(&self, source: &str) -> Result<()> {
        for pattern in self.sources.iter().chain(self.targets.values().flat_map(|t| t.sources.iter())) {
            check_relative_glob(pattern)
                .map_err(|e| e.context(format!("{source}: invalid sources pattern {pattern:?}")))?;
        }
        for pattern in self.exclude.iter().chain(self.targets.values().flat_map(|t| t.exclude.iter())) {
            check_relative_glob(pattern)
                .map_err(|e| e.context(format!("{source}: invalid exclude pattern {pattern:?}")))?;
        }
        if let Some(std) = &self.std {
            if std.trim().is_empty() {
                crate::hk_bail!("{source}: std must not be empty");
            }
        }
        if self.kind == Kind::HeaderOnly && !self.sources.is_empty() {
            crate::log_warn!("{source}: header-only modules ignore the sources setting");
        }
        Ok(())
    }

    /// Merge a target override to get the configuration that is effective for it.
    pub fn for_target(&self, target: &str) -> ConfigView<'_> {
        ConfigView { base: self, over: self.targets.get(target) }
    }
}

fn check_relative_glob(pattern: &str) -> Result<()> {
    let normalized = crate::glob::normalize_slashes(pattern);
    if normalized.starts_with('/') || normalized.contains("..") {
        crate::hk_bail!("globs must be relative to the module directory (absolute paths and .. are not supported)");
    }
    if normalized.is_empty() {
        crate::hk_bail!("glob must not be empty");
    }
    Ok(())
}

/// Read-only view of the base config plus a target override.
pub struct ConfigView<'a> {
    pub base: &'a Config,
    pub over: Option<&'a TargetOverride>,
}

impl<'a> ConfigView<'a> {
    pub fn defines(&self) -> Vec<String> {
        let mut out = self.base.defines.clone();
        if let Some(over) = self.over {
            out.extend(over.defines.iter().cloned());
        }
        out
    }

    pub fn undefines(&self) -> Vec<String> {
        let mut out = self.base.undefines.clone();
        if let Some(over) = self.over {
            out.extend(over.undefines.iter().cloned());
        }
        out
    }

    pub fn cflags(&self) -> Vec<String> {
        let mut out = self.base.compile.flags.clone();
        if let Some(over) = self.over {
            out.extend(over.cflags.iter().cloned());
            out.extend(over.compile.flags.iter().cloned());
        }
        out
    }

    pub fn cxxflags(&self) -> Vec<String> {
        let mut out = self.base.compile.cxx_flags.clone();
        if let Some(over) = self.over {
            out.extend(over.cxxflags.iter().cloned());
            out.extend(over.compile.cxx_flags.iter().cloned());
        }
        out
    }

    pub fn per_file(&self) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = self
            .base
            .compile
            .per_file
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(over) = self.over {
            out.extend(over.compile.per_file.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        out
    }

    pub fn sources(&self) -> Vec<String> {
        let mut out = self.base.default_sources();
        if let Some(over) = self.over {
            out.extend(over.sources.iter().cloned());
        }
        out
    }

    pub fn exclude(&self) -> Vec<String> {
        let mut out = self.base.exclude.clone();
        if let Some(over) = self.over {
            out.extend(over.exclude.iter().cloned());
        }
        out
    }

    pub fn include_dirs(&self) -> Vec<String> {
        let mut out = self.base.include_dirs.clone();
        if let Some(over) = self.over {
            out.extend(over.include.iter().cloned());
            out.extend(over.include_dirs.iter().cloned());
        }
        out
    }

    pub fn link(&self) -> LinkOpts {
        let mut out = self.base.link.clone();
        if let Some(over) = self.over {
            out.flags.extend(over.link.flags.iter().cloned());
            out.rpath.extend(over.link.rpath.iter().cloned());
            out.whole_archive.extend(over.link.whole_archive.iter().cloned());
            out.extra_objects.extend(over.link.extra_objects.iter().cloned());
            out.shared_flags.extend(over.link.shared_flags.iter().cloned());
            out.static_link |= over.link.static_link;
            out.nostdlib |= over.link.nostdlib;
            out.nostartfiles |= over.link.nostartfiles;
            if over.link.linker_script.is_some() {
                out.linker_script = over.link.linker_script.clone();
            }
            if over.link.entry.is_some() {
                out.entry = over.link.entry.clone();
            }
            out.flags.extend(over.ldflags.iter().cloned());
        }
        out
    }

    /// Header directories exposed by the target override.
    pub fn public_target_includes(&self) -> Vec<String> {
        self.over.map(|over| over.include.clone()).unwrap_or_default()
    }

    pub fn deps(&self) -> Dependencies {
        let mut out = self.base.deps.clone();
        if let Some(over) = self.over {
            out.mods.extend(over.deps.mods.iter().cloned());
            out.system.extend(over.deps.system.iter().cloned());
            out.static_libs.extend(over.deps.static_libs.iter().cloned());
            out.shared_libs.extend(over.deps.shared_libs.iter().cloned());
            out.lib_dirs.extend(over.deps.lib_dirs.iter().cloned());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name: app
kind: executable
std: c11
cxx_std: c++17
sources:
  - src/**/*.c
  - app/**.cpp
exclude:
  - src/legacy/**
include:
  - include
  - api
include_dirs:
  - third_party/foo/include
defines:
  - VERSION="1.0.0"
  - ENABLE_LOG
undefines:
  - NDEBUG
deps:
  mods: [hal, drivers]
  system: [pthread, m]
  static: [libfoo.a, bar]
  shared: [libz.so, z]
  lib_dirs: [/opt/foo/lib]
public:
  include: [public_api]
  defines: [STM32F407xx]
compile:
  flags: [-Wall]
  per_file:
    src/slow.c: [-O0]
link:
  flags: ["-Wl,--export-dynamic"]
  rpath: [/opt/foo/lib]
  whole_archive: [libplugin.a]
  extra_objects: [prebuilt/entry.o]
  linker_script: link/app.ld
  entry: _start
  static_link: true
targets:
  cortex-m4:
    defines: [USE_HAL_DRIVER]
    sources: [startup/**/*.S]
    cflags: [-mcpu=cortex-m4]
    link:
      linker_script: link/mcu.ld
"#;

    #[test]
    fn parses_rich_module_config() {
        let cfg: Config = crate::config::parse_yaml_str(SAMPLE).unwrap();
        cfg.validate("test").unwrap();
        assert_eq!(cfg.kind, Kind::Executable);
        assert_eq!(cfg.defines.len(), 2);
        assert_eq!(cfg.deps.static_libs, vec!["libfoo.a", "bar"]);
        assert_eq!(cfg.deps.shared_libs, vec!["libz.so", "z"]);
        assert_eq!(cfg.compile.per_file["src/slow.c"], vec!["-O0"]);
        assert_eq!(cfg.link.rpath, vec!["/opt/foo/lib"]);
        assert!(cfg.targets.contains_key("cortex-m4"));

        let view = cfg.for_target("cortex-m4");
        assert!(view.defines().contains(&"USE_HAL_DRIVER".to_string()));
        assert!(view.sources().iter().any(|s| s == "startup/**/*.S"));
        assert_eq!(view.link().linker_script.as_deref(), Some("link/mcu.ld"));
    }

    #[test]
    fn default_view_keeps_base_values() {
        let cfg: Config = crate::config::parse_yaml_str(SAMPLE).unwrap();
        let view = cfg.for_target("host");
        assert_eq!(view.link().linker_script.as_deref(), Some("link/app.ld"));
        assert!(!view.defines().contains(&"USE_HAL_DRIVER".to_string()));
    }

    #[test]
    fn legacy_config_still_loads_and_is_folded() {
        let text = r#"
name: old
std: c99
premacro: [OLD_MACRO]
dep:
  include: [inc]
  mod_deps: [depmod]
  lib: [pthread]
compiler:
  cc: gcc
"#;
        let mut cfg: Config = crate::config::parse_yaml_str(text).unwrap();
        cfg.normalize("legacy").unwrap();
        assert!(cfg.defines.contains(&"OLD_MACRO".to_string()));
        assert_eq!(cfg.deps.mods, vec!["depmod"]);
        assert_eq!(cfg.deps.system, vec!["pthread"]);
        assert_eq!(cfg.include_dirs, vec!["inc"]);
        assert!(cfg.compiler.is_none());
    }

    #[test]
    fn empty_config_is_executable_with_defaults() {
        let cfg: Config = crate::config::parse_yaml_str("{}").unwrap();
        assert_eq!(cfg.kind, Kind::Executable);
        assert_eq!(cfg.public_includes(), vec!["include"]);
        assert!(cfg.default_sources().iter().any(|s| s == "src/**/*.c"));
    }

    #[test]
    fn sources_may_not_escape_module_dir() {
        let cfg: Config = crate::config::parse_yaml_str("sources: [../outside/**/*.c]").unwrap();
        assert!(cfg.validate("t").is_err());
    }

    #[test]
    fn unknown_field_is_reported() {
        let err = crate::config::parse_yaml_str::<Config>("kind: executable\nkinds: 1\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("kinds"), "should name the unknown field: {err}");
    }
}
