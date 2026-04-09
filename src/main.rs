mod cli;
mod module;
mod utils;

use crate::{
    cli::context::CmdCtx,
    utils::yaml,
    utils::log,
    utils::logo::print_logo,
};
use clap::{CommandFactory, Parser, Subcommand};
use std::{error::Error, path::PathBuf};
#[derive(Parser)]
#[command(
    version = "1.0.0",
    author = "hui",
    about = "A lightweight build tool as an alternative to CMake and Makefile for C projects.",
    long_about = "HuiMake scans your project directory for modules, resolves dependencies between them,\nand orchestrates the build order automatically. Each module is a directory with its own\nconfig.yaml describing dependencies, compiler settings, and more."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Command>,
}
#[derive(Debug, Subcommand)]
enum Command {
    /// Build the current module or entire project
    Build {
        #[arg(
            short,
            long,
            help = "Build mode: debug or release (default: debug)",
            default_value = "debug"
        )]
        mode: Option<String>,
    },

    /// Remove all build artifacts (build/ directory)
    Clean,

    /// Build and run the module or project
    Run,

    /// Create a new module with the standard directory structure
    New {
        #[arg(help = "Name of the new module to create", default_value = "new_hk_project")]
        name: Option<String>,
    },
}
fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let mut cmd_data = CmdCtx::new();
    // println!("build mode is {:#?}",cmd_data);
    match &cli.cmd {
        Some(Command::New { name }) => {
            if let Some(name_vale) = name {
                cmd_data.gen(name_vale)?;
            } else {
                println!("have no name");
            }
        }
        Some(Command::Build { mode }) => {
            cmd_data.detect_env()?;
            match mode {
                Some(value) => {
                    if value.eq_ignore_ascii_case("release") {
                        println!("Release Mode");
                    } else {
                        cmd_data.build(false)?;
                        println!("Debug Mode");
                    }
                }
                _ => {
                    println!("Debug Mode");
                }
            }
        }
        Some(Command::Clean) => {
            cmd_data.detect_env()?;
            print_logo();
            cmd_data.clean()?;
        }

        Some(Command::Run) => {
            cmd_data.detect_env()?;
            cmd_data.run()?;
        }
        None => {
            // 显示帮助信息
            print_logo();
            let mut cmd = Cli::command();
            cmd.print_help().unwrap();
        }
    }
    Ok(())
}
