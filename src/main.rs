mod make_tools;
mod mods;
mod serde;
mod utility;
use crate::utility::logo;
use crate::{
    make_tools::cmdfn::CmdNeedData,
    mods::{analyzer::ModsManage, single::ModFile},
    serde::yaml,
    utility::logo::print_logo,
};
use clap::{CommandFactory, Parser, Subcommand};
use std::{error::Error, path::PathBuf};
#[derive(Parser)]
#[command(
    version = "1.0.0",                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       
    author = "hui", 
    about = "This is a small tool that serves as an alternative to CMake and Makefile.", 
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Command>,
}
#[derive(Debug, Subcommand)]
enum Command {
    /// build project
    Build {
        #[arg(
            short,
            long,
            help = "build (debug/release) default = debug",
            default_value = "debug"
        )]
        mode: Option<String>,
    },

    /// clean project mod
    Clean,

    /// build and run
    Run,

    /// new hk project
    New {
        #[arg(help = "project name", default_value = "new_hk_project")]
        name: Option<String>,
    },
}
fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let mut cmd_data = CmdNeedData::new();
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
            cmd_data.check()?;
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
            cmd_data.check()?;
            print_logo();
            cmd_data.clean()?;
        }

        Some(Command::Run) => {
            cmd_data.check()?;
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
