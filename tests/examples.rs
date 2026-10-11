//! 示范工程端到端验证：直接调用 hk 二进制。
//!
//! * 本机示例（01 单模块 / 02 多模块）：复制到临时目录后真实编译并运行，断言输出。
//! * MCU 示例（03 STM32F407 / 04 GD32F303）：用 --print-plan 断言交叉编译命令与后处理产物，
//!   不需要安装 arm-none-eabi 工具链。
//!
//! 构建都在临时目录里进行，不会污染仓库的 examples/。

mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::process::Command;

fn example_dir(name: &str) -> PathBuf {
    manifest_dir().join("examples").join(name)
}

/// 把示范工程复制到临时目录后返回（工程根 = 示例目录本身）。
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
    assert!(exe.exists(), "产物不存在: {}", exe.display());
    let out = Command::new(&exe).output().expect("运行产物失败");
    assert!(out.status.success(), "产物退出码非 0: {}", exe.display());
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn single_module_example_builds_and_runs() {
    if !gcc_available() {
        eprintln!("跳过 single_module_example_builds_and_runs：本机没有 gcc/ar");
        return;
    }
    let (_tmp, project) = staged_example("01-single-module");
    let module = project.join("hello");

    run_hk(&module, &["build"]).assert_ok("构建单模块示例").assert_contains("构建完成");
    let output = run_artifact(&module, "hello");
    assert_eq!(output.trim(), "HELLO FROM HK (answer=42)", "单模块示例输出不符: {output}");

    // 二次构建应全部跳过
    run_hk(&module, &["build"]).assert_ok("增量构建").assert_contains("执行 0 个步骤");
}

#[test]
fn multi_module_example_builds_and_runs() {
    if !gcc_available() {
        eprintln!("跳过 multi_module_example_builds_and_runs：本机没有 gcc/ar");
        return;
    }
    let (_tmp, project) = staged_example("02-multi-module");

    let build = run_hk(&project, &["build"]);
    build.assert_ok("构建多模块示例").assert_contains("构建完成");
    // 3 种产物：静态库 libcore/libensor、动态库 libmotion、可执行 app
    assert!(project.join("core/build/lib/libcore.a").exists(), "缺少 libcore.a");
    assert!(project.join("sensor/build/lib/libsensor.a").exists(), "缺少 libsensor.a");
    let shared = if cfg!(windows) { "motion/build/lib/libmotion.dll" } else { "motion/build/lib/libmotion.so" };
    assert!(project.join(shared).exists(), "缺少动态库 {shared}");

    let output = run_artifact(&project.join("app"), "app");
    // sensor_read(6, 7) = (6*7)+1 = 43；CORE_VERSION 由 core 的 public.defines 传播
    let first = output.lines().next().unwrap_or_default();
    assert_eq!(first.trim(), "sensor=43 core=2 sqrt=4", "多模块示例输出不符: {output}");
    assert!(output.contains("target=host"), "按目标覆盖的 HOST_BUILD 应生效: {output}");

    // hk run 也应可用
    run_hk(&project, &["run", "--bin", "app"]).assert_ok("hk run").assert_contains("sensor=43");
}

#[test]
fn consumer_can_link_against_the_shared_library() {
    if !gcc_available() {
        eprintln!("跳过 consumer_can_link_against_the_shared_library：本机没有 gcc/ar");
        return;
    }
    let (_tmp, project) = staged_example("02-multi-module");
    let app_config = project.join("app/config.yaml");
    let text = read(&app_config).replace("mods: [sensor]", "mods: [sensor, motion]");
    write(&app_config, &text);

    // -v 会打印真正执行的命令行，据此断言动态库进入了链接输入
    run_hk(&project, &["build", "-v"])
        .assert_ok("链接动态库")
        .assert_contains("libmotion");
    let shared = if cfg!(windows) {
        project.join("motion/build/lib/libmotion.dll")
    } else {
        project.join("motion/build/lib/libmotion.so")
    };
    assert!(shared.exists(), "动态库未生成: {}", shared.display());
    let exe = project.join("app/build/bin").join(if cfg!(windows) { "app.exe" } else { "app" });
    assert!(exe.exists(), "链接动态库后未生成可执行文件");
}

#[test]
fn build_survives_an_unusable_ambient_temp_dir() {
    if !gcc_available() {
        eprintln!("跳过 build_survives_an_unusable_ambient_temp_dir：本机没有 gcc/ar");
        return;
    }
    let (_tmp, project) = staged_example("01-single-module");
    let module = project.join("hello");

    // 把 TEMP/TMP/TMPDIR 指向一个不存在的目录：gcc 自己建的临时文件会失败，
    // hk 必须把编译器的临时目录换成构建目录下的 tmp。
    let bogus = project.join("no-such-temp-dir");
    let bogus = bogus.to_string_lossy().to_string();
    run_hk_with_env(
        &module,
        &["build"],
        &[("TEMP", &bogus), ("TMP", &bogus), ("TMPDIR", &bogus)],
    )
    .assert_ok("TEMP 不可用时构建")
    .assert_contains("构建完成");

    assert!(module.join("build/tmp").is_dir(), "应在构建目录下创建临时目录");
    let output = run_artifact(&module, "hello");
    assert!(output.contains("HELLO FROM HK"), "{output}");
}

#[test]
fn building_from_the_examples_root_only_builds_the_standalone_module() {
    if !gcc_available() {
        eprintln!("跳过 building_from_the_examples_root_only_builds_the_standalone_module：本机没有 gcc/ar");
        return;
    }
    let tmp = TempDir::new("examples-root");
    let root = tmp.path().join("examples");
    copy_tree(&manifest_dir().join("examples"), &root);

    // 在多个工程的上层目录运行：子工程被当作边界跳过，不再报"模块重名"
    run_hk(&root, &["build"])
        .assert_ok("在 examples 根目录构建")
        .assert_contains("跳过子工程")
        .assert_contains("hello: 编译");
    assert!(root.join("01-single-module/hello/build/bin/hello.exe").exists()
        || root.join("01-single-module/hello/build/bin/hello").exists());
}

#[test]
fn stm32_example_plan_is_complete() {
    let (_tmp, project) = staged_example("03-mcu-stm32");
    let out = run_hk(&project, &["build", "-t", "stm32f407", "-m", "mcu-release", "--print-plan"]);
    out.assert_ok("STM32 示例 print-plan")
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

    assert!(!project.join("firmware/build").exists(), "print-plan 不应真的构建");

    // 计划可落盘
    run_hk(&project, &["build", "-t", "stm32f407", "--print-plan", "-o", "plan.txt"]).assert_ok("计划落盘");
    assert!(read(&project.join("plan.txt")).contains("STM32F407VGTx_FLASH.ld"));
}

#[test]
fn gd32_example_plan_is_complete() {
    let (_tmp, project) = staged_example("04-mcu-gd32");
    let out = run_hk(&project, &["build", "-t", "gd32f303", "-m", "gd32-release", "--print-plan"]);
    out.assert_ok("GD32 示例 print-plan")
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

    // gd32-size 档启用 LTO
    run_hk(&project, &["build", "-t", "gd32f303", "-m", "gd32-size", "--print-plan"])
        .assert_ok("gd32-size print-plan")
        .assert_contains("-flto");
}

#[test]
fn mcu_examples_refuse_to_run_locally() {
    for (example, target) in [("03-mcu-stm32", "stm32f407"), ("04-mcu-gd32", "gd32f303")] {
        let (_tmp, project) = staged_example(example);
        run_hk(&project, &["run", "-t", target])
            .assert_failed("MCU 产物不应在本机运行")
            .assert_contains("无法在本机运行");
    }
}

#[test]
fn examples_report_their_effective_configuration() {
    let (_tmp, project) = staged_example("03-mcu-stm32");
    run_hk(&project, &["config", "-t", "stm32f407"])
        .assert_ok("hk config -t stm32f407")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("可本机运行    = false")
        .assert_contains("STM32");
    // 单模块示例没有 hk.yaml，也必须能正常构建（使用内置默认配置）
    let (_tmp2, single) = staged_example("01-single-module");
    assert!(!single.join("hk.yaml").exists());
    run_hk(&single.join("hello"), &["config"]).assert_ok("单模块示例 hk config").assert_contains("toolchains:");
}
