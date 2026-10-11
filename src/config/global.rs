//! 工程级配置：hk.yaml —— 工具链 / 目标（交叉编译）/ 配置档（构建模式）。

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

/// 工程级配置。
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
    /// 并行任务数，0 表示按 CPU 核数自动。
    #[serde(default)]
    pub jobs: usize,
    /// 编译器临时目录（TMPDIR/TMP/TEMP）。
    /// 缺省用 <模块>/<build_dir>/tmp；设为相对路径时相对工程根。
    #[serde(default)]
    pub temp_dir: Option<String>,
    #[serde(default)]
    pub toolchains: IndexMap<String, Toolchain>,
    #[serde(default)]
    pub targets: IndexMap<String, Target>,
    #[serde(default)]
    pub profiles: IndexMap<String, Profile>,
}

/// 工具链：一组可执行文件 + 产物命名约定。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchain {
    /// C 编译器，例如 arm-none-eabi-gcc。
    #[serde(default)]
    pub cc: Option<String>,
    /// C++ 编译器；模块含 C++ 源码时使用，缺省则由 cc 处理。
    #[serde(default)]
    pub cxx: Option<String>,
    /// 静态库打包器。
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
            .ok_or_else(|| HkError::new("工具链缺少 cc 配置：请在 hk.yaml 的 toolchains 中补充"))
    }

    pub fn ar(&self) -> Result<&str> {
        self.ar
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| HkError::new("工具链缺少 ar 配置：静态库模块需要它，请在 hk.yaml 中补充"))
    }

    pub fn cxx(&self) -> Result<&str> {
        match self.cxx.as_deref().filter(|s| !s.is_empty()) {
            Some(cxx) => Ok(cxx),
            None => Err(HkError::new(
                "工具链缺少 cxx 配置，但模块包含 C++ 源码：请在 hk.yaml 的 toolchains 中补充 cxx",
            )),
        }
    }

    pub fn objcopy(&self) -> Result<&str> {
        self.objcopy.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| {
            HkError::new("工具链缺少 objcopy 配置：生成 bin/hex 需要它，请在 hk.yaml 中补充")
        })
    }

    pub fn objdump(&self) -> Result<&str> {
        self.objdump.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| {
            HkError::new("工具链缺少 objdump 配置：生成反汇编清单需要它，请在 hk.yaml 中补充")
        })
    }

    pub fn size(&self) -> Result<&str> {
        self.size
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| HkError::new("工具链缺少 size 配置：size 报告需要它，请在 hk.yaml 中补充"))
    }
}

/// 交叉编译目标。
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
    /// 链接时附加的系统库（-lxxx）。
    #[serde(default)]
    pub libs: Vec<String>,
    #[serde(default)]
    pub lib_dirs: Vec<String>,
    /// -specs=xxx（nano.specs / nosys.specs / rdimon.specs ...）。
    #[serde(default)]
    pub specs: Vec<String>,
    #[serde(default)]
    pub sysroot: Option<String>,
    /// 链接脚本（-T）。
    #[serde(default)]
    pub linker_script: Option<String>,
    /// 入口符号。
    #[serde(default)]
    pub entry: Option<String>,
    /// 链接后产物。
    #[serde(default)]
    pub post: Vec<PostAction>,
    /// 传 -Wl,--print-memory-usage 并解析 FLASH/RAM 占用（GCC 10+）。
    #[serde(default)]
    pub memory_usage: bool,
    #[serde(default)]
    pub nostdlib: bool,
    #[serde(default)]
    pub nostartfiles: bool,
    #[serde(default)]
    pub strip: bool,
    /// 产物能否在构建机上直接运行（MCU 目标为 false）。
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

/// 构建配置档（--mode）。
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
    /// 内置默认配置（src/defaults/hk.yaml）。
    pub fn builtin() -> Result<Self> {
        super::parse_yaml_str(super::DEFAULT_GLOBAL_YAML)
            .map_err(|e| e.context("解析内置默认配置失败（这是工具自身的 bug）"))
    }

    /// 在目录下查找工程配置文件。
    pub fn find_file(dir: &Path) -> Option<PathBuf> {
        super::GLOBAL_FILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|p| p.is_file())
    }

    /// 从 start 向上查找工程根目录（含 hk.yaml / project.yaml）。
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

    /// 内置默认 + 工程文件覆盖。
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

    /// 同名条目整体替换（简单可预测）；标量以 override 为准。
    /// 想改内置条目，请先执行 hk config -o hk.yaml 导出一份完整配置再编辑。
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
            crate::hk_bail!("hk.yaml 中没有任何 targets 配置");
        }
        if self.profiles.is_empty() {
            crate::hk_bail!("hk.yaml 中没有任何 profiles 配置");
        }
        if !self.targets.contains_key(&self.default_target) {
            crate::hk_bail!(
                "默认目标 {:?} 不存在，可用目标: {}",
                self.default_target,
                self.target_names()
            );
        }
        if !self.profiles.contains_key(&self.default_mode) {
            crate::hk_bail!(
                "默认模式 {:?} 不存在，可用模式: {}",
                self.default_mode,
                self.profile_names()
            );
        }
        for (name, target) in &self.targets {
            if !self.toolchains.contains_key(&target.toolchain) {
                crate::hk_bail!(
                    "目标 {name} 引用了不存在的工具链 {:?}，可用工具链: {}",
                    target.toolchain,
                    self.toolchain_names()
                );
            }
        }
        for (name, toolchain) in &self.toolchains {
            if toolchain.cc.as_deref().unwrap_or("").is_empty() {
                crate::hk_bail!("工具链 {name} 缺少 cc 配置");
            }
        }
        Ok(())
    }

    pub fn target(&self, name: &str) -> Result<&Target> {
        self.targets.get(name).ok_or_else(|| {
            HkError::new(format!("未知目标 {name:?}，可用目标: {}", self.target_names()))
        })
    }

    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profiles.get(name).ok_or_else(|| {
            HkError::new(format!("未知模式 {name:?}，可用模式: {}", self.profile_names())).context(
                "-m/--mode 是构建模式（hk.yaml 中 profiles 的名字）；\
                 要构建指定模块请用 --module <模块名>",
            )
        })
    }

    pub fn toolchain(&self, name: &str) -> Result<&Toolchain> {
        self.toolchains.get(name).ok_or_else(|| {
            HkError::new(format!("未知工具链 {name:?}，可用工具链: {}", self.toolchain_names()))
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

    /// 导出配置（hk init / hk config 使用）。
    pub fn dump(&self) -> Result<String> {
        let header = "# 本文件由 hk 导出，可直接编辑；未列出的条目沿用内置默认值。\n\
                      # 详见 https://github.com/  (HuiMake) —— toolchains/targets/profiles 三层结构。\n";
        let body = serde_yaml::to_string(self)
            .map_err(|e| HkError::with_source("序列化配置失败", e))?;
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
        // flow 序列里的逗号会被 YAML 当成分隔符，这类参数必须加引号
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
        assert!(err.contains("whatever"), "错误信息应指出未知字段: {err}");
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
