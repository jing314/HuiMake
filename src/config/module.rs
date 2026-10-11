//! 模块级配置：config.yaml。
//!
//! 支持 executable / static-lib / shared-lib / object / header-only 五类产物，
//! 全局宏（defines/undefines）、include 目录、多源 glob、按目标（for target）覆盖、
//! 系统库/静态库/动态库链接、链接脚本、rpath 等常见编译场景。

use super::Kind;
use crate::error::{HkError, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// 模块名，缺省取目录名。
    #[serde(default)]
    pub name: Option<String>,
    /// 产物类型。
    #[serde(default)]
    pub kind: Kind,
    /// C 标准，例如 c99 / c11 / gnu11。
    #[serde(default)]
    pub std: Option<String>,
    /// C++ 标准，例如 c++17。
    #[serde(default)]
    pub cxx_std: Option<String>,
    /// 源码 glob（相对模块目录）；缺省扫描 src/ 与 bin/。
    #[serde(default)]
    pub sources: Vec<String>,
    /// 排除的 glob。
    #[serde(default)]
    pub exclude: Vec<String>,
    /// 对外暴露的头文件目录（相对模块目录），缺省 include。
    #[serde(default)]
    pub include: Option<Vec<String>>,
    /// 仅本模块使用的额外头文件目录（相对模块目录）。
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// 全局宏声明，例如 VERSION="1.0"、DEBUG。
    #[serde(default)]
    pub defines: Vec<String>,
    /// 取消的宏。
    #[serde(default)]
    pub undefines: Vec<String>,
    /// 依赖。
    #[serde(default)]
    pub deps: Dependencies,
    /// 对外传播的编译接口（被依赖时生效）。
    #[serde(default)]
    pub public: Public,
    #[serde(default)]
    pub compile: CompileOpts,
    #[serde(default)]
    pub link: LinkOpts,
    /// 产物名，缺省取模块名。
    #[serde(default)]
    pub output: Option<String>,
    /// 针对特定目标的覆盖：targets: { arm-none-eabi: {...} }。
    #[serde(default)]
    pub targets: IndexMap<String, TargetOverride>,

    // ---- 0.1 兼容字段（会被自动折叠到新字段，并打印废弃告警）----
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
    /// 本工程内的模块依赖（同级目录名）。
    #[serde(default)]
    pub mods: Vec<String>,
    /// 系统库，渲染为 -lxxx。
    #[serde(default)]
    pub system: Vec<String>,
    /// 静态库：写文件名/路径则直接参与链接，写裸名则按 -l 处理。
    #[serde(default, rename = "static")]
    pub static_libs: Vec<String>,
    /// 动态库：同上。
    #[serde(default, rename = "shared")]
    pub shared_libs: Vec<String>,
    /// -L 搜索目录。
    #[serde(default)]
    pub lib_dirs: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Public {
    /// 对外暴露的头文件目录。
    #[serde(default)]
    pub include: Vec<String>,
    /// 对外暴露的额外头文件目录（本模块的 include_dirs 默认不传播）。
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// 对外传播的宏（例如 MCU 的器件宏 STM32F407xx）。
    #[serde(default)]
    pub defines: Vec<String>,
    /// 对外传播的编译参数（例如需要的架构宏）。
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
    /// 逐文件追加参数：src/slow.c: ["-O0"]。
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
    /// 以 --whole-archive 方式链接的库。
    #[serde(default)]
    pub whole_archive: Vec<String>,
    /// 直接参与链接的额外目标文件 / 归档。
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
    /// 裸库名是否用 -Wl,-Bstatic 包裹，强制静态链接。
    #[serde(default)]
    pub static_link: bool,
    /// 仅 shared-lib 追加的参数。
    #[serde(default)]
    pub shared_flags: Vec<String>,
}

/// 针对特定目标的模块覆盖。
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

/// 0.1 旧字段。
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

    /// 把 0.1 的旧字段折叠到新字段，并提示用户迁移。
    pub fn normalize(&mut self, source: &str) -> Result<()> {
        if !self.premacro.is_empty() {
            crate::log_warn!(
                "{source}: premacro 已改名 defines，两者都会生效，请尽快迁移"
            );
            let old = std::mem::take(&mut self.premacro);
            self.defines.extend(old);
        }
        if let Some(legacy) = self.dep.take() {
            crate::log_warn!(
                "{source}: dep.mod_deps / dep.lib / dep.include 已迁移到 deps.mods / deps.system / include_dirs"
            );
            self.deps.mods.extend(legacy.mod_deps);
            self.deps.system.extend(legacy.lib);
            let mut include_dirs = legacy.include;
            self.include_dirs.append(&mut include_dirs);
        }
        if self.compiler.is_some() {
            crate::log_warn!(
                "{source}: compiler 段已废弃，编译器与交叉编译参数统一在工程根 hk.yaml 的 toolchains/targets 中配置"
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
            .ok_or_else(|| HkError::new(format!("无法推断模块名: {}", dir.display())))
    }

    pub fn output_name(&self) -> Option<&str> {
        self.output.as_deref().filter(|s| !s.trim().is_empty())
    }

    /// 缺省源码扫描范围。
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

    /// 对外暴露的头文件目录。
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
                .map_err(|e| e.context(format!("{source}: sources 中的 {pattern:?} 不合法")))?;
        }
        for pattern in self.exclude.iter().chain(self.targets.values().flat_map(|t| t.exclude.iter())) {
            check_relative_glob(pattern)
                .map_err(|e| e.context(format!("{source}: exclude 中的 {pattern:?} 不合法")))?;
        }
        if let Some(std) = &self.std {
            if std.trim().is_empty() {
                crate::hk_bail!("{source}: std 不能为空");
            }
        }
        if self.kind == Kind::HeaderOnly && !self.sources.is_empty() {
            crate::log_warn!("{source}: header-only 模块会忽略 sources 配置");
        }
        Ok(())
    }

    /// 合并某个目标的覆盖，得到该目标下真正生效的配置视图。
    pub fn for_target(&self, target: &str) -> ConfigView<'_> {
        ConfigView { base: self, over: self.targets.get(target) }
    }
}

fn check_relative_glob(pattern: &str) -> Result<()> {
    let normalized = crate::glob::normalize_slashes(pattern);
    if normalized.starts_with('/') || normalized.contains("..") {
        crate::hk_bail!("必须使用模块目录内的相对 glob（不支持绝对路径或 ..）");
    }
    if normalized.is_empty() {
        crate::hk_bail!("glob 不能为空");
    }
    Ok(())
}

/// 基础配置 + 目标覆盖的只读视图。
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

    /// 仅目标覆盖里对外暴露的头文件目录。
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
        assert!(err.contains("kinds"), "应指出未知字段: {err}");
    }
}
