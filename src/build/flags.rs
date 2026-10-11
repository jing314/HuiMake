//! 编译/链接参数组装：工具链 × 目标 × 配置档 × 模块配置，全部来自 YAML。
//!
//! 这里是纯函数，便于单元测试（尤其是交叉编译/MCU 场景不再需要真实工具链即可断言）。

use crate::config::{ConfigView, Kind, Profile, Target};
use std::path::{Path, PathBuf};

pub struct FlagInput<'a> {
    pub view: ConfigView<'a>,
    pub target: &'a Target,
    pub profile: &'a Profile,
    pub kind: Kind,
    /// 本模块 + 依赖传播过来的头文件搜索目录（已解析为绝对路径）。
    pub include_dirs: &'a [PathBuf],
    /// -L 搜索目录（已解析为绝对路径）。
    pub lib_dirs: Vec<String>,
    /// 链接脚本（已解析为绝对路径）。
    pub linker_script: Option<String>,
    /// 依赖通过 public.defines 传播过来的宏。
    pub public_defines: &'a [String],
    /// 依赖通过 public.cflags 传播过来的编译参数。
    pub public_cflags: &'a [String],
    pub map_path: Option<&'a Path>,
    pub strip: bool,
}

fn defines(view: &ConfigView<'_>, target: &Target, profile: &Profile, public: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    out.extend(view.defines());
    out.extend(target.defines.iter().cloned());
    out.extend(profile.defines.iter().cloned());
    out.extend(public.iter().cloned());
    // 目标级宏与模块 public.defines 常常重复，去重后输出更干净
    let mut seen = std::collections::HashSet::new();
    out.retain(|item| seen.insert(item.clone()));
    out
}

fn macro_args(defines: &[String], undefines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for def in defines {
        out.push(format!("-D{def}"));
    }
    for undef in undefines {
        out.push(format!("-U{undef}"));
    }
    out
}

fn include_args(dirs: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for dir in dirs {
        out.push("-I".to_string());
        out.push(dir.display().to_string());
    }
    out
}

/// 目标级架构参数（以 -m 开头，例如 -mcpu / -mthumb / -mfloat-abi / -mfpu）。
/// 这类参数同时用于链接，保证 ld 选择正确的架构与运行库。
fn arch_flags(target: &Target) -> Vec<String> {
    target
        .cflags
        .iter()
        .filter(|flag| flag.starts_with("-m"))
        .cloned()
        .collect()
}

/// specs 既支持裸名（nano.specs），也支持已经带 -specs= 前缀的写法。
fn spec_arg(spec: &str) -> String {
    if spec.starts_with("-specs=") || spec.starts_with("--specs=") {
        spec.to_string()
    } else {
        format!("-specs={spec}")
    }
}

fn target_machine_args(input: &FlagInput<'_>, cxx: bool) -> Vec<String> {
    let mut out = Vec::new();
    if cxx {
        out.extend(input.target.cxxflags.iter().cloned());
        out.extend(input.profile.cxxflags.iter().cloned());
        out.extend(input.view.cxxflags());
    } else {
        out.extend(input.target.cflags.iter().cloned());
        out.extend(input.profile.cflags.iter().cloned());
        out.extend(input.view.cflags());
    }
    out.extend(input.public_cflags.iter().cloned());
    for spec in &input.target.specs {
        out.push(spec_arg(spec));
    }
    if let Some(sysroot) = &input.target.sysroot {
        out.push(format!("--sysroot={sysroot}"));
    }
    out
}

/// C 与 C++ 各自的编译参数（不含 -c/-o/源文件）。
pub fn compile(input: &FlagInput<'_>) -> (Vec<String>, Vec<String>) {
    let defines = defines(&input.view, input.target, input.profile, input.public_defines);
    let undefines = input.view.undefines();
    let macros = macro_args(&defines, &undefines);
    let includes = include_args(input.include_dirs);

    let mut c = Vec::new();
    if let Some(std) = input.view.base.std.as_deref() {
        c.push(format!("-std={std}"));
    }
    c.extend(target_machine_args(input, false));
    c.extend(macros.iter().cloned());
    c.extend(includes.iter().cloned());

    let mut cxx = Vec::new();
    if let Some(std) = input.view.base.cxx_std.as_deref() {
        cxx.push(format!("-std={std}"));
    }
    cxx.extend(target_machine_args(input, true));
    cxx.extend(macros);
    cxx.extend(includes);

    (c, cxx)
}

/// 链接参数中位于目标文件之前的部分。
pub fn link_pre(input: &FlagInput<'_>) -> Vec<String> {
    let link = input.view.link();
    let mut out = Vec::new();
    if input.kind == Kind::SharedLib {
        out.push("-shared".to_string());
        out.extend(link.shared_flags.iter().cloned());
    }
    out.extend(input.profile.ldflags.iter().cloned());
    out.extend(input.target.ldflags.iter().cloned());
    out.extend(arch_flags(input.target));
    out.extend(link.flags.iter().cloned());

    for dir in &input.lib_dirs {
        out.push(format!("-L{dir}"));
    }
    for dir in &link.rpath {
        out.push(format!("-Wl,-rpath,{dir}"));
    }
    if let Some(map) = input.map_path {
        out.push(format!("-Wl,-Map,{}", map.display()));
    }
    if input.target.memory_usage {
        out.push("-Wl,--print-memory-usage".to_string());
    }
    if let Some(script) = &input.linker_script {
        out.push("-T".to_string());
        out.push(script.clone());
    }
    let entry = link.entry.clone().or_else(|| input.target.entry.clone());
    if let Some(entry) = entry {
        out.push(format!("-Wl,-e,{entry}"));
    }
    if input.target.nostdlib || link.nostdlib {
        out.push("-nostdlib".to_string());
    }
    if input.target.nostartfiles || link.nostartfiles {
        out.push("-nostartfiles".to_string());
    }
    for spec in &input.target.specs {
        out.push(spec_arg(spec));
    }
    if let Some(sysroot) = &input.target.sysroot {
        out.push(format!("--sysroot={sysroot}"));
    }
    out
}

/// 链接参数中位于库之后的部分。
pub fn link_post(input: &FlagInput<'_>, output: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for lib in &input.target.libs {
        out.push(format!("-l{lib}"));
    }
    out.push("-o".to_string());
    out.push(output.display().to_string());
    if input.strip {
        out.push("-s".to_string());
    }
    out
}

/// 编译单个文件时追加的逐文件参数。
pub fn per_file_flags(view: &ConfigView<'_>, rel_path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (pattern, flags) in view.per_file() {
        if pattern == rel_path || crate::glob::matches(&pattern, rel_path) {
            out.extend(flags);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GlobalConfig};

    fn project_config(yaml: &str) -> Config {
        crate::config::parse_yaml_str(yaml).unwrap()
    }

    fn global() -> GlobalConfig {
        GlobalConfig::builtin().unwrap()
    }

    #[test]
    fn host_debug_flags_are_composed_from_yaml() {
        let global = global();
        let cfg = project_config("name: app\nstd: c11\ndefines: [FOO=1]\ninclude_dirs: []\n");
        let target = global.target("host").unwrap();
        let profile = global.profile("debug").unwrap();
        let view = cfg.for_target("host");
        let input = FlagInput {
            view,
            target,
            profile,
            kind: Kind::Executable,
            include_dirs: &[],
            lib_dirs: Vec::new(),
            linker_script: None,
            public_defines: &[],
            public_cflags: &[],
            map_path: None,
            strip: false,
        };
        let (c, cxx) = compile(&input);
        assert_eq!(c[0], "-std=c11");
        assert!(c.contains(&"-Og".to_string()), "debug profile 的 -Og 应来自 YAML");
        assert!(c.contains(&"-DDEBUG".to_string()));
        assert!(c.contains(&"-DFOO=1".to_string()));
        // C++ 未配置 cxx_std 时不应注入 -std=
        assert!(!cxx.iter().any(|a| a.starts_with("-std=")));
    }

    #[test]
    fn mcu_target_flags_match_expected_cross_toolchain_invocation() {
        let global = global();
        let cfg = project_config("name: fw\nstd: c11\n");
        let target = global.target("cortex-m4f").unwrap();
        let profile = global.profile("mcu-release").unwrap();
        let view = cfg.for_target("cortex-m4f");
        let input = FlagInput {
            view,
            target,
            profile,
            kind: Kind::Executable,
            include_dirs: &[],
            lib_dirs: Vec::new(),
            linker_script: None,
            public_defines: &["STM32F407xx".to_string()],
            public_cflags: &[],
            map_path: None,
            strip: true,
        };
        let (c, _) = compile(&input);
        for expected in [
            "-mcpu=cortex-m4",
            "-mthumb",
            "-mfloat-abi=hard",
            "-mfpu=fpv4-sp-d16",
            "-Os",
            "-ffunction-sections",
            "-fdata-sections",
            "-specs=nano.specs",
            "-specs=nosys.specs",
            "-DSTM32F407xx",
            "-DNDEBUG",
        ] {
            assert!(c.contains(&expected.to_string()), "缺少 {expected}: {c:?}");
        }

        let map = std::path::PathBuf::from("build/bin/fw.map");
        let out = std::path::PathBuf::from("build/bin/fw.elf");
        let link_input = FlagInput {
            view: cfg.for_target("cortex-m4f"),
            target,
            profile,
            kind: Kind::Executable,
            include_dirs: &[],
            lib_dirs: Vec::new(),
            linker_script: None,
            public_defines: &[],
            public_cflags: &[],
            map_path: Some(&map),
            strip: true,
        };
        let pre = link_pre(&link_input);
        assert!(pre.contains(&"-Wl,--gc-sections".to_string()));
        assert!(pre.contains(&"-Wl,--print-memory-usage".to_string()));
        assert!(pre.contains(&format!("-Wl,-Map,{}", map.display())));
        assert!(pre.contains(&"-specs=nano.specs".to_string()));
        // 链接脚本来自模块配置（此处为 None）时不应凭空添加 -T
        assert!(!pre.contains(&"-T".to_string()));
        let post = link_post(&link_input, &out);
        assert!(post.contains(&"-o".to_string()));
        assert!(post.contains(&out.display().to_string()));
        assert!(post.contains(&"-s".to_string()));
    }

    #[test]
    fn shared_lib_link_adds_shared_flag() {
        let global = global();
        let cfg = project_config("name: z\nkind: shared-lib\n");
        let input = FlagInput {
            view: cfg.for_target("host"),
            target: global.target("host").unwrap(),
            profile: global.profile("release").unwrap(),
            kind: Kind::SharedLib,
            include_dirs: &[],
            lib_dirs: Vec::new(),
            linker_script: None,
            public_defines: &[],
            public_cflags: &[],
            map_path: None,
            strip: false,
        };
        assert!(link_pre(&input).contains(&"-shared".to_string()));
    }

    #[test]
    fn per_file_flags_match_glob_and_exact() {
        let cfg = project_config(
            "compile:\n  per_file:\n    src/slow.c: [-O0]\n    vendor/**/*.c: [-w]\n",
        );
        assert_eq!(per_file_flags(&cfg.for_target("host"), "src/slow.c"), vec!["-O0"]);
        assert_eq!(per_file_flags(&cfg.for_target("host"), "vendor/a/b.c"), vec!["-w"]);
        assert!(per_file_flags(&cfg.for_target("host"), "src/fast.c").is_empty());
    }
}
