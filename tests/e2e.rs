//! End-to-end tests: invoke the real hk binary and build with the host gcc / binutils.
//!
//! Tests that need gcc skip themselves (with a note) when no toolchain is present, so
//! `cargo test` still passes on a machine without a compiler.

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
        assert!(self.ok, "{what} should succeed; output:\n{}", self.all());
        self
    }

    fn assert_failed(&self, what: &str) -> &Self {
        assert!(!self.ok, "{what} should fail; output:\n{}", self.all());
        self
    }

    fn assert_contains(&self, needle: &str) -> &Self {
        assert!(
            self.all().contains(needle),
            "output is missing {needle:?}:\n{}",
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
        .expect("failed to run hk");
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

/// Create a two-module project: hal (static lib) + app (executable).
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
        "#include <stdio.h>\n#include \"hal.h\"\n\n#ifndef HAL_ENABLED\n#error \"public.defines was not propagated to the dependent\"\n#endif\n\nint main(void)\n{\n    printf(\"hal=%d tag=%s\\n\", hal_value(), APP_TAG);\n    return 0;\n}\n",
    );
}

#[test]
fn native_project_builds_links_and_runs() {
    if !gcc_available() {
        eprintln!("skipping native_project_builds_links_and_runs: no gcc/ar on this host");
        return;
    }
    let tmp = TempDir::new("native");
    scaffold_project(tmp.path());

    run_hk(tmp.path(), &["build"]).assert_ok("hk build").assert_contains("Build finished");

    let exe = tmp.path().join("app/build/bin/app.exe");
    let exe = if exe.exists() { exe } else { tmp.path().join("app/build/bin/app") };
    assert!(exe.exists(), "executable was not produced: {}", exe.display());
    let lib = tmp.path().join("hal/build/lib/libhal.a");
    assert!(lib.exists(), "static library was not produced: {}", lib.display());

    let out = Command::new(&exe).output().expect("run the built artifact");
    assert!(out.status.success(), "the artifact exited with a non-zero code");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hal=42 tag=v1");

    // hk run must work as well
    run_hk(tmp.path().join("app").as_path(), &["run"])
        .assert_ok("hk run")
        .assert_contains("hal=42 tag=v1");
}

#[test]
fn incremental_build_skips_unchanged_and_rebuilds_on_change() {
    if !gcc_available() {
        eprintln!("skipping incremental_build_skips_unchanged_and_rebuilds_on_change: no gcc/ar on this host");
        return;
    }
    let tmp = TempDir::new("incremental");
    scaffold_project(tmp.path());

    run_hk(tmp.path(), &["build"]).assert_ok("first build");
    let app_obj = tmp.path().join("app/build/obj/bin/main.o");
    let first_mtime = fs::metadata(&app_obj).expect("app object file").modified().unwrap();

    let second = run_hk(tmp.path(), &["build"]);
    second.assert_ok("second build").assert_contains("0 step(s) executed");

    // Touching hal's implementation: only hal recompiles, app must not
    let hal_src = tmp.path().join("hal/src/hal.c");
    let mut text = read(&hal_src);
    text.push_str("\nint hal_extra(void)\n{\n    return 1;\n}\n");
    write(&hal_src, &text);
    let third = run_hk(tmp.path(), &["build"]);
    third.assert_ok("build after changing the implementation").assert_contains("hal: compiling 1 file(s)");
    assert!(!third.all().contains("app: compiling"), "app must not be recompiled:\n{}", third.all());

    // Touching a header app depends on indirectly: app must recompile (depfile works)
    let hal_h = tmp.path().join("hal/include/hal.h");
    let mut header = read(&hal_h);
    header.push_str("\n/* touched */\n");
    write(&hal_h, &header);
    let fourth = run_hk(tmp.path(), &["build"]);
    fourth.assert_ok("build after changing the header").assert_contains("app: compiling 1 file(s)");
    let new_mtime = fs::metadata(&app_obj).unwrap().modified().unwrap();
    assert!(new_mtime > first_mtime, "the app object file must be regenerated");

    // --force must rebuild everything
    let forced = run_hk(tmp.path(), &["build", "--force"]);
    forced
        .assert_ok("forced rebuild")
        .assert_contains("app: compiling 1 file(s)")
        .assert_contains("hal: compiling 1 file(s)");
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

    // Plan mode must not produce any build output
    assert!(!tmp.path().join("app/build").exists(), "print-plan must not compile anything");

    // The plan can be written to a file
    run_hk(
        tmp.path(),
        &["build", "-t", "cortex-m4f", "--print-plan", "--output", "plan.txt"],
    )
    .assert_ok("plan written to disk");
    assert!(read(&tmp.path().join("plan.txt")).contains("arm-none-eabi-gcc"));
}

#[test]
fn mcu_post_pipeline_produces_binary_hex_map_and_size() {
    if !gcc_available() || !has_tool("objcopy") || !has_tool("size") {
        eprintln!("skipping mcu_post_pipeline...: no gcc/objcopy/size on this host");
        return;
    }
    let tmp = TempDir::new("mcu");
    scaffold_project(tmp.path());

    let out = run_hk(tmp.path(), &["build", "-t", "sim", "-m", "mcu-release"]);
    out.assert_ok("MCU target build").assert_contains("section sizes");

    let bin_dir = tmp.path().join("app/build/bin");
    for name in ["app.elf", "app.bin", "app.hex", "app.map"] {
        assert!(bin_dir.join(name).exists(), "missing artifact {name} (directory content: {:?})", fs::read_dir(&bin_dir).map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name())).collect::<Vec<_>>()));
    }
    let bin = fs::read(bin_dir.join("app.bin")).unwrap();
    assert!(!bin.is_empty(), "the bin file must not be empty");
}

#[test]
fn run_refuses_targets_that_cannot_execute_locally() {
    let tmp = TempDir::new("runnoguard");
    scaffold_project(tmp.path());
    run_hk(tmp.path(), &["run", "-t", "sim"])
        .assert_failed("an MCU target must not be run")
        .assert_contains("cannot run on this host");
}

#[test]
fn new_module_scaffold_builds_out_of_the_box() {
    if !gcc_available() {
        eprintln!("skipping new_module_scaffold_builds_out_of_the_box: no gcc/ar on this host");
        return;
    }
    let tmp = TempDir::new("newmod");
    run_hk(tmp.path(), &["new", "demo"]).assert_ok("hk new");

    let config = read(&tmp.path().join("demo/config.yaml"));
    assert!(config.contains("name: demo"), "the generated config must use the module name: {config}");

    let demo = tmp.path().join("demo");
    run_hk(&demo, &["build"]).assert_ok("hk build (new module)");
    run_hk(&demo, &["run"])
        .assert_ok("hk run (new module)")
        .assert_contains("hello from HK");

    // static library template
    run_hk(tmp.path(), &["new", "mylib", "--kind", "static-lib"]).assert_ok("hk new --kind static-lib");
    run_hk(&tmp.path().join("mylib"), &["build"]).assert_ok("build static library module");
    assert!(tmp.path().join("mylib/build/lib/libmylib.a").exists());
}

#[test]
fn configuration_errors_are_actionable() {
    let tmp = TempDir::new("errors");
    scaffold_project(tmp.path());

    run_hk(tmp.path(), &["build", "-t", "no-such-target"])
        .assert_failed("an unknown target must fail")
        .assert_contains("unknown target")
        .assert_contains("cortex-m4f");

    run_hk(tmp.path(), &["build", "-m", "no-such-mode"])
        .assert_failed("an unknown mode must fail")
        .assert_contains("unknown mode")
        .assert_contains("release");

    // A wrong dependency must fail loudly, not be ignored
    write(
        &tmp.path().join("app/config.yaml"),
        "name: app\ndeps:\n  mods: [ghost]\n",
    );
    run_hk(tmp.path(), &["build"])
        .assert_failed("an unknown dependency must fail")
        .assert_contains("ghost");

    // Unknown fields must fail (a typo may not silently take effect)
    write(
        &tmp.path().join("app/config.yaml"),
        "name: app\nkindd: executable\n",
    );
    run_hk(tmp.path(), &["build"])
        .assert_failed("an unknown field must fail")
        .assert_contains("kindd");
}

#[test]
fn config_and_init_export_yaml() {
    let tmp = TempDir::new("config");
    scaffold_project(tmp.path());

    let out = run_hk(tmp.path(), &["config", "--target", "cortex-m4f"]);
    out.assert_ok("hk config --target")
        .assert_contains("arm-none-eabi-gcc")
        .assert_contains("runnable      = false");

    let dump = run_hk(tmp.path(), &["config"]);
    dump.assert_ok("hk config").assert_contains("toolchains:");

    let empty = TempDir::new("init");
    run_hk(empty.path(), &["init"]).assert_ok("hk init");
    let generated = read(&empty.path().join("hk.yaml"));
    assert!(generated.contains("cortex-m4f"), "the exported config must contain the built-in targets");
    run_hk(empty.path(), &["init"])
        .assert_failed("a second init must report that the file exists")
        .assert_contains("already exists");
}

#[test]
fn clean_removes_build_directories() {
    if !gcc_available() {
        eprintln!("skipping clean_removes_build_directories: no gcc/ar on this host");
        return;
    }
    let tmp = TempDir::new("clean");
    scaffold_project(tmp.path());
    run_hk(tmp.path(), &["build"]).assert_ok("build");
    assert!(tmp.path().join("app/build").exists());

    run_hk(tmp.path(), &["clean", "--dry-run"])
        .assert_ok("dry-run clean")
        .assert_contains("would remove");
    assert!(tmp.path().join("app/build").exists(), "a dry run must not delete anything");

    run_hk(tmp.path(), &["clean"]).assert_ok("clean");
    assert!(!tmp.path().join("app/build").exists());
    assert!(!tmp.path().join("hal/build").exists());
}
