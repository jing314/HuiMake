//! Command implementations: build / run / clean / new / init / config.

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

/// Console level: -q lowers to errors only, -v raises to debug, -vv to trace.
/// Without flags the compiled-in default is used (INFO for debug builds of hk,
/// WARN for release builds, so a shipped hk stays quiet).
fn set_log_level(cli: &Cli) {
    let level = if cli.quiet {
        log::ERROR
    } else {
        match cli.verbose {
            0 => log::DEFAULT_LEVEL,
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
    std::env::current_dir().map_err(|e| HkError::with_source("failed to get the current directory", e))
}

fn open_project() -> Result<Project> {
    Project::discover(&current_dir()?)
}

/// Without an explicit --module: inside a module directory only that module is
/// built, at the project root everything is built.
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

/// Validate -m/-t before scanning for modules. Otherwise a mistake such as
/// "-m .\01-single-module\" gets buried under unrelated errors like
/// "duplicate module name".
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
        "project root {}, modules [{}]",
        project.root.display(),
        project.module_names()
    );
    match &project.global_file {
        Some(file) => crate::log_debug!("project config {}", file.display()),
        None => crate::log_debug!("no hk.yaml found, using the built-in defaults"),
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
            "target {:?} cannot run on this host (runnable: false in hk.yaml) — \
             flash the generated .bin/.hex onto the MCU instead",
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
            "no runnable executable found: the module kind must be executable, or select one with --bin",
        ));
    }

    for (module, exe) in targets {
        let dir = project.module(&module)?.dir.clone();
        crate::log_info!("running {} (cwd {})", util::short_path(&exe), util::short_path(&dir));
        let cmd = Cmd::new(exe.to_string_lossy().to_string())
            .args(args.args.clone())
            .cwd(dir);
        match cmd.run_inherit()? {
            Some(0) => {}
            Some(code) => {
                crate::log_error!("{module} exited with code {code}");
                return Ok(code);
            }
            None => return Err(HkError::new(format!("{module} was terminated by a signal"))),
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
            crate::log_plain!("would remove {}", dir.display());
            continue;
        }
        util::remove_path(&dir)?;
        crate::log_debug!("removed {}", dir.display());
        removed += 1;
    }
    crate::log_plain!(
        "Clean finished: removed the {} directory of {removed} module(s)",
        project.global.build_dir
    );
    Ok(0)
}

fn cmd_new(args: &NewArgs) -> Result<i32> {
    let name = args.name.trim();
    if name.is_empty() {
        return Err(HkError::new("module name must not be empty"));
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(HkError::new(format!(
            "invalid module name {name:?} (must not contain path separators)"
        )));
    }
    let parent = match &args.dir {
        Some(dir) => dir.clone(),
        None => current_dir()?,
    };
    let dir = parent.join(name);
    if dir.exists() && !args.force {
        return Err(HkError::new(format!(
            "directory {} already exists (use --force to overwrite the template files)",
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
                &format!("#ifndef {upper}_H\n#define {upper}_H\n\n/* TODO: declare the interface */\n\n#endif\n", upper = name.to_uppercase()),
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

    crate::log_plain!("Created module {} ({})", util::short_path(&dir), kind.as_str());
    crate::log_plain!("Next: edit {name}/{MODULE_FILE_NAME}, then run hk build");
    Ok(0)
}

fn module_template(name: &str, kind: Kind) -> String {
    format!(
        "# Module configuration (HuiMake)\n\
         # Full field reference: run \"hk config\" or see doc/configuration.md\n\
         name: {name}\n\
         kind: {kind}\n\
         \n\
         # Language standard (omit to use the compiler default)\n\
         # std: c11\n\
         # cxx_std: c++17\n\
         \n\
         # Source globs, relative to this directory.\n\
         # Omit to scan src/ and bin/ for .c/.cc/.cpp/.S/.s\n\
         # sources:\n\
         #   - src/**/*.c\n\
         # exclude:\n\
         #   - src/legacy/**\n\
         \n\
         # Header directories (default: include/), every subdirectory is added to -I\n\
         # include:\n\
         #   - include\n\
         #   - api\n\
         \n\
         # Global macros\n\
         # defines:\n\
         #   - VERSION=\"1.0.0\"\n\
         #   - ENABLE_LOG\n\
         \n\
         # Dependencies\n\
         # deps:\n\
         #   mods: [other_module]         # modules of this project\n\
         #   system: [pthread, m]         # -lxxx\n\
         #   static: [third_party/libfoo.a]\n\
         #   shared: [z]\n\
         #   lib_dirs: [third_party/lib]\n\
         \n\
         # Per-target overrides (target names come from hk.yaml)\n\
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
            "{} already exists (use --force to overwrite)",
            path.display()
        )));
    }
    let global = GlobalConfig::builtin()?;
    util::write_if_changed(&path, &global.dump()?)?;
    crate::log_plain!("Wrote {}", util::short_path(&path));
    crate::log_plain!(
        "Edit it to configure toolchains, cross-compilation targets and build modes (profiles)"
    );
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
        println!("# target {target_name} (toolchain {})", target.toolchain);
        println!("cc            = {}", toolchain.cc.clone().unwrap_or_default());
        println!("cxx           = {}", toolchain.cxx.clone().unwrap_or_default());
        println!("ar            = {}", toolchain.ar.clone().unwrap_or_default());
        println!("objcopy       = {}", toolchain.objcopy.clone().unwrap_or_default());
        println!("objdump       = {}", toolchain.objdump.clone().unwrap_or_default());
        println!("size          = {}", toolchain.size.clone().unwrap_or_default());
        println!("runnable      = {}", target.runnable);
        println!("static lib    = {}", naming.static_lib_name("NAME"));
        println!("shared lib    = {}", naming.shared_lib_name("NAME"));
        println!("executable    = {}", naming.executable_name("NAME"));
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
        println!("# mode {mode} (use -m to pick another profile)");
        println!("profile cflags = {}", profile.cflags.join(" "));
        println!("profile ldflags= {}", profile.ldflags.join(" "));
        println!("strip          = {}", profile.strip || target.strip);
        return Ok(0);
    }

    let text = global.dump()?;
    match &args.output {
        Some(path) => {
            util::write_if_changed(path, &text)?;
            crate::log_plain!("Wrote {}", util::short_path(path));
        }
        None => print!("{text}"),
    }
    Ok(0)
}
