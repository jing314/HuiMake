//! 命令实现：build / run / clean / new / init / config。

use crate::build::{self, BuildOptions};
use crate::cli::{BuildArgs, CleanArgs, Cli, Command, ConfigArgs, InitArgs, NewArgs, RunArgs};
use crate::config::{GlobalConfig, Kind, MODULE_FILE_NAME};
use crate::error::{HkError, Result};
use crate::log;
use crate::project::Project;
use crate::util::{self, Cmd};
use std::collections::HashSet;
use std::path::PathBuf;

pub fn run(cli: Cli) -> Result<i32> {
    set_log_level(&cli);
    match &cli.command {
        Some(Command::Build(args)) => cmd_build(args),
        Some(Command::Run(args)) => cmd_run(args),
        Some(Command::Clean(args)) => cmd_clean(args),
        Some(Command::New(args)) => cmd_new(args),
        Some(Command::Init(args)) => cmd_init(args),
        Some(Command::Config(args)) => cmd_config(args),
        None => {
            print_help();
            Ok(0)
        }
    }
}

fn set_log_level(cli: &Cli) {
    let level = if cli.quiet {
        log::ERROR
    } else {
        match cli.verbose {
            0 => log::INFO,
            1 => log::DEBUG,
            _ => log::TRACE,
        }
    };
    log::set_level(level);
}

fn print_help() {
    use clap::CommandFactory;
    let mut command = Cli::command();
    let _ = command.print_help();
    println!();
}

fn current_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|e| HkError::with_source("获取当前目录失败", e))
}

fn open_project() -> Result<Project> {
    Project::discover(&current_dir()?)
}

/// 未显式指定时：在模块目录内只构建该模块，在工程根构建全部。
fn effective_scope(project: &Project, explicit: Option<&str>) -> Option<String> {
    match explicit {
        Some(name) => Some(name.to_string()),
        None => project.current.clone(),
    }
}

fn make_options(args: &BuildArgs, project: &Project) -> BuildOptions {
    BuildOptions {
        mode: args.mode.clone(),
        target: args.target.clone(),
        jobs: args.jobs,
        print_plan: args.print_plan,
        force: args.force,
        keep_going: args.keep_going,
        scope: effective_scope(project, args.module.as_deref()),
        plan_output: args.output.clone(),
        run_args: Vec::new(),
        run_bin: None,
    }
}

/// 先校验 -m/-t 是否合法，再去做工程发现；
/// 否则 -m 后面误写成目录这类问题会被"模块重名"之类的错误掩盖。
fn precheck_mode_and_target(args: &BuildArgs) -> Result<()> {
    if args.mode.is_none() && args.target.is_none() {
        return Ok(());
    }
    let cwd = current_dir()?;
    let root = GlobalConfig::find_root(&cwd);
    let global = GlobalConfig::load(root.as_deref())?;
    if let Some(mode) = &args.mode {
        global.profile(mode)?;
    }
    if let Some(target) = &args.target {
        global.target(target)?;
    }
    Ok(())
}

fn cmd_build(args: &BuildArgs) -> Result<i32> {
    precheck_mode_and_target(args)?;
    let project = open_project()?;
    crate::log_debug!(
        "工程根 {}，模块 [{}]",
        project.root.display(),
        project.module_names()
    );
    match &project.global_file {
        Some(file) => crate::log_debug!("工程配置 {}", file.display()),
        None => crate::log_debug!("未找到 hk.yaml，使用内置默认配置"),
    }
    let opts = make_options(args, &project);
    if let Some(scope) = &opts.scope {
        project.module(scope)?;
    }
    build::build(&project, &opts)?;
    Ok(0)
}

fn cmd_run(args: &RunArgs) -> Result<i32> {
    precheck_mode_and_target(&args.build)?;
    let project = open_project()?;
    let mut opts = make_options(&args.build, &project);
    opts.run_bin = args.bin.clone();
    opts.run_args = args.args.clone();

    let settings = build::resolve(&project, &opts)?;
    if !settings.target.runnable {
        return Err(HkError::new(format!(
            "目标 {:?} 的产物无法在本机运行（hk.yaml 中 runnable: false）\
             —— MCU 目标请用生成的 bin/hex 文件烧录",
            settings.target_name
        )));
    }
    let report = build::build(&project, &opts)?;

    let wanted: HashSet<String> = match &opts.scope {
        Some(name) => [name.clone()].into_iter().collect(),
        None => report.plans.iter().map(|p| p.module.clone()).collect(),
    };
    let mut targets: Vec<(String, PathBuf)> = Vec::new();
    for plan in &report.plans {
        if !wanted.contains(&plan.module) {
            continue;
        }
        for exe in &plan.executables {
            targets.push((plan.module.clone(), exe.clone()));
        }
    }
    if let Some(bin) = &args.bin {
        targets.retain(|(module, exe)| {
            module == bin
                || exe.file_stem().map(|s| s == bin.as_str()).unwrap_or(false)
                || exe.file_name().map(|s| s.to_string_lossy() == *bin).unwrap_or(false)
        });
    }
    if targets.is_empty() {
        return Err(HkError::new(
            "没有可运行的可执行文件：模块 kind 需要是 executable，或用 --bin 指定",
        ));
    }

    for (module, exe) in targets {
        let dir = project.module(&module)?.dir.clone();
        crate::log_plain!("==> 运行 {}（工作目录 {}）", util::short_path(&exe), util::short_path(&dir));
        let cmd = Cmd::new(exe.to_string_lossy().to_string())
            .args(args.args.clone())
            .cwd(dir);
        match cmd.run_inherit()? {
            Some(0) => {}
            Some(code) => {
                crate::log_error!("{} 退出码 {}", module, code);
                return Ok(code);
            }
            None => return Err(HkError::new(format!("{module} 被信号终止"))),
        }
    }
    Ok(0)
}

fn cmd_clean(args: &CleanArgs) -> Result<i32> {
    let project = open_project()?;
    let names: Vec<String> = if args.all {
        project.modules.keys().cloned().collect()
    } else {
        match effective_scope(&project, args.module.as_deref()) {
            Some(name) => vec![name],
            None => project.modules.keys().cloned().collect(),
        }
    };

    let mut removed = 0;
    for name in names {
        let module = project.module(&name)?;
        let dir = module.dir.join(&project.global.build_dir);
        if !dir.exists() {
            continue;
        }
        if args.dry_run {
            crate::log_plain!("将删除 {}", dir.display());
            continue;
        }
        util::remove_path(&dir)?;
        crate::log_debug!("已清理 {}", dir.display());
        removed += 1;
    }
    crate::log_plain!("清理完成：{removed} 个模块的 {} 目录", project.global.build_dir);
    Ok(0)
}

fn cmd_new(args: &NewArgs) -> Result<i32> {
    let name = args.name.trim();
    if name.is_empty() {
        return Err(HkError::new("模块名不能为空"));
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(HkError::new(format!("模块名 {name:?} 不合法（不能包含路径分隔符）")));
    }
    let parent = match &args.dir {
        Some(dir) => dir.clone(),
        None => current_dir()?,
    };
    let dir = parent.join(name);
    if dir.exists() && !args.force {
        return Err(HkError::new(format!(
            "目录 {} 已存在（加 --force 覆盖其中的模板文件）",
            dir.display()
        )));
    }
    util::ensure_dir(&dir)?;

    let kind: Kind = args.kind.into();
    util::write_if_changed(&dir.join(MODULE_FILE_NAME), &module_template(name, kind))?;

    match kind {
        Kind::Executable => {
            util::write_if_changed(
                &dir.join("bin").join("main.c"),
                "#include <stdio.h>\n\nint main(void)\n{\n    printf(\"hello from HK\\n\");\n    return 0;\n}\n",
            )?;
        }
        Kind::HeaderOnly => {
            util::write_if_changed(
                &dir.join("include").join(format!("{name}.h")),
                &format!("#ifndef {upper}_H\n#define {upper}_H\n\n/* TODO: 声明接口 */\n\n#endif\n", upper = name.to_uppercase()),
            )?;
        }
        _ => {
            util::write_if_changed(
                &dir.join("include").join(format!("{name}.h")),
                &format!(
                    "#ifndef {upper}_H\n#define {upper}_H\n\nint {name}_add(int a, int b);\n\n#endif\n",
                    upper = name.to_uppercase()
                ),
            )?;
            util::write_if_changed(
                &dir.join("src").join(format!("{name}.c")),
                &format!("#include \"{name}.h\"\n\nint {name}_add(int a, int b)\n{{\n    return a + b;\n}}\n"),
            )?;
        }
    }
    util::write_if_changed(&dir.join(".gitignore"), "/build\n")?;

    crate::log_plain!("已创建模块 {}（{}）", util::short_path(&dir), kind.as_str());
    crate::log_plain!("下一步：编辑 {}/{}，然后运行 hk build", name, MODULE_FILE_NAME);
    Ok(0)
}

fn module_template(name: &str, kind: Kind) -> String {
    format!(
        "# 模块配置（HuiMake）\n\
         # 完整的字段说明见 hk config 导出结果，或 doc/configuration.md\n\
         name: {name}\n\
         kind: {kind}\n\
         \n\
         # 语言标准（不写则用编译器默认值）\n\
         # std: c11\n\
         # cxx_std: c++17\n\
         \n\
         # 源码 glob（相对本目录）；不写则默认扫描 src/ 与 bin/ 下的 .c/.cc/.cpp/.S\n\
         # sources:\n\
         #   - src/**/*.c\n\
         # exclude:\n\
         #   - src/legacy/**\n\
         \n\
         # 头文件目录（默认 include/），可加多个\n\
         # include:\n\
         #   - include\n\
         #   - api\n\
         \n\
         # 全局宏声明\n\
         # defines:\n\
         #   - VERSION=\"1.0.0\"\n\
         #   - ENABLE_LOG\n\
         \n\
         # 依赖\n\
         # deps:\n\
         #   mods: [other_module]      # 工程内模块\n\
         #   system: [pthread, m]      # -lxxx\n\
         #   static: [third_party/libfoo.a]\n\
         #   shared: [z]\n\
         #   lib_dirs: [third_party/lib]\n\
         \n\
         # 交叉编译时按目标覆盖（目标名来自 hk.yaml 的 targets）\n\
         # targets:\n\
         #   cortex-m4f:\n\
         #     defines: [STM32F407xx, USE_HAL_DRIVER]\n\
         #     sources: [startup/**/*.S]\n\
         #     link:\n\
         #       linker_script: link/fw.ld\n",
        kind = kind.as_str()
    )
}

fn cmd_init(args: &InitArgs) -> Result<i32> {
    let cwd = current_dir()?;
    let file = args.output.clone().unwrap_or_else(|| PathBuf::from("hk.yaml"));
    let path = if file.is_absolute() { file } else { cwd.join(file) };
    if path.exists() && !args.force {
        return Err(HkError::new(format!(
            "{} 已存在（加 --force 覆盖）",
            path.display()
        )));
    }
    let global = GlobalConfig::builtin()?;
    util::write_if_changed(&path, &global.dump()?)?;
    crate::log_plain!("已生成 {}", util::short_path(&path));
    crate::log_plain!("现在可以直接编辑该文件配置工具链（toolchains）、交叉编译目标（targets）与构建模式（profiles）");
    Ok(0)
}

fn cmd_config(args: &ConfigArgs) -> Result<i32> {
    let cwd = current_dir()?;
    let root = GlobalConfig::find_root(&cwd);
    let global = GlobalConfig::load(root.as_deref())?;

    if let Some(target_name) = &args.target {
        let target = global.target(target_name)?;
        let toolchain = global.toolchain(&target.toolchain)?;
        let naming = target.naming(toolchain);
        let mode = args.mode.clone().unwrap_or_else(|| global.default_mode.clone());
        let profile = global.profile(&mode)?;
        println!("# 目标 {target_name}（工具链 {}）", target.toolchain);
        println!("cc            = {}", toolchain.cc.clone().unwrap_or_default());
        println!("cxx           = {}", toolchain.cxx.clone().unwrap_or_default());
        println!("ar            = {}", toolchain.ar.clone().unwrap_or_default());
        println!("objcopy       = {}", toolchain.objcopy.clone().unwrap_or_default());
        println!("size          = {}", toolchain.size.clone().unwrap_or_default());
        println!("可本机运行    = {}", target.runnable);
        println!("静态库命名    = {}", naming.static_lib_name("NAME"));
        println!("动态库命名    = {}", naming.shared_lib_name("NAME"));
        println!("可执行命名    = {}", naming.executable_name("NAME"));
        println!("defines       = {}", target.defines.join(" "));
        println!("cflags        = {}", target.cflags.join(" "));
        println!("ldflags       = {}", target.ldflags.join(" "));
        println!("specs         = {}", target.specs.join(" "));
        println!("libs          = {}", target.libs.join(" "));
        println!("lib_dirs      = {}", target.lib_dirs.join(" "));
        println!("sysroot       = {}", target.sysroot.clone().unwrap_or_default());
        println!("linker_script = {}", target.linker_script.clone().unwrap_or_default());
        println!("entry         = {}", target.entry.clone().unwrap_or_default());
        println!("post          = {:?}", target.post);
        println!("memory_usage  = {}", target.memory_usage);
        println!("# 模式 {mode}（用 -m 指定其他配置档）");
        println!("profile cflags = {}", profile.cflags.join(" "));
        println!("profile ldflags= {}", profile.ldflags.join(" "));
        println!("strip          = {}", profile.strip || target.strip);
        return Ok(0);
    }

    let text = global.dump()?;
    match &args.output {
        Some(path) => {
            util::write_if_changed(path, &text)?;
            crate::log_plain!("已写入 {}", util::short_path(path));
        }
        None => print!("{text}"),
    }
    Ok(0)
}
