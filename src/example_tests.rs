//! 针对 examples/ 下示范工程的单元测试。
//!
//! 纯 Rust、无需 gcc 或交叉工具链：直接把示范工程当作输入，
//! 解析配置 → 发现模块 → 依赖拓扑 → 对「每个目标 × 每个配置档」生成构建计划，
//! 并断言关键编译/链接/后处理参数。
//!
//! 覆盖：单模块、多模块（静态库/动态库/可执行）、MCU（STM32F407、GD32F303）。

use crate::build::steps::{self, ModulePlan, PlanCtx, Step, StepKind};
use crate::config::{Config, GlobalConfig, MODULE_FILE_NAME};
use crate::project::Project;
use std::fs;
use std::path::{Path, PathBuf};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// 含 hk.yaml 的示范工程（多模块 / MCU）。
fn project_examples() -> Vec<PathBuf> {
    let root = examples_dir();
    let mut out = Vec::new();
    for entry in fs::read_dir(&root).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", root.display())) {
        let path = entry.expect("目录项").path();
        if path.is_dir() && path.join("hk.yaml").is_file() {
            out.push(path);
        }
    }
    out.sort();
    assert!(out.len() >= 3, "至少应有 3 个含 hk.yaml 的示范工程: {out:?}");
    out
}

/// 全部示范工程根目录（单模块示例没有 hk.yaml，单独补上）。
fn all_example_roots() -> Vec<PathBuf> {
    let mut out = vec![examples_dir().join("01-single-module")];
    out.extend(project_examples());
    out
}

fn collect_config_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", dir.display())) {
        let path = entry.expect("目录项").path();
        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name == "build" || name.starts_with('.') {
                continue;
            }
            collect_config_files(&path, out);
        } else if path.file_name().map(|n| n == MODULE_FILE_NAME).unwrap_or(false) {
            out.push(path);
        }
    }
}

fn plan_for<'a>(
    project: &'a Project,
    target_name: &'a str,
    profile_name: &'a str,
    module: &str,
    force: bool,
) -> ModulePlan {
    let target = project.global.target(target_name).expect("目标存在");
    let profile = project.global.profile(profile_name).expect("配置档存在");
    let toolchain = project.global.toolchain(&target.toolchain).expect("工具链存在");
    let naming = target.naming(toolchain);
    let ctx = PlanCtx {
        project,
        project_root: &project.root,
        target_name,
        target,
        profile,
        toolchain,
        naming: &naming,
        build_dir: &project.global.build_dir,
        force,
        strip: profile.strip || target.strip,
    };
    steps::plan_module(&ctx, module)
        .unwrap_or_else(|e| panic!("规划 {}/{target_name}/{profile_name}/{module} 失败: {e}", project.root.display()))
}

fn args_of(step: &Step) -> Vec<String> {
    step.cmd.args.iter().map(|a| a.to_string_lossy().to_string()).collect()
}

fn joined(step: &Step) -> String {
    format!("{} {}", step.cmd.program, args_of(step).join(" "))
}

fn step_in(plan: &ModulePlan, phase: usize) -> &Step {
    plan.phases[phase]
        .first()
        .unwrap_or_else(|| panic!("模块 {} 的第 {phase} 阶段没有步骤", plan.module))
}

fn contains_all(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "缺少 {needle:?}:\n{text}");
    }
}

// ---------------------------------------------------------------------------
// 配置与工程结构
// ---------------------------------------------------------------------------

#[test]
fn every_example_global_config_is_valid() {
    for dir in project_examples() {
        let cfg = GlobalConfig::load(Some(&dir))
            .unwrap_or_else(|e| panic!("加载 {} 的 hk.yaml 失败: {e}", dir.display()));
        assert!(!cfg.targets.is_empty(), "{} 没有目标", dir.display());
        assert!(!cfg.profiles.is_empty(), "{} 没有配置档", dir.display());
        assert!(cfg.targets.contains_key(&cfg.default_target));
        assert!(cfg.profiles.contains_key(&cfg.default_mode));
    }
}

#[test]
fn every_example_module_config_parses() {
    let mut count = 0;
    for root in all_example_roots() {
        let mut files = Vec::new();
        collect_config_files(&root, &mut files);
        assert!(!files.is_empty(), "{} 下没有找到 config.yaml", root.display());
        for file in files {
            let text = file.display().to_string();
            Config::load_file(&file).unwrap_or_else(|e| panic!("解析 {text} 失败: {e}"));
            count += 1;
        }
    }
    assert!(count >= 11, "示范工程的模块配置应覆盖到多模块与 MCU 场景，实际 {count} 个");
}

#[test]
fn every_example_project_resolves_dependencies_and_topology() {
    for root in all_example_roots() {
        let project = Project::discover(&root)
            .unwrap_or_else(|e| panic!("发现 {} 失败: {e}", root.display()));
        assert!(!project.modules.is_empty(), "{} 没有模块", root.display());
        let levels = project.topo_levels()
            .unwrap_or_else(|e| panic!("{} 依赖拓扑失败: {e}", root.display()));
        let total: usize = levels.iter().map(|l| l.len()).sum();
        assert_eq!(total, project.modules.len());
    }
}

#[test]
fn every_example_module_plans_for_every_target_and_profile() {
    let mut combos = 0;
    for root in all_example_roots() {
        let project = Project::discover(&root).unwrap();
        let targets: Vec<String> = project.global.targets.keys().cloned().collect();
        let profiles: Vec<String> = project.global.profiles.keys().cloned().collect();
        let modules: Vec<String> = project.modules.keys().cloned().collect();
        for target in &targets {
            for profile in &profiles {
                for module in &modules {
                    let plan = plan_for(&project, target, profile, module, true);
                    combos += 1;
                    // header-only 之外的模块都应产出可执行/归档计划或明确无需重建
                    if module != "unused" {
                        assert!(
                            !plan.artifacts.is_empty() || plan.kind == crate::config::Kind::HeaderOnly,
                            "{module} 在 {target}/{profile} 下没有产物"
                        );
                    }
                }
            }
        }
    }
    assert!(combos >= 30, "目标×配置档×模块的组合过少: {combos}");
}

// ---------------------------------------------------------------------------
// 01 单模块
// ---------------------------------------------------------------------------

#[test]
fn examples_root_is_a_container_not_a_project() {
    // 直接在上层目录运行 hk 时，只应看到不隶属于任何子工程的模块，
    // 不能把 02/03/04 三个子工程的模块混成一个工程（否则 bsp/firmware 会撞名）。
    let project = Project::discover(&examples_dir()).unwrap();
    assert_eq!(
        project.modules.keys().cloned().collect::<Vec<_>>(),
        vec!["hello".to_string()]
    );
}

#[test]
fn single_module_example_uses_default_sources_and_includes() {
    let root = examples_dir().join("01-single-module");
    let project = Project::discover(&root).unwrap();
    assert_eq!(project.modules.len(), 1, "单模块示例只应有一个模块");

    let plan = plan_for(&project, "host", "debug", "hello", true);
    assert_eq!(plan.kind, crate::config::Kind::Executable);
    assert_eq!(plan.phases[0].len(), 2, "src/greet.c 与 bin/main.c 都应参与编译");
    let sources = plan.phases[0]
        .iter()
        .map(|s| {
            let args = args_of(s);
            let index = args.iter().position(|a| a == "-c").expect("编译命令应含 -c");
            args[index + 1].clone()
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(sources.contains("greet.c"), "{sources}");
    assert!(sources.contains("main.c"), "{sources}");

    let compile = joined(step_in(&plan, 0));
    contains_all(&compile, &["-std=c11", "-DGREETING=\"hello from hk\"", "-DENABLE_SHOUT", "include"]);
    let link = joined(step_in(&plan, 1));
    assert!(link.ends_with("hello.exe") || link.ends_with("hello"), "{link}");
}

// ---------------------------------------------------------------------------
// 02 多模块
// ---------------------------------------------------------------------------

#[test]
fn build_steps_point_compiler_temp_dir_into_the_build_directory() {
    let root = examples_dir().join("01-single-module");
    let project = Project::discover(&root).unwrap();
    let plan = plan_for(&project, "host", "debug", "hello", true);
    let step = step_in(&plan, 0);

    let temp = step
        .cmd
        .envs
        .iter()
        .find(|(key, _)| key == "TEMP")
        .map(|(_, value)| value.clone())
        .expect("编译命令应设置 TEMP");
    let normalized = temp.replace('\\', "/");
    assert!(normalized.ends_with("build/tmp"), "TEMP 应指向构建目录下的 tmp: {temp}");
    for key in ["TMP", "TMPDIR"] {
        assert!(
            step.cmd.envs.iter().any(|(k, v)| k == key && v == &temp),
            "应同时设置 {key}"
        );
    }
    assert_eq!(step.temp_dir.as_deref(), Some(std::path::Path::new(&temp)));
}

#[test]
fn multi_module_example_builds_three_artifact_kinds() {
    let root = examples_dir().join("02-multi-module");
    let project = Project::discover(&root).unwrap();
    assert_eq!(project.modules.len(), 4);
    assert_eq!(
        project.dep_closure("app").unwrap(),
        vec!["core".to_string(), "sensor".to_string()]
    );

    // core：静态库
    let core = plan_for(&project, "host", "debug", "core", true);
    assert_eq!(core.kind, crate::config::Kind::StaticLib);
    let archive = step_in(&core, 1);
    assert!(matches!(archive.kind, StepKind::Archive));
    assert_eq!(archive.cmd.program, "ar");
    assert!(archive.cmd.args.iter().any(|a| a.to_string_lossy() == "rcs"));
    assert!(core.artifacts[0].to_string_lossy().ends_with("libcore.a"));

    // motion：动态库
    let motion = plan_for(&project, "host", "debug", "motion", true);
    assert_eq!(motion.kind, crate::config::Kind::SharedLib);
    let shared = joined(step_in(&motion, 1));
    assert!(shared.contains("-shared"), "{shared}");
    let shared_suffix = if cfg!(target_os = "windows") { ".dll" } else { ".so" };
    assert!(
        motion.artifacts[0].to_string_lossy().ends_with(shared_suffix),
        "{:?}",
        motion.artifacts[0]
    );

    // app：链接两个静态库（依赖者在前）+ 系统库
    let app = plan_for(&project, "host", "debug", "app", true);
    let link = joined(step_in(&app, 1));
    let sensor_pos = link.find("libsensor.a").expect("应链接 libsensor.a");
    let core_pos = link.find("libcore.a").expect("应链接 libcore.a");
    assert!(sensor_pos < core_pos, "依赖者应排在被依赖者之前:\n{link}");
    assert!(link.contains("-lm"), "{link}");
    let app_compile = joined(step_in(&app, 0));
    assert!(
        app_compile.contains("-DCORE_VERSION=2"),
        "core 的 public.defines 应传播到 app 的编译参数:\n{app_compile}"
    );
}

// ---------------------------------------------------------------------------
// 03 STM32 / 04 GD32
// ---------------------------------------------------------------------------

#[test]
fn stm32_example_emits_expected_cross_commands() {
    let root = examples_dir().join("03-mcu-stm32");
    let project = Project::discover(&root).unwrap();
    let target = project.global.target("stm32f407").unwrap();
    assert!(!target.runnable, "MCU 产物不应在本机运行");
    assert!(target.memory_usage);

    let plan = plan_for(&project, "stm32f407", "mcu-release", "firmware", true);
    assert_eq!(plan.kind, crate::config::Kind::Executable);

    let compile = joined(step_in(&plan, 0));
    contains_all(
        &compile,
        &[
            "arm-none-eabi-gcc",
            "-mcpu=cortex-m4",
            "-mthumb",
            "-mfloat-abi=hard",
            "-mfpu=fpv4-sp-d16",
            "-ffunction-sections",
            "-fdata-sections",
            "-Os",
            "-specs=nano.specs",
            "-specs=nosys.specs",
            "-DSTM32F407xx",
            "-DUSE_HAL_DRIVER",
            "-DNDEBUG",
        ],
    );

    let link = joined(step_in(&plan, 1));
    contains_all(
        &link,
        &[
            "arm-none-eabi-gcc",
            "-Wl,--gc-sections",
            "-Wl,--print-memory-usage",
            "-T",
            "STM32F407VGTx_FLASH.ld",
            "-Wl,-e,Reset_Handler",
            "-Wl,-Map,",
            "-s",
            "libbsp.a",
            "libhal.a",
            "firmware.elf",
            "startup_stm32f407",
        ],
    );
    assert!(link.find("libbsp.a").unwrap() < link.find("libhal.a").unwrap(), "{link}");
    assert!(link.contains("main.o"), "firmware 的 main.c 应对应 main.o:\n{link}");

    // 链接后处理：bin / hex / size
    let post = &plan.phases[2];
    let names: Vec<&str> = post.iter().map(|s| s.cmd.program.as_str()).collect();
    assert!(names.contains(&"arm-none-eabi-objcopy"), "{names:?}");
    assert!(names.contains(&"arm-none-eabi-size"), "{names:?}");
    let objcopy_args: Vec<String> = post.iter().flat_map(args_of).collect();
    assert!(objcopy_args.contains(&"binary".to_string()));
    assert!(objcopy_args.contains(&"ihex".to_string()));
    assert!(post.iter().any(|s| matches!(s.kind, StepKind::Size)));
}

#[test]
fn gd32_example_emits_expected_cross_commands() {
    let root = examples_dir().join("04-mcu-gd32");
    let project = Project::discover(&root).unwrap();
    let target = project.global.target("gd32f303").unwrap();
    assert!(!target.runnable);
    assert!(project.global.profiles.contains_key("gd32-release"));

    let plan = plan_for(&project, "gd32f303", "gd32-release", "firmware", true);

    // 编译参数：架构 + 优化档 + 固件库宏
    let compile = joined(step_in(&plan, 0));
    contains_all(
        &compile,
        &[
            "arm-none-eabi-gcc",
            "-mcpu=cortex-m4",
            "-mthumb",
            "-mfloat-abi=hard",
            "-O2",
            "-DGD32F30X_HD",
            "-DUSE_STDPERIPH_DRIVER",
            "-DNDEBUG",
        ],
    );

    // 链接参数：架构参数 + 链接脚本 + 依赖库 + 内存占用报告
    let link = joined(step_in(&plan, 1));
    contains_all(
        &link,
        &[
            "arm-none-eabi-gcc",
            "-mcpu=cortex-m4",
            "-mfloat-abi=hard",
            "GD32F303CCT6_FLASH.ld",
            "-Wl,--print-memory-usage",
            "libbsp.a",
            "libgd32_driver.a",
            "firmware.elf",
            "startup_gd32f303",
        ],
    );

    // 逐文件优化等级：在 gd32-debug 档下只有 gd32_timing.c 带额外的 -O2
    let driver_plan = plan_for(&project, "gd32f303", "gd32-debug", "gd32_driver", true);
    let timing = driver_plan.phases[0]
        .iter()
        .find(|s| args_of(s).iter().any(|a| a.ends_with("gd32_timing.c")))
        .expect("gd32_timing.c 应参与编译");
    assert!(args_of(timing).contains(&"-O2".to_string()), "{}", joined(timing));
    let driver = driver_plan.phases[0]
        .iter()
        .find(|s| args_of(s).iter().any(|a| a.ends_with("gd32_driver.c")))
        .expect("gd32_driver.c 应参与编译");
    assert!(!args_of(driver).contains(&"-O2".to_string()), "{}", joined(driver));

    // 自定义的 size 档：
    let size_plan = plan_for(&project, "gd32f303", "gd32-size", "gd32_driver", true);
    let size_compile = joined(step_in(&size_plan, 0));
    assert!(size_compile.contains("-flto"), "{size_compile}");
}

#[test]
fn mcu_examples_declare_linker_scripts_and_startup_sources() {
    for (example, script) in [
        ("03-mcu-stm32", "STM32F407VGTx_FLASH.ld"),
        ("04-mcu-gd32", "GD32F303CCT6_FLASH.ld"),
    ] {
        let root = examples_dir().join(example);
        let project = Project::discover(&root).unwrap();
        let module = project.module("firmware").unwrap();
        let link = module.config.for_target("host").link();
        let script_path = root.join("firmware").join(link.linker_script.expect("配置了链接脚本"));
        assert!(script_path.is_file(), "{} 不存在", script_path.display());
        let text = fs::read_to_string(&script_path).unwrap();
        assert!(text.contains("MEMORY"), "链接脚本应包含 MEMORY 段（内存占用报告需要）");
        assert!(text.contains("Reset_Handler"), "链接脚本应声明 Reset_Handler");

        // 启动文件必须能被 sources glob 命中
        let target = project.global.targets.keys().next().unwrap().clone();
        let plan = plan_for(&project, &target, "mcu-debug", "firmware", true);
        assert!(
            plan.phases[0].iter().any(|s| args_of(s).iter().any(|a| a.ends_with(".s"))),
            "{example}: 启动文件未参与编译"
        );
        assert!(text.contains("LENGTH"), "{example}: 链接脚本缺少 LENGTH");
        assert!(script.ends_with(".ld"));
    }
}
