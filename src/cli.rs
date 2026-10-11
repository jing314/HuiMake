//! Command line definition.

use crate::config::Kind;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "hk",
    version,
    author = "hui",
    about = "HuiMake: a YAML-driven C/C++ build tool with cross-compilation and MCU support"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Increase log verbosity: -v shows per-step progress and every command, -vv shows all details
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Only print errors
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build the current module or the whole project
    Build(BuildArgs),
    /// Build and run (targets that cannot run on this host are refused)
    Run(RunArgs),
    /// Remove build artifacts
    Clean(CleanArgs),
    /// Create a new module with the standard directory layout
    New(NewArgs),
    /// Write hk.yaml (the built-in defaults) into the project root
    Init(InitArgs),
    /// Print the effective configuration
    Config(ConfigArgs),
}

#[derive(Debug, Args, Clone, Default)]
pub struct BuildArgs {
    /// Build mode: the name of a profile in hk.yaml (debug / release / mcu-release ...)
    #[arg(short, long)]
    pub mode: Option<String>,
    /// Target: the name of a target in hk.yaml (host / cortex-m4f / gd32f303 ...)
    #[arg(short, long)]
    pub target: Option<String>,
    /// Number of parallel jobs, 0 = auto
    #[arg(short, long)]
    pub jobs: Option<usize>,
    /// Only build this module and its dependencies
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,
    /// Print the commands instead of executing them
    #[arg(long)]
    pub print_plan: bool,
    /// Write the build plan to a file (use together with --print-plan)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Ignore incremental state and rebuild everything
    #[arg(long)]
    pub force: bool,
    /// Keep building independent modules after a failure
    #[arg(short = 'k', long)]
    pub keep_going: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub build: BuildArgs,
    /// Only run this module or executable
    #[arg(long, value_name = "NAME")]
    pub bin: Option<String>,
    /// Arguments passed to the program (must follow --)
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CleanArgs {
    /// Only clean this module
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,
    /// Clean every module in the project (ignore the current module)
    #[arg(long)]
    pub all: bool,
    /// Only list the directories that would be removed
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KindArg {
    Executable,
    StaticLib,
    SharedLib,
    Object,
    HeaderOnly,
}

impl From<KindArg> for Kind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Executable => Kind::Executable,
            KindArg::StaticLib => Kind::StaticLib,
            KindArg::SharedLib => Kind::SharedLib,
            KindArg::Object => Kind::Object,
            KindArg::HeaderOnly => Kind::HeaderOnly,
        }
    }
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// Module name (also used as the directory name)
    pub name: String,
    /// Kind of artifact the module produces
    #[arg(short, long, value_enum, default_value_t = KindArg::Executable)]
    pub kind: KindArg,
    /// Create the module inside this directory (defaults to the current one)
    #[arg(short, long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// Overwrite the template files of an existing directory
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Overwrite an existing hk.yaml
    #[arg(long)]
    pub force: bool,
    /// Output file (defaults to hk.yaml)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// Write to a file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Show the resolved settings of one target
    #[arg(short, long, value_name = "TARGET")]
    pub target: Option<String>,
    /// Show the resolved settings of one mode (profile)
    #[arg(short, long, value_name = "MODE")]
    pub mode: Option<String>,
}
