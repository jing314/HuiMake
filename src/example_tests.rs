//! Unit tests driven by the projects under examples/.
//!
//! Pure Rust, no gcc or cross toolchain needed: the example projects are the input.
//! Each one is parsed, its modules discovered and ordered, and a build plan is created
//! for every target x profile combination, asserting the key compile/link/post flags.
//!
//! Coverage: single module, multi module (static/shared/executable) and MCU
//! (STM32F407, GD32F303).

use crate::build::steps::{self, ModulePlan, PlanCtx, Step, StepKind};
use crate::config::{Config, GlobalConfig, MODULE_FILE_NAME};
use crate::project::Project;
use std::fs;
use std::path::{Path, PathBuf};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Example projects that carry an hk.yaml (multi-module / MCU).
fn project_examples() -> Vec<PathBuf> {
    let root = examples_dir();
    let mut out = Vec::new();
    for entry in fs::read_dir(&root).unwrap_or_else(|e| panic!("failed to read {}: {e}", root.display())) {
        let path = entry.expect("directory entry").path();
        if path.is_dir() && path.join("hk.yaml").is_file() {
            out.push(path);
        }
    }
    out.sort();
    assert!(out.len() >= 3, "expected at least 3 examples with an hk.yaml: {out:?}");
    out
}

/// All example roots (the single-module example has no hk.yaml, so add it explicitly).
fn all_example_roots() -> Vec<PathBuf> {
    let mut out = vec![examples_dir().join("01-single-module")];
    out.extend(project_examples());
    out
}

fn collect_config_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display())) {
        let path = entry.expect("directory entry").path();
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
    let target = project.global.target(target_name).expect("target exists");
    let profile = project.global.profile(profile_name).expect("profile exists");
    let toolchain = project.global.toolchain(&target.toolchain).expect("toolchain exists");
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
        .unwrap_or_else(|e| panic!("failed to plan {}/{target_name}/{profile_name}/{module}: {e}", project.root.display()))
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
        .unwrap_or_else(|| panic!("module {} has no step in phase {phase}", plan.module))
}

fn contains_all(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "missing {needle:?}:\n{text}");
    }
}

// ---------------------------------------------------------------------------
// configuration and project structure
// ---------------------------------------------------------------------------

#[test]
fn every_example_global_config_is_valid() {
    for dir in project_examples() {
        let cfg = GlobalConfig::load(Some(&dir))
            .unwrap_or_else(|e| panic!("failed to load the hk.yaml of {}: {e}", dir.display()));
        assert!(!cfg.targets.is_empty(), "{} defines no targets", dir.display());
        assert!(!cfg.profiles.is_empty(), "{} defines no profiles", dir.display());
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
        assert!(!files.is_empty(), "no config.yaml found under {}", root.display());
        for file in files {
            let text = file.display().to_string();
            Config::load_file(&file).unwrap_or_else(|e| panic!("failed to parse {text}: {e}"));
            count += 1;
        }
    }
    assert!(count >= 11, "the examples must cover multi-module and MCU setups, found {count}");
}

#[test]
fn every_example_project_resolves_dependencies_and_topology() {
    for root in all_example_roots() {
        let project = Project::discover(&root)
            .unwrap_or_else(|e| panic!("failed to discover {}: {e}", root.display()));
        assert!(!project.modules.is_empty(), "{} has no modules", root.display());
        let levels = project.topo_levels()
            .unwrap_or_else(|e| panic!("dependency ordering of {} failed: {e}", root.display()));
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
                    // every module except header-only must produce a plan or artifacts
                    if module != "unused" {
                        assert!(
                            !plan.artifacts.is_empty() || plan.kind == crate::config::Kind::HeaderOnly,
                            "{module} produces nothing for {target}/{profile}"
                        );
                    }
                }
            }
        }
    }
    assert!(combos >= 30, "too few target x profile x module combinations: {combos}");
}

// ---------------------------------------------------------------------------
// 01 single module
// ---------------------------------------------------------------------------

#[test]
fn examples_root_is_a_container_not_a_project() {
    // Running hk in the parent directory must only see modules that belong to no nested
    // project; the modules of 02/03/04 may not be merged into one project (bsp/firmware
    // would collide).
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
    assert_eq!(project.modules.len(), 1, "the single-module example has exactly one module");

    let plan = plan_for(&project, "host", "debug", "hello", true);
    assert_eq!(plan.kind, crate::config::Kind::Executable);
    assert_eq!(plan.phases[0].len(), 2, "both src/greet.c and bin/main.c must be compiled");
    let sources = plan.phases[0]
        .iter()
        .map(|s| {
            let args = args_of(s);
            let index = args.iter().position(|a| a == "-c").expect("a compile command must contain -c");
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
// 02 multi module
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
        .expect("a compile command must set TEMP");
    let normalized = temp.replace('\\', "/");
    assert!(normalized.ends_with("build/tmp"), "TEMP must point at <build>/tmp: {temp}");
    for key in ["TMP", "TMPDIR"] {
        assert!(
            step.cmd.envs.iter().any(|(k, v)| k == key && v == &temp),
            "{key} must be set as well"
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

    // core: static library
    let core = plan_for(&project, "host", "debug", "core", true);
    assert_eq!(core.kind, crate::config::Kind::StaticLib);
    let archive = step_in(&core, 1);
    assert!(matches!(archive.kind, StepKind::Archive));
    assert_eq!(archive.cmd.program, "ar");
    assert!(archive.cmd.args.iter().any(|a| a.to_string_lossy() == "rcs"));
    assert!(core.artifacts[0].to_string_lossy().ends_with("libcore.a"));

    // motion: shared library
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

    // app: links two static libraries (users first) plus a system library
    let app = plan_for(&project, "host", "debug", "app", true);
    let link = joined(step_in(&app, 1));
    let sensor_pos = link.find("libsensor.a").expect("libsensor.a must be linked");
    let core_pos = link.find("libcore.a").expect("libcore.a must be linked");
    assert!(sensor_pos < core_pos, "users must come before providers:\n{link}");
    assert!(link.contains("-lm"), "{link}");
    let app_compile = joined(step_in(&app, 0));
    assert!(
        app_compile.contains("-DCORE_VERSION=2"),
        "core public.defines must reach the compile flags of app:\n{app_compile}"
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
    assert!(!target.runnable, "an MCU artifact must not run on this host");
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
    assert!(link.contains("main.o"), "firmware main.c must show up as main.o:\n{link}");

    // post-link steps: bin / hex / size
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

    // compile flags: architecture, optimisation level, firmware library macros
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

    // link flags: architecture, linker script, dependencies, memory usage report
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

    // per-file optimisation: only gd32_timing.c gets the extra -O2 in the gd32-debug profile
    let driver_plan = plan_for(&project, "gd32f303", "gd32-debug", "gd32_driver", true);
    let timing = driver_plan.phases[0]
        .iter()
        .find(|s| args_of(s).iter().any(|a| a.ends_with("gd32_timing.c")))
        .expect("gd32_timing.c must be compiled");
    assert!(args_of(timing).contains(&"-O2".to_string()), "{}", joined(timing));
    let driver = driver_plan.phases[0]
        .iter()
        .find(|s| args_of(s).iter().any(|a| a.ends_with("gd32_driver.c")))
        .expect("gd32_driver.c must be compiled");
    assert!(!args_of(driver).contains(&"-O2".to_string()), "{}", joined(driver));

    // custom size profile:
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
        let script_path = root.join("firmware").join(link.linker_script.expect("a linker script is configured"));
        assert!(script_path.is_file(), "{} does not exist", script_path.display());
        let text = fs::read_to_string(&script_path).unwrap();
        assert!(text.contains("MEMORY"), "the linker script needs a MEMORY section for the usage report");
        assert!(text.contains("Reset_Handler"), "the linker script must declare Reset_Handler");

        // the startup file must be matched by the sources glob
        let target = project.global.targets.keys().next().unwrap().clone();
        let plan = plan_for(&project, &target, "mcu-debug", "firmware", true);
        assert!(
            plan.phases[0].iter().any(|s| args_of(s).iter().any(|a| a.ends_with(".s"))),
            "{example}: the startup file is not part of the build"
        );
        assert!(text.contains("LENGTH"), "{example}: the linker script has no LENGTH");
        assert!(script.ends_with(".ld"));
    }
}
