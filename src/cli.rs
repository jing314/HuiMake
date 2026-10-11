//! 命令行定义。

use crate::config::Kind;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "hk",
    version,
    author = "hui",
    about = "HuiMake: 用 YAML 替代 CMake/Makefile 的 C/C++ 构建工具，支持交叉编译与 MCU 目标"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// 提高日志详细程度（-v 显示命令行，-vv 显示全部细节）
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
    /// 只输出错误
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 构建当前模块或整个工程
    Build(BuildArgs),
    /// 构建并运行（仅限可在本机运行的目标）
    Run(RunArgs),
    /// 清理构建产物
    Clean(CleanArgs),
    /// 创建新的模块目录
    New(NewArgs),
    /// 在工程根生成 hk.yaml（导出内置默认配置）
    Init(InitArgs),
    /// 打印当前生效的配置
    Config(ConfigArgs),
}

#[derive(Debug, Args, Clone, Default)]
pub struct BuildArgs {
    /// 构建模式（hk.yaml 中 profiles 的名字，例如 debug / release / mcu-release）
    #[arg(short, long)]
    pub mode: Option<String>,
    /// 目标（hk.yaml 中 targets 的名字，例如 host / cortex-m4f / riscv32-unknown-elf）
    #[arg(short, long)]
    pub target: Option<String>,
    /// 并行任务数，0 表示自动
    #[arg(short, long)]
    pub jobs: Option<usize>,
    /// 只构建指定模块及其依赖
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,
    /// 只打印将执行的命令，不实际编译
    #[arg(long)]
    pub print_plan: bool,
    /// 把构建计划写入文件（配合 --print-plan）
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// 忽略增量信息，全量重建
    #[arg(long)]
    pub force: bool,
    /// 某个模块失败后继续构建其他独立模块
    #[arg(short = 'k', long)]
    pub keep_going: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub build: BuildArgs,
    /// 只运行指定的模块或可执行文件
    #[arg(long, value_name = "NAME")]
    pub bin: Option<String>,
    /// 传给目标程序的参数（需写在 -- 之后）
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CleanArgs {
    /// 只清理指定模块
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,
    /// 清理工程内所有模块（忽略当前所在模块）
    #[arg(long)]
    pub all: bool,
    /// 只列出将要删除的目录
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
    /// 模块名（同时作为目录名）
    pub name: String,
    /// 产物类型
    #[arg(short, long, value_enum, default_value_t = KindArg::Executable)]
    pub kind: KindArg,
    /// 在指定目录下创建（默认当前目录）
    #[arg(short, long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// 覆盖已存在的目录
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// 覆盖已存在的 hk.yaml
    #[arg(long)]
    pub force: bool,
    /// 输出文件（默认 hk.yaml）
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// 写出到文件（默认打印到 stdout）
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// 只查看某个目标解析后的结果
    #[arg(short, long, value_name = "TARGET")]
    pub target: Option<String>,
    /// 只查看某个模式解析后的结果
    #[arg(short, long, value_name = "MODE")]
    pub mode: Option<String>,
}
