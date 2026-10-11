//! 端到端测试：真正调用 hk 二进制，并用本机 gcc / binutils 完成构建。
//!
//! 需要 gcc 的用例在缺少工具链时会自动跳过（打印说明），保证在没有编译环境的
//! 机器上 cargo test 依然可以通过。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn hk_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hk"))
}

fn has_tool(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn gcc_available() -> bool {
    has_tool("gcc") && has_tool("ar")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "hk-e2e-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, content).expect("write file");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Run {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    fn assert_ok(&self, what: &str) -> &Self {
        assert!(self.ok, "{what} 应成功，实际输出:\n{}", self.all());
        self
    }

    fn assert_failed(&self, what: &str) -> &Self {
        assert!(!self.ok, "{what} 应失败，实际输出:\n{}", self.all());
        self
    }

    fn assert_contains(&self, needle: &str) -> &Self {
        assert!(
            self.all().contains(needle),
            "输出中缺少 {needle:?}:\n{}",
            self.all()
        );
        self
    }

}

fn run_hk(dir: &Path, args: &[&str]) -> Run {
    let out = Command::new(hk_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("执行 hk 失败");
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

const HOST_YAML: &str = r#"
toolchains:
  mingw:
    cc: gcc
    cxx: g++
    ar: ar
    objcopy: objcopy
    objdump: objdump
    size: size

targets:
  sim:
    toolchain: mingw
    cflags: [-ffunction-sections, -fdata-sections]
    ldflags: ["-Wl,--gc-sections"]
    post: [elf, bin, hex, map, size]
    memory_usage: true
    exe_suffix: .elf
    runnable: false

  cortex-m4f:
    toolchain: arm-none-eabi
    cflags: [-mcpu=cortex-m4, -mthumb, -mfloat-abi=hard, -mfpu=fpv4-sp-d16, -ffunction-sections, -fdata-sections]
    ldflags: ["-Wl,--gc-sections"]
    specs: [nano.specs, nosys.specs]
    post: [elf, bin, hex, map, size]
    memory_usage: true
    runnable: false
    exe_suffix: .elf

profiles:
  mcu-release:
    cflags: [-Os]
    defines: [NDEBUG]
    strip: true
    post: [elf, bin, hex, map, size]
"#;

/// 生成一个 hal(静态库) + app(可执行) 的两模块工程。
fn scaffold_project(root: &Path) {
    write(&root.join("hk.yaml"), HOST_YAML);
    write(
        &root.join("hal/config.yaml"),
        "name: hal\nkind: static-lib\nsources: [src/**/*.c]\n\npublic:\n  defines: [HAL_ENABLED=1]\n",
    );
    write(
        &root.join("hal/include/hal.h"),
        "#ifndef HAL_H\n#define HAL_H\n\nint hal_value(void);\n\n#endif\n",
    );
    write(
        &root.join("hal/src/hal.c"),
        "#include \"hal.h\"\n\nint hal_value(void)\n{\n    return 42;\n}\n",
    );
    write(
        &root.join("app/config.yaml"),
        "name: app\nkind: executable\nsources: [bin/**/*.c]\ndefines:\n  - 'APP_TAG=\"v1\"'\ndeps:\n  mods: [hal]\n\
         \ntargets:\n  cortex-m4f:\n    defines: [STM32F407xx]\n    link:\n      linker_script: link/fw.ld\n",
    );
    write(
        &root.join("app/bin/main.c"),
        "#include <stdio.h>\n#include \"hal.h\"\n\n#ifndef HAL_ENABLED\n#error \"public.defines 没有传播到依赖方\"\n#endif\n\nint main(void)\n{\n    printf(\"hal=%d tag=%s\\n\", hal_value(), APP_TAG);\n    return 0;\n}\n",
    );
}

#[test]
fn native_project_builds_links_and_runs() {
    if !gcc_available() {
        eprintln!("跳过 native_project_builds_links_and_runs：本机没有 gcc/ar");
        return;
    }
    let tmp = TempDir::new("native");
    scaffold_project(tmp.path());

    run_hk(tmp.path(), &["build"]).assert_ok("hk build").assert_contains("构建完成");

    let exe = tmp.path().join("app/build/bin/app.exe");
    let exe = if exe.exists() { exe } else { tmp.path().join("app/build/bin/app") };
    assert!(exe.exists(), "可执行文件未生成: {}", exe.display());
    let lib = tmp.path().join("hal/build/lib/libhal.a");
    assert!(lib.exists(), "静态库未生成: {}", lib.display());

    let out = Command::new(&exe).output().expect("运行产物");
    assert!(out.status.success(), "产物退出码非 0");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hal=42 tag=v1");

    // hk run 也要能工作
    run_hk(tmp.path().join("app").as_path(), &["run"])
        .assert_ok("hk run")
        .assert_contains("hal=42 tag=v1");
}

#[test]
fn incremental_build_skips_unchanged_and_rebuilds_on_change() {
    if !gcc_available() {
        eprintln!("跳过 incremental_build_skips_unchanged_and_rebuilds_on_change：本机没有 gcc/ar");
        return;
    }
    let tmp = TempDir::new("incremental");
    scaffold_project(tmp.path());

    run_hk(tmp.path(), &["build"]).assert_ok("首次构建");
    let app_obj = tmp.path().join("app/build/obj/bin/main.o");
    let first_mtime = fs::metadata(&app_obj).expect("app 目标文件").modified().unwrap();

    let second = run_hk(tmp.path(), &["build"]);
    second.assert_ok("二次构建").assert_contains("执行 0 个步骤");

    // 改动 hal 的实现：只有 hal 需要重编，app 不应重编
    let hal_src = tmp.path().join("hal/src/hal.c");
    let mut text = read(&hal_src);
    text.push_str("\nint hal_extra(void)\n{\n    return 1;\n}\n");
    write(&hal_src, &text);
    let third = run_hk(tmp.path(), &["build"]);
    third.assert_ok("改动实现后构建").assert_contains("hal: 编译 1 个源文件");
    assert!(!third.all().contains("app: 编译"), "app 不应重编:\n{}", third.all());

    // 改动被 app 间接依赖的头文件：app 必须重编（依赖文件生效）
    let hal_h = tmp.path().join("hal/include/hal.h");
    let mut header = read(&hal_h);
    header.push_str("\n/* touched */\n");
    write(&hal_h, &header);
    let fourth = run_hk(tmp.path(), &["build"]);
    fourth.assert_ok("改动头文件后构建").assert_contains("app: 编译 1 个源文件");
    let new_mtime = fs::metadata(&app_obj).unwrap().modified().unwrap();
    assert!(new_mtime > first_mtime, "app 目标文件应被重新生成");

    // --force 必须全量重建
    let forced = run_hk(tmp.path(), &["build", "--force"]);
    forced
        .assert_ok("强制重建")
        .assert_contains("app: 编译 1 个源文件")
        .assert_contains("hal: 编译 1 个源文件");
}

#[test]
fn print_plan_shows_cross_toolchain_commands_without_building() {
    let tmp = TempDir::new("plan");
    scaffold_project(tmp.path());

    let out = run_hk(
        tmp.path(),
        &["build", "-t", "cortex-m4f", "-m", "mcu-release", "--print-plan"],
    );
    out.assert_ok("--print-plan")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("-mcpu=cortex-m4")
        .assert_contains("-mthumb")
        .assert_contains("-Os")
        .assert_contains("-specs=nano.specs")
        .assert_contains("-specs=nosys.specs")
        .assert_contains("-Wl,--gc-sections")
        .assert_contains("-Wl,--print-memory-usage")
        .assert_contains("-T")
        .assert_contains("fw.ld")
        .assert_contains("app.map")
        .assert_contains("-DAPP_TAG=")
        .assert_contains("-DSTM32F407xx")
        .assert_contains("arm-none-eabi-objcopy -O binary")
        .assert_contains("arm-none-eabi-objcopy -O ihex")
        .assert_contains("arm-none-eabi-size")
        .assert_contains("app.map");

    // 计划模式绝不能产生任何构建产物
    assert!(!tmp.path().join("app/build").exists(), "print-plan 不应执行编译");

    // 计划可以落盘
    run_hk(
        tmp.path(),
        &["build", "-t", "cortex-m4f", "--print-plan", "--output", "plan.txt"],
    )
    .assert_ok("计划落盘");
    assert!(read(&tmp.path().join("plan.txt")).contains("arm-none-eabi-gcc"));
}

#[test]
fn mcu_post_pipeline_produces_binary_hex_map_and_size() {
    if !gcc_available() || !has_tool("objcopy") || !has_tool("size") {
        eprintln!("跳过 mcu_post_pipeline...：本机缺少 gcc/objcopy/size");
        return;
    }
    let tmp = TempDir::new("mcu");
    scaffold_project(tmp.path());

    let out = run_hk(tmp.path(), &["build", "-t", "sim", "-m", "mcu-release"]);
    out.assert_ok("MCU 目标构建").assert_contains("段大小");

    let bin_dir = tmp.path().join("app/build/bin");
    for name in ["app.elf", "app.bin", "app.hex", "app.map"] {
        assert!(bin_dir.join(name).exists(), "缺少产物 {name}（目录内容: {:?}）", fs::read_dir(&bin_dir).map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name())).collect::<Vec<_>>()));
    }
    let bin = fs::read(bin_dir.join("app.bin")).unwrap();
    assert!(!bin.is_empty(), "bin 文件不应为空");
}

#[test]
fn run_refuses_targets_that_cannot_execute_locally() {
    let tmp = TempDir::new("runnoguard");
    scaffold_project(tmp.path());
    run_hk(tmp.path(), &["run", "-t", "sim"])
        .assert_failed("MCU 目标不应被运行")
        .assert_contains("无法在本机运行");
}

#[test]
fn new_module_scaffold_builds_out_of_the_box() {
    if !gcc_available() {
        eprintln!("跳过 new_module_scaffold_builds_out_of_the_box：本机没有 gcc/ar");
        return;
    }
    let tmp = TempDir::new("newmod");
    run_hk(tmp.path(), &["new", "demo"]).assert_ok("hk new");

    let config = read(&tmp.path().join("demo/config.yaml"));
    assert!(config.contains("name: demo"), "生成的配置名应是模块名: {config}");

    let demo = tmp.path().join("demo");
    run_hk(&demo, &["build"]).assert_ok("hk build（新模块）");
    run_hk(&demo, &["run"])
        .assert_ok("hk run（新模块）")
        .assert_contains("hello from HK");

    // 静态库模板
    run_hk(tmp.path(), &["new", "mylib", "--kind", "static-lib"]).assert_ok("hk new --kind static-lib");
    run_hk(&tmp.path().join("mylib"), &["build"]).assert_ok("构建静态库模块");
    assert!(tmp.path().join("mylib/build/lib/libmylib.a").exists());
}

#[test]
fn configuration_errors_are_actionable() {
    let tmp = TempDir::new("errors");
    scaffold_project(tmp.path());

    run_hk(tmp.path(), &["build", "-t", "no-such-target"])
        .assert_failed("未知目标应当报错")
        .assert_contains("未知目标")
        .assert_contains("cortex-m4f");

    run_hk(tmp.path(), &["build", "-m", "no-such-mode"])
        .assert_failed("未知模式应当报错")
        .assert_contains("未知模式")
        .assert_contains("release");

    // 依赖写错必须报错，而不是静默忽略
    write(
        &tmp.path().join("app/config.yaml"),
        "name: app\ndeps:\n  mods: [ghost]\n",
    );
    run_hk(tmp.path(), &["build"])
        .assert_failed("未知依赖应当报错")
        .assert_contains("ghost");

    // 未知字段必须报错（防止配置写错却静默生效）
    write(
        &tmp.path().join("app/config.yaml"),
        "name: app\nkindd: executable\n",
    );
    run_hk(tmp.path(), &["build"])
        .assert_failed("未知字段应当报错")
        .assert_contains("kindd");
}

#[test]
fn config_and_init_export_yaml() {
    let tmp = TempDir::new("config");
    scaffold_project(tmp.path());

    let out = run_hk(tmp.path(), &["config", "--target", "cortex-m4f"]);
    out.assert_ok("hk config --target")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("可本机运行    = false");

    let dump = run_hk(tmp.path(), &["config"]);
    dump.assert_ok("hk config").assert_contains("toolchains:");

    let empty = TempDir::new("init");
    run_hk(empty.path(), &["init"]).assert_ok("hk init");
    let generated = read(&empty.path().join("hk.yaml"));
    assert!(generated.contains("cortex-m4f"), "导出的配置应包含内置目标");
    run_hk(empty.path(), &["init"])
        .assert_failed("重复 init 应提示已存在")
        .assert_contains("已存在");
}

#[test]
fn clean_removes_build_directories() {
    if !gcc_available() {
        eprintln!("跳过 clean_removes_build_directories：本机没有 gcc/ar");
        return;
    }
    let tmp = TempDir::new("clean");
    scaffold_project(tmp.path());
    run_hk(tmp.path(), &["build"]).assert_ok("构建");
    assert!(tmp.path().join("app/build").exists());

    run_hk(tmp.path(), &["clean", "--dry-run"])
        .assert_ok("预演清理")
        .assert_contains("将删除");
    assert!(tmp.path().join("app/build").exists(), "预演不应真的删除");

    run_hk(tmp.path(), &["clean"]).assert_ok("清理");
    assert!(!tmp.path().join("app/build").exists());
    assert!(!tmp.path().join("hal/build").exists());
}
