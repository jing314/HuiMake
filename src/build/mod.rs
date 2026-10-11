//! Build orchestration: resolve the config, plan, execute level by level in parallel,
//! then report.

pub mod flags;
pub mod steps;

use crate::config::{Naming, PostAction, Profile, Target, Toolchain};
use crate::error::{HkError, Result};
use crate::project::Project;
use crate::util;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use steps::{ModulePlan, PlanCtx, Step, StepKind};

#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Build mode (a profile name).
    pub mode: Option<String>,
    /// Target name.
    pub target: Option<String>,
    /// Parallel jobs; 0 or None means auto.
    pub jobs: Option<usize>,
    /// Print the commands instead of running them.
    pub print_plan: bool,
    /// Ignore incremental state and rebuild everything.
    pub force: bool,
    /// Keep building independent modules after a failure.
    pub keep_going: bool,
    /// Only build this module and its dependencies.
    pub scope: Option<String>,
    /// Plan output file (--print-plan --output).
    pub plan_output: Option<PathBuf>,
    /// Arguments `hk run` passes to the program.
    pub run_args: Vec<String>,
    /// `hk run` only runs this module/executable.
    pub run_bin: Option<String>,
}

pub struct BuildReport {
    pub plans: Vec<ModulePlan>,
}

pub struct Settings<'a> {
    pub target_name: String,
    pub mode: String,
    pub target: &'a Target,
    pub profile: &'a Profile,
    pub toolchain: &'a Toolchain,
    pub naming: Naming,
    pub jobs: usize,
    pub strip: bool,
}

pub fn auto_jobs() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

pub fn resolve<'a>(project: &'a Project, opts: &BuildOptions) -> Result<Settings<'a>> {
    let global = &project.global;
    let target_name = opts.target.clone().unwrap_or_else(|| global.default_target.clone());
    let mode = opts.mode.clone().unwrap_or_else(|| global.default_mode.clone());
    let target = global.target(&target_name)?;
    let profile = global.profile(&mode)?;
    let toolchain = global.toolchain(&target.toolchain)?;
    let naming = target.naming(toolchain);
    let jobs = match opts.jobs {
        Some(0) | None => match target.jobs {
            Some(n) if n > 0 => n,
            _ => {
                if global.jobs > 0 {
                    global.jobs
                } else {
                    auto_jobs()
                }
            }
        },
        Some(n) => n,
    };
    let strip = profile.strip || target.strip;
    Ok(Settings {
        target_name,
        mode,
        target,
        profile,
        toolchain,
        naming,
        jobs: jobs.max(1),
        strip,
    })
}

fn plan_all(project: &Project, settings: &Settings<'_>, opts: &BuildOptions, order: &[String]) -> Result<Vec<ModulePlan>> {
    let ctx = PlanCtx {
        project,
        project_root: &project.root,
        target_name: &settings.target_name,
        target: settings.target,
        profile: settings.profile,
        toolchain: settings.toolchain,
        naming: &settings.naming,
        build_dir: &project.global.build_dir,
        force: opts.force,
        strip: settings.strip,
    };
    order
        .iter()
        .map(|name| steps::plan_module(&ctx, name).map_err(|e| e.context(format!("planning module {name}"))))
        .collect()
}

pub fn build(project: &Project, opts: &BuildOptions) -> Result<BuildReport> {
    let started = Instant::now();
    let settings = resolve(project, opts)?;
    let order = project.topo_order(opts.scope.as_deref())?;
    if order.is_empty() {
        crate::hk_bail!("there is nothing to build");
    }
    let plans = plan_all(project, &settings, opts, &order)?;

    if opts.print_plan {
        print_plan(project, &settings, opts, &plans)?;
        return Ok(BuildReport { plans });
    }

    let index: HashMap<&str, usize> = plans.iter().enumerate().map(|(i, p)| (p.module.as_str(), i)).collect();
    let levels = project.topo_levels()?;
    let mut executed = 0usize;

    for level in &levels {
        let level_paths: Vec<usize> = level
            .iter()
            .filter_map(|name| index.get(name.as_str()).copied())
            .collect();
        if level_paths.is_empty() {
            continue;
        }
        for phase in 0..3usize {
            let batch: Vec<&Step> = level_paths
                .iter()
                .flat_map(|idx| plans[*idx].phases[phase].iter())
                .collect();
            if batch.is_empty() {
                continue;
            }
            for idx in &level_paths {
                if let Some(header) = phase_header(&plans[*idx], phase) {
                    // Progress lines are INFO: a release build of hk (default level WARN)
                    // prints only warnings/errors and the final summary. Use -v to see them.
                    crate::log_info!("==> {header}");
                }
            }
            executed += batch.len();
            run_parallel(settings.jobs, &batch, !opts.keep_going, |step: &&Step| execute_step(step))
                .map_err(|e| {
                    e.context(format!(
                        "build failed (target {} / mode {})",
                        settings.target_name, settings.mode
                    ))
                })?;
        }
    }

    let (up_to_date, artifacts) = summarize(&plans);
    let elapsed = started.elapsed();
    crate::log_plain!(
        "Build finished: {} module(s), {} step(s) executed, {} up to date, {:.2}s",
        plans.len(),
        executed,
        up_to_date,
        elapsed.as_secs_f64()
    );
    if !artifacts.is_empty() {
        crate::log_plain!("Artifacts:");
        for artifact in artifacts {
            crate::log_plain!("  {}", util::short_path(&artifact));
        }
    }

    Ok(BuildReport { plans })
}

fn summarize(plans: &[ModulePlan]) -> (usize, Vec<PathBuf>) {
    let mut up_to_date = 0;
    let mut artifacts = Vec::new();
    for plan in plans {
        if plan.is_up_to_date() {
            up_to_date += 1;
        }
        artifacts.extend(plan.artifacts.iter().cloned());
    }
    (up_to_date, artifacts)
}

fn phase_header(plan: &ModulePlan, phase: usize) -> Option<String> {
    let steps = &plan.phases[phase];
    if steps.is_empty() {
        return None;
    }
    Some(match phase {
        0 => format!("{}: compiling {} file(s)", plan.module, steps.len()),
        1 => format!("{}: {} -> {}", plan.module, steps[0].action, step_target(&steps[0])),
        _ => format!("{}: {} post-link step(s) ({})", plan.module, steps.len(), steps.iter().map(|s| s.action).collect::<Vec<_>>().join(" / ")),
    })
}

fn step_target(step: &Step) -> String {
    if let Some(artifact) = &step.artifact {
        return util::short_path(artifact);
    }
    let args: Vec<String> = step.cmd.args.iter().map(|a| a.to_string_lossy().to_string()).collect();
    match args.iter().rposition(|a| a == "-o") {
        Some(idx) if idx + 1 < args.len() => args[idx + 1].clone(),
        _ => step.module.clone(),
    }
}

fn execute_step(step: &Step) -> Result<()> {
    let what = format!("{} {}", step.module, step.action);
    crate::log_debug!("[{}] {}", step.action, step.cmd.display());
    // Create artifact directories on demand (obj / lib / bin)
    if let Some(artifact) = &step.artifact {
        if let Some(parent) = artifact.parent() {
            util::ensure_dir(parent)?;
        }
    }
    // Create the compiler temp directory on demand
    if let Some(temp) = &step.temp_dir {
        util::ensure_dir(temp)?;
    }
    match &step.kind {
        StepKind::Size => {
            let out = step.cmd.run_checked(&what)?;
            crate::log_plain!("{} section sizes (text/data/bss/dec/hex):", step.module);
            crate::log_plain!("{}", out.stdout.trim_end());
        }
        StepKind::Post(PostAction::Lst) => {
            let out = step.cmd.run_checked(&what)?;
            if let Some(artifact) = &step.artifact {
                util::write_if_changed(artifact, &out.stdout)?;
                crate::log_debug!("{}: disassembly written to {}", step.module, artifact.display());
            }
        }
        StepKind::Link { memory_usage } => {
            let out = step.cmd.run_checked(&what)?;
            let mut remaining = out.stderr.clone();
            if *memory_usage {
                let rows = parse_memory_usage(&out.stderr);
                if !rows.is_empty() {
                    crate::log_plain!("{} memory usage:", step.module);
                    for row in &rows {
                        let warn = if row.oversized() { "  <== over capacity!" } else { "" };
                        crate::log_plain!("  {}{}", row.render(), warn);
                    }
                    remaining = strip_memory_lines(&out.stderr);
                }
            }
            report_warnings(&remaining);
        }
        _ => {
            let out = step.cmd.run_checked(&what)?;
            report_warnings(&out.stderr);
        }
    }
    if let Some((path, text)) = &step.sig_file {
        util::write_if_changed(path, text)?;
    }
    Ok(())
}

fn report_warnings(stderr: &str) {
    let text = stderr.trim();
    if !text.is_empty() {
        crate::log::print_block(text);
    }
}

/// Drop the --print-memory-usage table; whatever remains is a compiler diagnostic.
fn strip_memory_lines(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("Memory region") && !trimmed.ends_with('%')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn print_plan(project: &Project, settings: &Settings<'_>, opts: &BuildOptions, plans: &[ModulePlan]) -> Result<()> {
    let mut lines = Vec::new();
    lines.push(format!(
        "# hk build plan — project={} target={} mode={} jobs={}",
        project.global.project.clone().unwrap_or_else(|| "-".into()),
        settings.target_name,
        opts.mode.clone().unwrap_or_else(|| project.global.default_mode.clone()),
        settings.jobs
    ));
    lines.push(format!(
        "# toolchain={} (cc={}) strip={} build_dir={}",
        settings.target.toolchain,
        settings.toolchain.cc.clone().unwrap_or_default(),
        settings.strip,
        project.global.build_dir
    ));
    for plan in plans {
        lines.push(format!("# module {} [{}]", plan.module, plan.kind.as_str()));
        if plan.is_up_to_date() {
            lines.push("    # up to date, skipped".to_string());
            continue;
        }
        for phase in &plan.phases {
            for step in phase {
                lines.push(format!("    {}", util::plan_line(&step.cmd)));
            }
        }
    }
    for line in &lines {
        println!("{line}");
    }
    if let Some(path) = &opts.plan_output {
        util::write_plan(path, &lines)?;
        crate::log_info!("plan written to {}", path.display());
    }
    Ok(())
}

/// Run in parallel; with stop_on_error = false the remaining independent work is
/// still attempted.
pub fn run_parallel<T, F>(jobs: usize, items: &[T], stop_on_error: bool, f: F) -> Result<()>
where
    T: Sync,
    F: Fn(&T) -> Result<()> + Sync,
{
    if items.is_empty() {
        return Ok(());
    }
    let workers = jobs.max(1).min(items.len());
    if workers <= 1 {
        for item in items {
            f(item)?;
        }
        return Ok(());
    }
    let cursor = AtomicUsize::new(0);
    let failure: Mutex<Option<HkError>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if stop_on_error && failure.lock().map(|slot| slot.is_some()).unwrap_or(true) {
                    break;
                }
                let index = cursor.fetch_add(1, Ordering::SeqCst);
                if index >= items.len() {
                    break;
                }
                if let Err(err) = f(&items[index]) {
                    if let Ok(mut slot) = failure.lock() {
                        if slot.is_none() {
                            *slot = Some(err);
                        }
                    }
                    if stop_on_error {
                        break;
                    }
                }
            });
        }
    });
    match failure.into_inner() {
        Ok(Some(err)) => Err(err),
        _ => Ok(()),
    }
}

#[derive(Debug, PartialEq)]
pub struct MemoryRow {
    pub region: String,
    pub used: f64,
    pub used_unit: String,
    pub size: f64,
    pub size_unit: String,
    pub percent: f64,
}

impl MemoryRow {
    pub fn render(&self) -> String {
        format!(
            "{:>8}  {:>10.0} {} / {:.0} {}  ({:.2}%)",
            self.region, self.used, self.used_unit, self.size, self.size_unit, self.percent
        )
    }

    pub fn oversized(&self) -> bool {
        self.percent > 100.0
    }
}

/// Parse the output of gcc -Wl,--print-memory-usage.
pub fn parse_memory_usage(stderr: &str) -> Vec<MemoryRow> {
    let mut rows = Vec::new();
    for line in stderr.lines() {
        let trimmed = line.trim();
        let colon = match trimmed.find(':') {
            Some(idx) => idx,
            None => continue,
        };
        let region = trimmed[..colon].trim().to_string();
        if region.is_empty() || region.split_whitespace().count() > 1 {
            continue;
        }
        let tokens: Vec<&str> = trimmed[colon + 1..].split_whitespace().collect();
        if tokens.len() < 5 {
            continue;
        }
        let (used, used_unit) = match tokens[0].parse::<f64>() {
            Ok(value) => (value, tokens[1].to_string()),
            Err(_) => continue,
        };
        let size = match tokens[2].parse::<f64>() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let percent = tokens
            .iter()
            .find_map(|t| t.strip_suffix('%').and_then(|v| v.parse::<f64>().ok()));
        rows.push(MemoryRow {
            region,
            used,
            used_unit,
            size,
            size_unit: tokens[3].to_string(),
            percent: percent.unwrap_or(0.0),
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gcc_memory_usage_report() {
        let stderr = "Memory region         Used Size  Region Size  %age Used\n\
                      \x20          FLASH:       12288 B       512 KB      2.34%\n\
                      \x20            RAM:        3200 B       128 KB      2.44%\n";
        let rows = parse_memory_usage(stderr);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].region, "FLASH");
        assert_eq!(rows[0].used, 12288.0);
        assert_eq!(rows[0].percent, 2.34);
        assert!(!rows[0].oversized());
    }

    #[test]
    fn oversized_region_is_flagged() {
        let stderr = "RAM:  140000 B  128 KB  106.81%";
        let rows = parse_memory_usage(stderr);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].oversized());
    }

    #[test]
    fn parallel_execution_stops_on_first_error() {
        let items = vec![1, 2, 3, 4, 5];
        let result = run_parallel(2, &items, true, |item| {
            if *item == 3 {
                Err(HkError::new("boom"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
    }

    #[test]
    fn parallel_execution_runs_every_item() {
        let items: Vec<u32> = (0..32).collect();
        let seen = Mutex::new(Vec::new());
        run_parallel(4, &items, true, |item| {
            seen.lock().unwrap().push(*item);
            Ok(())
        })
        .unwrap();
        let mut seen = seen.into_inner().unwrap();
        seen.sort_unstable();
        assert_eq!(seen, items);
    }
}
