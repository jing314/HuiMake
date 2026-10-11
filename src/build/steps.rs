//! 单模块构建计划：源码扫描 → 编译 → 打包/链接 → 链接后处理。
//!
//! 计划是纯数据（步骤 + 命令行 + 签名），既可以直接执行，也可以在 --print-plan
//! 模式下只打印，便于在没有交叉工具链的机器上核对 MCU 编译参数。

use crate::config::{ConfigView, Kind, Naming, PostAction, Profile, Target, Toolchain};
use crate::error::{HkError, Result};
use crate::glob;
use crate::project::{ModInfo, Project};
use crate::util::{self, Cmd};
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    Compile,
    Archive,
    Link { memory_usage: bool },
    Post(PostAction),
    Size,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub module: String,
    pub action: &'static str,
    pub cmd: Cmd,
    /// 成功后写入的签名文件与内容（用于增量判断）。
    pub sig_file: Option<(PathBuf, String)>,
    /// 该步骤产出的文件（objcopy / objdump 需要）。
    pub artifact: Option<PathBuf>,
    /// 传给子进程的临时目录（执行前会创建）。
    pub temp_dir: Option<PathBuf>,
    pub kind: StepKind,
}

#[derive(Debug)]
pub struct ModulePlan {
    pub module: String,
    pub kind: Kind,
    pub has_cxx: bool,
    /// 0: 编译 1: 打包/链接 2: 链接后处理；阶段内可并行执行。
    pub phases: [Vec<Step>; 3],
    /// 已是最新、无需执行的步骤数。
    pub skipped: usize,
    /// 该模块产出的最终产物。
    pub artifacts: Vec<PathBuf>,
    /// 该模块产出的可执行文件（hk run 使用）。
    pub executables: Vec<PathBuf>,
}

impl ModulePlan {
    pub fn is_up_to_date(&self) -> bool {
        self.phases.iter().all(|p| p.is_empty())
    }

    pub fn empty(module: String, kind: Kind) -> Self {
        ModulePlan {
            module,
            kind,
            has_cxx: false,
            phases: [Vec::new(), Vec::new(), Vec::new()],
            skipped: 0,
            artifacts: Vec::new(),
            executables: Vec::new(),
        }
    }
}

pub struct PlanCtx<'a> {
    pub project: &'a Project,
    /// 工程根（用于解析 target 中配置的相对路径）。
    pub project_root: &'a Path,
    pub target_name: &'a str,
    pub target: &'a Target,
    pub profile: &'a Profile,
    pub toolchain: &'a Toolchain,
    pub naming: &'a Naming,
    pub build_dir: &'a str,
    pub force: bool,
    pub strip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    C,
    Cxx,
    Asm,
}

impl SourceKind {
    fn from_rel(rel: &str) -> Option<Self> {
        let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        match ext {
            "c" => Some(SourceKind::C),
            "cc" | "cpp" | "cxx" | "c++" | "C" => Some(SourceKind::Cxx),
            "S" | "s" | "asm" => Some(SourceKind::Asm),
            _ => None,
        }
    }

    fn is_cxx(self) -> bool {
        self == SourceKind::Cxx
    }
}

#[derive(Debug, Clone)]
struct SourceUnit {
    rel: String,
    full: PathBuf,
    obj: PathBuf,
    depfile: PathBuf,
    kind: SourceKind,
}

/// 链接时某个依赖模块贡献出的输入（归档 / 动态库 / 目标文件）。
pub fn dep_artifacts(ctx: &PlanCtx<'_>, dep: &str) -> Result<Vec<PathBuf>> {
    let info = ctx.project.module(dep)?;
    let build = info.dir.join(ctx.build_dir);
    let name = info.config.output_name().unwrap_or(&info.name).to_string();
    match info.config.kind {
        Kind::StaticLib => Ok(vec![build.join("lib").join(ctx.naming.static_lib_name(&name))]),
        Kind::SharedLib => Ok(vec![build.join("lib").join(ctx.naming.shared_lib_name(&name))]),
        Kind::Object => {
            let view = info.config.for_target(ctx.target_name);
            let units = collect_sources(ctx, info, &view, &build.join("obj"))?;
            Ok(units.into_iter().map(|u| u.obj).collect())
        }
        Kind::HeaderOnly => Ok(Vec::new()),
        Kind::Executable => Err(HkError::new(format!(
            "模块 {} 依赖了可执行模块 {dep}；可执行文件不能作为链接输入，请把它改成 static-lib",
            info.name
        ))),
    }
}

pub fn plan_module(ctx: &PlanCtx<'_>, name: &str) -> Result<ModulePlan> {
    let info = ctx.project.module(name)?;
    let kind = info.config.kind;
    let mut plan = ModulePlan::empty(name.to_string(), kind);
    if kind == Kind::HeaderOnly {
        crate::log_debug!("{name}: header-only 模块，跳过构建");
        return Ok(plan);
    }

    let build = info.dir.join(ctx.build_dir);
    let obj_root = build.join("obj");
    let temp_dir = module_temp_dir(ctx, info);
    let view = info.config.for_target(ctx.target_name);
    let units = collect_sources(ctx, info, &view, &obj_root)?;
    plan.has_cxx = units.iter().any(|u| u.kind.is_cxx());

    let closure = ctx.project.dep_closure(name)?;
    for dep in &closure {
        let dep_info = ctx.project.module(dep)?;
        if dep_info.config.kind == Kind::Executable {
            return Err(HkError::new(format!(
                "模块 {name} 依赖了可执行模块 {dep}，请把 {dep} 改为 static-lib"
            )));
        }
        if collect_cxx_flag(ctx, dep_info)? {
            plan.has_cxx = true;
        }
    }

    let (include_dirs, public_defines, public_cflags) = collect_interfaces(ctx, info, &view, &closure)?;
    let out_name = info.config.output_name().unwrap_or(&info.name).to_string();
    let post = merged_post(ctx, kind);

    let map_path = if post.contains(&PostAction::Map) && kind.links() {
        Some(output_path(ctx, kind, &build, &out_name).with_extension("map"))
    } else {
        None
    };
    let output = if kind.links() { Some(output_path(ctx, kind, &build, &out_name)) } else { None };

    let lib_dirs = resolved_lib_dirs(ctx, &info.dir, &view);
    let linker_script = resolved_linker_script(ctx, &info.dir, &view);
    let flag_input = || crate::build::flags::FlagInput {
        view: info.config.for_target(ctx.target_name),
        target: ctx.target,
        profile: ctx.profile,
        kind,
        include_dirs: &include_dirs,
        lib_dirs: lib_dirs.clone(),
        linker_script: linker_script.clone(),
        public_defines: &public_defines,
        public_cflags: &public_cflags,
        map_path: map_path.as_deref(),
        strip: ctx.strip,
    };
    let (cflags, cxxflags) = crate::build::flags::compile(&flag_input());

    // ---------- 阶段 0：编译 ----------
    let mut objects: Vec<PathBuf> = Vec::new();
    for unit in &units {
        objects.push(unit.obj.clone());
        let compiler = if unit.kind.is_cxx() { ctx.toolchain.cxx()? } else { ctx.toolchain.cc()? };
        let std_flags = if unit.kind.is_cxx() { &cxxflags } else { &cflags };
        let per_file = crate::build::flags::per_file_flags(&view, &unit.rel);
        let sig_text = format!(
            "{}|{}|{}|{}",
            compiler,
            std_flags.join(" "),
            per_file.join(" "),
            unit.rel
        );
        let mut cmd = Cmd::new(compiler).args(std_flags.iter().cloned());
        cmd = cmd.arg("-c").arg(&unit.full).arg("-o").arg(&unit.obj);
        cmd = cmd.args(["-MMD", "-MP", "-MF"]).arg(&unit.depfile);
        cmd = cmd.args(per_file);
        let sig_file = unit.obj.with_extension("sig");
        if !ctx.force
            && up_to_date(
                &unit.obj,
                &sig_file,
                &sig_text,
                std::slice::from_ref(&unit.full),
                Some(&unit.depfile),
            )
        {
            plan.skipped += 1;
            continue;
        }
        let cmd = cmd.with_response_file(&unit.obj.with_extension("rsp"))?;
        plan.phases[0].push(Step {
            module: name.to_string(),
            action: "编译",
            cmd,
            sig_file: Some((sig_file, sig_text)),
            artifact: Some(unit.obj.clone()),
            temp_dir: None,
            kind: StepKind::Compile,
        });
        crate::log_trace!("{name}: 编译 {} -> {}", unit.rel, unit.obj.display());
    }

    apply_temp_env(&mut plan, 0, &temp_dir);

    if kind == Kind::Object {
        plan.artifacts.extend(objects.iter().cloned());
        return Ok(plan);
    }

    // ---------- 阶段 1：打包 / 链接 ----------
    if kind == Kind::StaticLib {
        let lib = build.join("lib").join(ctx.naming.static_lib_name(&out_name));
        let sig_text = format!("ar rcs|{}|{}", lib.display(), out_name);
        let sig_file = lib.with_extension("sig");
        if !ctx.force && up_to_date(&lib, &sig_file, &sig_text, &objects, None) {
            plan.skipped += 1;
        } else {
            let mut cmd = Cmd::new(ctx.toolchain.ar()?).args(["rcs"]).arg(&lib).args(objects.iter().cloned());
            cmd = cmd.with_response_file(&build.join("lib").join(format!("{out_name}.rsp")))?;
            plan.phases[1].push(Step {
                module: name.to_string(),
                action: "打包",
                cmd,
                sig_file: Some((sig_file, sig_text)),
                artifact: Some(lib.clone()),
                temp_dir: None,
                kind: StepKind::Archive,
            });
        }
        apply_temp_env(&mut plan, 1, &temp_dir);
        plan.artifacts.push(lib);
        return Ok(plan);
    }

    // 可执行文件 / 动态库
    let out = output.clone().expect("链接类产物必须有输出路径");
    let link_input = flag_input();
    let mut cmd = Cmd::new(if plan.has_cxx { ctx.toolchain.cxx()? } else { ctx.toolchain.cc()? });
    cmd = cmd.args(crate::build::flags::link_pre(&link_input));
    cmd = cmd.args(objects.iter().cloned());
    cmd = cmd.args(render_link_libs(ctx, info, &view, &closure, &build)?);
    cmd = cmd.args(crate::build::flags::link_post(&link_input, &out));
    let rsp = out.with_extension("rsp");
    let sig_text = format!("link|{}", cmd.display());
    let sig_file = out.with_extension("sig");

    let mut inputs = objects.clone();
    if !ctx.force && up_to_date(&out, &sig_file, &sig_text, &inputs, None) {
        plan.skipped += 1;
    } else {
        let cmd = cmd.with_response_file(&rsp)?;
        plan.phases[1].push(Step {
            module: name.to_string(),
            action: if kind == Kind::SharedLib { "链接动态库" } else { "链接" },
            cmd,
            sig_file: Some((sig_file, sig_text)),
            artifact: Some(out.clone()),
            temp_dir: None,
            kind: StepKind::Link { memory_usage: ctx.target.memory_usage },
        });
    }
    inputs.clear();
    plan.artifacts.push(out.clone());
    if kind == Kind::Executable {
        plan.executables.push(out.clone());
    }

    // ---------- 阶段 2：链接后处理 ----------
    for action in post {
        if let Some(step) = post_step(ctx, name, action, &out)? {
            plan.phases[2].push(step);
        }
    }
    apply_temp_env(&mut plan, 1, &temp_dir);
    apply_temp_env(&mut plan, 2, &temp_dir);
    Ok(plan)
}

/// 相对路径统一解析成绝对路径，避免受 hk 调用目录影响。
fn resolve_display(base: &Path, item: &str) -> String {
    let path = Path::new(item);
    if path.is_absolute() {
        item.to_string()
    } else {
        base.join(path).to_string_lossy().to_string()
    }
}

/// -L 目录：模块配置的相对于模块目录，目标配置的相对于工程根。
fn resolved_lib_dirs(ctx: &PlanCtx<'_>, module_dir: &Path, view: &ConfigView<'_>) -> Vec<String> {
    let mut out: Vec<String> = view
        .deps()
        .lib_dirs
        .iter()
        .map(|dir| resolve_display(module_dir, dir))
        .collect();
    out.extend(ctx.target.lib_dirs.iter().map(|dir| resolve_display(ctx.project_root, dir)));
    out
}

/// 链接脚本：模块里的相对于模块目录，目标里的相对于工程根。
fn resolved_linker_script(
    ctx: &PlanCtx<'_>,
    module_dir: &Path,
    view: &ConfigView<'_>,
) -> Option<String> {
    let link = view.link();
    if let Some(script) = &link.linker_script {
        return Some(resolve_display(module_dir, script));
    }
    ctx.target
        .linker_script
        .as_ref()
        .map(|script| resolve_display(ctx.project_root, script))
}

/// 编译器用的临时目录：默认放在模块构建目录下，保证一定可写。
fn module_temp_dir(ctx: &PlanCtx<'_>, info: &ModInfo) -> PathBuf {
    match ctx.project.global.temp_dir.as_deref() {
        Some(configured) if !configured.trim().is_empty() => {
            let base = Path::new(configured);
            let root = if base.is_absolute() {
                base.to_path_buf()
            } else {
                ctx.project_root.join(base)
            };
            root.join(&info.name)
        }
        _ => info.dir.join(ctx.build_dir).join("tmp"),
    }
}

/// 把某个阶段的命令统一指向临时目录（并记录目录以便执行前创建）。
fn apply_temp_env(plan: &mut ModulePlan, phase: usize, temp: &Path) {
    for step in plan.phases[phase].iter_mut() {
        step.cmd = step.cmd.clone().temp_env(temp);
        step.temp_dir = Some(temp.to_path_buf());
    }
}

fn collect_cxx_flag(ctx: &PlanCtx<'_>, info: &ModInfo) -> Result<bool> {
    let view = info.config.for_target(ctx.target_name);
    let build = info.dir.join(ctx.build_dir).join("obj");
    Ok(collect_sources(ctx, info, &view, &build)?.iter().any(|u| u.kind.is_cxx()))
}

/// 汇总头文件目录与对外传播的宏/参数。
fn collect_interfaces(
    ctx: &PlanCtx<'_>,
    info: &ModInfo,
    view: &ConfigView<'_>,
    closure: &[String],
) -> Result<(Vec<PathBuf>, Vec<String>, Vec<String>)> {
    let mut includes: Vec<PathBuf> = Vec::new();
    let mut defines: Vec<String> = Vec::new();
    let mut cflags: Vec<String> = Vec::new();

    for rel in info.config.public_includes().iter().chain(view.include_dirs().iter()) {
        push_include(&mut includes, &info.dir, rel)?;
    }
    for dep in closure {
        let dep_info = ctx.project.module(dep)?;
        let dep_view = dep_info.config.for_target(ctx.target_name);
        for rel in dep_info
            .config
            .public_includes()
            .iter()
            .chain(dep_info.config.public.include_dirs.iter())
            .chain(dep_view.public_target_includes().iter())
        {
            push_include(&mut includes, &dep_info.dir, rel)?;
        }
        defines.extend(dep_info.config.public.defines.iter().cloned());
        cflags.extend(dep_info.config.public.cflags.iter().cloned());
    }
    defines.dedup();
    cflags.dedup();
    Ok((includes, defines, cflags))
}

fn push_include(out: &mut Vec<PathBuf>, base: &Path, rel: &str) -> Result<()> {
    let dir = base.join(rel);
    if !dir.is_dir() {
        if rel != "include" {
            crate::log_warn!("头文件目录不存在，已忽略: {}", dir.display());
        }
        return Ok(());
    }
    let dir = util::normalize(&dir).unwrap_or(dir);
    for sub in util::walk_dirs_pruned(&dir, &[])? {
        if !out.contains(&sub) {
            out.push(sub);
        }
    }
    Ok(())
}

fn collect_sources(
    ctx: &PlanCtx<'_>,
    info: &ModInfo,
    view: &ConfigView<'_>,
    obj_root: &Path,
) -> Result<Vec<SourceUnit>> {
    let patterns = view.sources();
    let excludes = view.exclude();
    let files = util::walk_files_pruned(&info.dir, &[ctx.build_dir])?;
    let mut units = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for full in files {
        let rel = util::rel_to(&info.dir, &full);
        if rel.starts_with("..") {
            continue;
        }
        if !patterns.iter().any(|p| glob::matches(p, &rel)) {
            continue;
        }
        if excludes.iter().any(|p| glob::matches(p, &rel)) {
            crate::log_debug!("{}: 排除 {}", info.name, rel);
            continue;
        }
        let kind = match SourceKind::from_rel(&rel) {
            Some(kind) => kind,
            None => continue,
        };
        let mut obj_rel = replace_extension(&rel, ctx.naming.obj_suffix.trim_start_matches('.'));
        if !used.insert(obj_rel.clone()) {
            obj_rel = format!("{obj_rel}.{}", kind_ext(kind));
            used.insert(obj_rel.clone());
        }
        units.push(SourceUnit {
            rel,
            full,
            obj: obj_root.join(&obj_rel),
            depfile: obj_root.join(format!("{obj_rel}.d")),
            kind,
        });
    }
    units.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(units)
}

fn kind_ext(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::C => "c",
        SourceKind::Cxx => "cxx",
        SourceKind::Asm => "S",
    }
}

fn replace_extension(rel: &str, new_ext: &str) -> String {
    match rel.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.{new_ext}"),
        None => format!("{rel}.{new_ext}"),
    }
}

fn output_path(ctx: &PlanCtx<'_>, kind: Kind, build: &Path, name: &str) -> PathBuf {
    match kind {
        Kind::Executable => build.join("bin").join(ctx.naming.executable_name(name)),
        Kind::SharedLib => build.join("lib").join(ctx.naming.shared_lib_name(name)),
        _ => build.join("lib").join(ctx.naming.static_lib_name(name)),
    }
}

fn merged_post(ctx: &PlanCtx<'_>, kind: Kind) -> Vec<PostAction> {
    let mut out: Vec<PostAction> = Vec::new();
    if kind != Kind::Executable {
        return out;
    }
    for action in ctx.target.post.iter().chain(ctx.profile.post.iter()) {
        if !out.contains(action) {
            out.push(*action);
        }
    }
    out
}

/// 链接输入：依赖模块产物（拓扑逆序，被依赖者放最后）+ 本模块配置的静态库/动态库/目标文件。
fn render_link_libs(
    ctx: &PlanCtx<'_>,
    info: &ModInfo,
    view: &ConfigView<'_>,
    closure: &[String],
    _build: &Path,
) -> Result<Vec<OsString>> {
    let link = view.link();
    let deps = view.deps();
    let mut args: Vec<OsString> = Vec::new();
    let mut archives: Vec<PathBuf> = Vec::new();

    // 1. 工程内模块依赖：逆拓扑序（使用者在前，被依赖者在后）
    for dep in closure.iter().rev() {
        archives.extend(dep_artifacts(ctx, dep)?);
    }

    // 2. 模块自己配置的额外目标文件 / 归档
    for item in &link.extra_objects {
        archives.push(resolve_rel(&info.dir, item));
    }
    for item in &deps.static_libs {
        if util::is_path_like(item) {
            archives.push(resolve_rel(&info.dir, item));
        }
    }

    // 3. 多个归档用 --start-group 兜住循环引用（GNU ld 特性）
    let group = !cfg!(target_os = "macos") && archives.len() > 1;
    if group {
        args.push(OsString::from("-Wl,--start-group"));
    }
    for archive in &archives {
        args.push(archive.clone().into_os_string());
    }
    if group {
        args.push(OsString::from("-Wl,--end-group"));
    }

    // 4. whole-archive
    for item in &link.whole_archive {
        args.push(OsString::from("-Wl,--whole-archive"));
        if util::is_path_like(item) {
            args.push(resolve_rel(&info.dir, item).into_os_string());
        } else {
            args.push(OsString::from(format!("-l{item}")));
        }
        args.push(OsString::from("-Wl,--no-whole-archive"));
    }

    // 5. 动态库
    for item in &deps.shared_libs {
        if util::is_path_like(item) {
            args.push(resolve_rel(&info.dir, item).into_os_string());
        } else {
            args.push(OsString::from(format!("-l{item}")));
        }
    }

    // 6. 裸名静态库（可选强制静态）
    for item in &deps.static_libs {
        if util::is_path_like(item) {
            continue;
        }
        if link.static_link {
            args.push(OsString::from("-Wl,-Bstatic"));
            args.push(OsString::from(format!("-l{item}")));
            args.push(OsString::from("-Wl,-Bdynamic"));
        } else {
            args.push(OsString::from(format!("-l{item}")));
        }
    }

    // 7. 系统库
    for item in &deps.system {
        if util::is_path_like(item) {
            args.push(resolve_rel(&info.dir, item).into_os_string());
        } else {
            args.push(OsString::from(format!("-l{item}")));
        }
    }
    Ok(args)
}

fn resolve_rel(base: &Path, item: &str) -> PathBuf {
    let path = Path::new(item);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn post_step(
    ctx: &PlanCtx<'_>,
    module: &str,
    action: PostAction,
    out: &Path,
) -> Result<Option<Step>> {
    let base = out.with_extension("");
    let make = |cmd: Cmd, artifact: Option<PathBuf>, action: &'static str, kind: StepKind| -> Step {
        Step { module: module.to_string(), action, cmd, sig_file: None, artifact, temp_dir: None, kind }
    };
    let step = match action {
        // .elf 就是链接产物本身（由 exe_suffix 决定）
        PostAction::Elf => return Ok(None),
        PostAction::Map => return Ok(None),
        PostAction::Bin => {
            let target = base.with_extension("bin");
            if is_fresh(&target, out) {
                return Ok(None);
            }
            let cmd = Cmd::new(ctx.toolchain.objcopy()?).args(["-O", "binary"]).arg(out).arg(&target);
            make(cmd, Some(target), "生成 bin", StepKind::Post(PostAction::Bin))
        }
        PostAction::Hex => {
            let target = base.with_extension("hex");
            if is_fresh(&target, out) {
                return Ok(None);
            }
            let cmd = Cmd::new(ctx.toolchain.objcopy()?).args(["-O", "ihex"]).arg(out).arg(&target);
            make(cmd, Some(target), "生成 hex", StepKind::Post(PostAction::Hex))
        }
        PostAction::Lst => {
            let target = base.with_extension("lst");
            if is_fresh(&target, out) {
                return Ok(None);
            }
            let cmd = Cmd::new(ctx.toolchain.objdump()?).arg("-d").arg(out);
            make(cmd, Some(target), "反汇编", StepKind::Post(PostAction::Lst))
        }
        PostAction::Size => {
            let cmd = Cmd::new(ctx.toolchain.size()?).arg(out);
            make(cmd, None, "size", StepKind::Size)
        }
    };
    Ok(Some(step))
}

fn is_fresh(artifact: &Path, input: &Path) -> bool {
    match (util::mtime(artifact), util::mtime(input)) {
        (Some(a), Some(b)) => a >= b,
        _ => false,
    }
}

/// 增量判断：产物存在、命令行签名未变、所有输入都不比产物新。
pub fn up_to_date(
    out: &Path,
    sig_file: &Path,
    sig_text: &str,
    inputs: &[PathBuf],
    depfile: Option<&Path>,
) -> bool {
    let out_time = match util::mtime(out) {
        Some(time) => time,
        None => return false,
    };
    match std::fs::read_to_string(sig_file) {
        Ok(old) if old == sig_text => {}
        _ => return false,
    }
    for input in inputs {
        match util::mtime(input) {
            Some(time) if time <= out_time => {}
            _ => return false,
        }
    }
    if let Some(depfile) = depfile {
        for dep in read_depfile(depfile) {
            match util::mtime(&dep) {
                // 依赖的头部被删除时也重编，交给编译器报出真实错误
                Some(time) if time <= out_time => {}
                _ => return false,
            }
        }
    }
    true
}

/// 解析 gcc -MMD 生成的依赖文件。
pub fn read_depfile(path: &Path) -> Vec<PathBuf> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let joined = text.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut out = Vec::new();
    for line in joined.lines() {
        let start = if line.len() > 1 && line.as_bytes()[1] == b':' { 2 } else { 0 };
        let rest = match line[start..].find(": ") {
            Some(idx) => &line[start + idx + 2..],
            None => continue,
        };
        for token in rest.split_whitespace() {
            let token = token.trim_end_matches('\\');
            if !token.is_empty() {
                out.push(PathBuf::from(token));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hk-build-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ctx<'a>(
        project: &'a Project,
        target: &'a Target,
        profile: &'a Profile,
        toolchain: &'a Toolchain,
        naming: &'a Naming,
        build_dir: &'a str,
    ) -> PlanCtx<'a> {
        PlanCtx {
            project,
            project_root: &project.root,
            target_name: "host",
            target,
            profile,
            toolchain,
            naming,
            build_dir,
            force: true,
            strip: false,
        }
    }

    #[test]
    fn depfile_parsing_handles_line_continuations_and_windows_drive() {
        let dir = temp_dir("depfile");
        let dep = dir.join("a.d");
        write(
            &dep,
            "build/obj/main.o: C:\\proj\\src\\main.c \\\n  C:\\proj\\include\\a.h \\\n  C:\\proj\\include\\b.h\n",
        );
        let parsed = read_depfile(&dep);
        assert_eq!(parsed.len(), 3, "{parsed:?}");
        assert_eq!(parsed[0], PathBuf::from("C:\\proj\\src\\main.c"));
        assert_eq!(parsed[2], PathBuf::from("C:\\proj\\include\\b.h"));
    }

    #[test]
    fn link_order_puts_dependents_before_providers() {
        let root = temp_dir("linkorder");
        write(&root.join("app/config.yaml"), "kind: executable\nsources: [bin/**/*.c]\ndeps:\n  mods: [drv]\n");
        write(&root.join("app/bin/main.c"), "int main(void){return 0;}\n");
        write(&root.join("drv/config.yaml"), "kind: static-lib\nsources: [src/**/*.c]\ndeps:\n  mods: [hal]\n");
        write(&root.join("drv/src/d.c"), "int d(void){return 0;}\n");
        write(&root.join("hal/config.yaml"), "kind: static-lib\nsources: [src/**/*.c]\n");
        write(&root.join("hal/src/h.c"), "int h(void){return 0;}\n");

        let project = Project::discover(&root).unwrap();
        let global = crate::config::GlobalConfig::builtin().unwrap();
        let target = global.target("host").unwrap();
        let profile = global.profile("debug").unwrap();
        let toolchain = global.toolchain("host").unwrap();
        let naming = target.naming(toolchain);
        let c = ctx(&project, target, profile, toolchain, &naming, "build");
        let plan = plan_module(&c, "app").unwrap();
        let link = plan.phases[1].iter().find(|s| matches!(s.kind, StepKind::Link { .. })).unwrap();
        let args: Vec<String> = link.cmd.args.iter().map(|a| a.to_string_lossy().to_string()).collect();
        let drv = args.iter().position(|a| a.ends_with("libdrv.a")).expect("应链接 libdrv.a");
        let hal = args.iter().position(|a| a.ends_with("libhal.a")).expect("应链接 libhal.a");
        assert!(drv < hal, "被依赖者应排在后面: {args:?}");
        assert!(args.iter().any(|a| a == "-Wl,--start-group"));
    }

    #[test]
    fn mcu_post_steps_are_generated() {
        let root = temp_dir("mcu");
        write(
            &root.join("fw/config.yaml"),
            "name: fw\nkind: executable\nsources: [src/**/*.c]\ninclude: []\n",
        );
        write(&root.join("fw/src/main.c"), "int main(void){return 0;}\n");
        let project = Project::discover(&root).unwrap();
        let global = crate::config::GlobalConfig::builtin().unwrap();
        let target = global.target("cortex-m4f").unwrap();
        let profile = global.profile("mcu-release").unwrap();
        let toolchain = global.toolchain("arm-none-eabi").unwrap();
        let naming = target.naming(toolchain);
        let c = ctx(&project, target, profile, toolchain, &naming, "build");
        let plan = plan_module(&c, "fw").unwrap();
        let module_dir = util::normalize(&root.join("fw")).unwrap();
        assert_eq!(plan.artifacts[0], module_dir.join("build").join("bin").join("fw.elf"));
        let actions: Vec<String> = plan.phases[2].iter().map(|s| s.action.to_string()).collect();
        assert!(actions.contains(&"生成 bin".to_string()), "{actions:?}");
        assert!(actions.contains(&"生成 hex".to_string()), "{actions:?}");
        assert!(actions.contains(&"size".to_string()), "{actions:?}");
        let compile = &plan.phases[0][0];
        let args: Vec<String> = compile.cmd.args.iter().map(|a| a.to_string_lossy().to_string()).collect();
        assert!(args.contains(&"-mcpu=cortex-m4".to_string()));
        assert!(args.contains(&"-Os".to_string()));
        assert_eq!(compile.cmd.program, "arm-none-eabi-gcc");
    }
}
