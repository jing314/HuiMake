//! End-to-end verification of the example projects, driving the real hk binary.
//!
//! * host examples (01 single module / 02 multi module): copied to a temp directory,
//!   really compiled and run, then their output is asserted.
//! * MCU examples (03 STM32F407 / 04 GD32F303): --print-plan output is asserted for the
//!   cross-compile commands and post-link artifacts; no arm-none-eabi toolchain needed.
//!
//! Everything is built in temp directories, so examples/ stays clean.

mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::process::Command;

fn example_dir(name: &str) -> PathBuf {
    manifest_dir().join("examples").join(name)
}

/// Copy an example into a temp directory (the example directory is the project root).
fn staged_example(name: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new(name);
    let dst = tmp.path().join(name);
    copy_tree(&example_dir(name), &dst);
    (tmp, dst)
}

fn run_artifact(dir: &Path, name: &str) -> String {
    let exe = dir.join("build").join("bin").join(name);
    let exe = if exe.exists() {
        exe
    } else {
        dir.join("build").join("bin").join(format!("{name}.exe"))
    };
    assert!(exe.exists(), "artifact does not exist: {}", exe.display());
    let out = Command::new(&exe).output().expect("failed to run the artifact");
    assert!(out.status.success(), "artifact exited with a non-zero code: {}", exe.display());
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn single_module_example_builds_and_runs() {
    if !gcc_available() {
        eprintln!("skipping single_module_example_builds_and_runs: no gcc/ar on this host");
        return;
    }
    let (_tmp, project) = staged_example("01-single-module");
    let module = project.join("hello");

    run_hk(&module, &["build"]).assert_ok("build the single-module example").assert_contains("Build finished");
    let output = run_artifact(&module, "hello");
    assert_eq!(output.trim(), "HELLO FROM HK (answer=42)", "unexpected output of the single-module example: {output}");

    // A second build must skip everything
    run_hk(&module, &["build"]).assert_ok("incremental build").assert_contains("0 step(s) executed");
}

#[test]
fn multi_module_example_builds_and_runs() {
    if !gcc_available() {
        eprintln!("skipping multi_module_example_builds_and_runs: no gcc/ar on this host");
        return;
    }
    let (_tmp, project) = staged_example("02-multi-module");

    let build = run_hk(&project, &["build"]);
    build.assert_ok("build the multi-module example").assert_contains("Build finished");
    // three artifact kinds: static libs libcore/libsensor, shared lib libmotion, executable app
    assert!(project.join("core/build/lib/libcore.a").exists(), "libcore.a is missing");
    assert!(project.join("sensor/build/lib/libsensor.a").exists(), "libsensor.a is missing");
    let shared = if cfg!(windows) { "motion/build/lib/libmotion.dll" } else { "motion/build/lib/libmotion.so" };
    assert!(project.join(shared).exists(), "shared library {shared} is missing");

    let output = run_artifact(&project.join("app"), "app");
    // sensor_read(6, 7) = (6*7)+1 = 43; CORE_VERSION comes from core's public.defines
    let first = output.lines().next().unwrap_or_default();
    assert_eq!(first.trim(), "sensor=43 core=2 sqrt=4", "unexpected output of the multi-module example: {output}");
    assert!(output.contains("target=host"), "the per-target HOST_BUILD override must apply: {output}");

    // hk run must work as well
    run_hk(&project, &["run", "--bin", "app"]).assert_ok("hk run").assert_contains("sensor=43");
}

#[test]
fn consumer_can_link_against_the_shared_library() {
    if !gcc_available() {
        eprintln!("skipping consumer_can_link_against_the_shared_library: no gcc/ar on this host");
        return;
    }
    let (_tmp, project) = staged_example("02-multi-module");
    let app_config = project.join("app/config.yaml");
    let text = read(&app_config).replace("mods: [sensor]", "mods: [sensor, motion]");
    write(&app_config, &text);

    // -v prints the real command lines, so the shared library must show up in the link input
    run_hk(&project, &["build", "-v"])
        .assert_ok("link the shared library")
        .assert_contains("libmotion");
    let shared = if cfg!(windows) {
        project.join("motion/build/lib/libmotion.dll")
    } else {
        project.join("motion/build/lib/libmotion.so")
    };
    assert!(shared.exists(), "shared library was not produced: {}", shared.display());
    let exe = project.join("app/build/bin").join(if cfg!(windows) { "app.exe" } else { "app" });
    assert!(exe.exists(), "no executable was produced after linking the shared library");
}

#[test]
fn build_survives_an_unusable_ambient_temp_dir() {
    if !gcc_available() {
        eprintln!("skipping build_survives_an_unusable_ambient_temp_dir: no gcc/ar on this host");
        return;
    }
    let (_tmp, project) = staged_example("01-single-module");
    let module = project.join("hello");

    // Point TEMP/TMP/TMPDIR at a directory that does not exist: gcc's own temp files
    // would fail, so hk must redirect the compiler to <module>/build/tmp.
    let bogus = project.join("no-such-temp-dir");
    let bogus = bogus.to_string_lossy().to_string();
    run_hk_with_env(
        &module,
        &["build"],
        &[("TEMP", &bogus), ("TMP", &bogus), ("TMPDIR", &bogus)],
    )
    .assert_ok("build with an unusable TEMP")
    .assert_contains("Build finished");

    assert!(module.join("build/tmp").is_dir(), "the temp directory must be created inside the build directory");
    let output = run_artifact(&module, "hello");
    assert!(output.contains("HELLO FROM HK"), "{output}");
}

#[test]
fn building_from_the_examples_root_only_builds_the_standalone_module() {
    if !gcc_available() {
        eprintln!("skipping building_from_the_examples_root_only_builds_the_standalone_module: no gcc/ar on this host");
        return;
    }
    let tmp = TempDir::new("examples-root");
    let root = tmp.path().join("examples");
    copy_tree(&manifest_dir().join("examples"), &root);

    // Running in a parent directory of several projects: nested projects are skipped as
    // boundaries instead of failing with "duplicate module name"
    run_hk(&root, &["build"])
        .assert_ok("build from the examples root")
        .assert_contains("skipping nested project")
        .assert_contains("hello: compiling");
    assert!(root.join("01-single-module/hello/build/bin/hello.exe").exists()
        || root.join("01-single-module/hello/build/bin/hello").exists());
}

#[test]
fn stm32_example_plan_is_complete() {
    let (_tmp, project) = staged_example("03-mcu-stm32");
    let out = run_hk(&project, &["build", "-t", "stm32f407", "-m", "mcu-release", "--print-plan"]);
    out.assert_ok("STM32 example print-plan")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("-mcpu=cortex-m4")
        .assert_contains("-mfpu=fpv4-sp-d16")
        .assert_contains("-specs=nano.specs")
        .assert_contains("-Wl,--gc-sections")
        .assert_contains("-Wl,--print-memory-usage")
        .assert_contains("-T")
        .assert_contains("STM32F407VGTx_FLASH.ld")
        .assert_contains("-DSTM32F407xx")
        .assert_contains("USE_HAL_DRIVER")
        .assert_contains("startup_stm32f407")
        .assert_contains("libbsp.a")
        .assert_contains("libhal.a")
        .assert_contains("firmware.elf")
        .assert_contains("firmware.bin")
        .assert_contains("firmware.hex")
        .assert_contains("arm-none-eabi-objcopy -O binary")
        .assert_contains("arm-none-eabi-objcopy -O ihex")
        .assert_contains("arm-none-eabi-size");

    assert!(!project.join("firmware/build").exists(), "print-plan must not build anything");

    // The plan can be written to a file
    run_hk(&project, &["build", "-t", "stm32f407", "--print-plan", "-o", "plan.txt"]).assert_ok("plan written to disk");
    assert!(read(&project.join("plan.txt")).contains("STM32F407VGTx_FLASH.ld"));
}

#[test]
fn gd32_example_plan_is_complete() {
    let (_tmp, project) = staged_example("04-mcu-gd32");
    let out = run_hk(&project, &["build", "-t", "gd32f303", "-m", "gd32-release", "--print-plan"]);
    out.assert_ok("GD32 example print-plan")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("-mcpu=cortex-m4")
        .assert_contains("-mfloat-abi=hard")
        .assert_contains("-O2")
        .assert_contains("-DGD32F30X_HD")
        .assert_contains("-DUSE_STDPERIPH_DRIVER")
        .assert_contains("GD32F303CCT6_FLASH.ld")
        .assert_contains("-Wl,--print-memory-usage")
        .assert_contains("startup_gd32f303")
        .assert_contains("libbsp.a")
        .assert_contains("libgd32_driver.a")
        .assert_contains("firmware.elf")
        .assert_contains("arm-none-eabi-objcopy -O binary")
        .assert_contains("arm-none-eabi-size");

    // the gd32-size profile enables LTO
    run_hk(&project, &["build", "-t", "gd32f303", "-m", "gd32-size", "--print-plan"])
        .assert_ok("gd32-size print-plan")
        .assert_contains("-flto");
}

#[test]
fn mcu_examples_refuse_to_run_locally() {
    for (example, target) in [("03-mcu-stm32", "stm32f407"), ("04-mcu-gd32", "gd32f303")] {
        let (_tmp, project) = staged_example(example);
        run_hk(&project, &["run", "-t", target])
            .assert_failed("an MCU artifact must not run locally")
            .assert_contains("cannot run on this host");
    }
}

#[test]
fn examples_report_their_effective_configuration() {
    let (_tmp, project) = staged_example("03-mcu-stm32");
    run_hk(&project, &["config", "-t", "stm32f407"])
        .assert_ok("hk config -t stm32f407")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("runnable      = false")
        .assert_contains("STM32");
    // The single-module example has no hk.yaml and must still build (built-in defaults)
    let (_tmp2, single) = staged_example("01-single-module");
    assert!(!single.join("hk.yaml").exists());
    run_hk(&single.join("hello"), &["config"]).assert_ok("hk config of the single-module example").assert_contains("toolchains:");
}
