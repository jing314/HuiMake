mod serde;
mod mods;
mod make_tools;

use std::{error::Error, path::PathBuf};
use crate::{make_tools::usecc, mods::{analyzer::ModsManage, single::ModFile}, serde::yaml};
// use crate::make_tools;
use clap::{CommandFactory, Parser, Subcommand};
use serde_yaml::value;
#[derive(Parser)]
#[command(
    version = "1.0.0", 
    author = "hui", 
    about = "This is a small tool that serves as an alternative to CMake and Makefile.", 
    long_about = None
)]
struct Cli{
    #[command(subcommand)]
    cmd: Option<Command>,
}
#[derive(Debug)]
#[derive(Subcommand)]
enum Command {
    /// build project
    Build {
        #[arg(short, long, help = "build (debug/release) default = debug",default_value = "debug")]
        mode: Option<String>,
    },
    
    /// clean project mod
    Clean {
        #[arg(short, long, help = "clean all build file")]
        all: bool,
    },
    
    /// build and run
    Run {
        #[arg(help = "program argument")]
        args: Vec<String>,
    },
        
    /// new hk project
    New {
        #[arg(help = "project name")]
        name: Option<String>,
    },
}
fn main() ->Result<(),Box<dyn Error>>{
    let cli = Cli::parse();

    match &cli.cmd {
        Some(Command::Build { mode }) =>{
            let mut project = ModsManage::build_mods_depsgraph()?;
            match mode {
                Some(value)=>{
                    if value.eq_ignore_ascii_case("Release"){
                        println!("Release Mode");
                    }else {
                        let nexts = project.get_next_build_mod()?;
                        for iter in nexts{
                            println!("222222222");
                            usecc::build_single_c_mod(&iter)?;
                            println!("444444444")
                        }
                        // println!("{:#?}",project);
                        println!("Debug Mode");
                    }
                }
                _=>{
                    println!("Debug Mode");
                }
            }
        }
        Some(Command::Clean { all })=>{
            if *all {
                println!("clean all");
            }else{
                println!("clean currect mod");
            }
        }
        Some(Command::New { name })=>{
            if let Some(name_vale) = name {
                ModFile::gen(name_vale)?;
            }else {
                println!("have no name");
            }
        }
        Some(Command::Run { args })=>{
            if args.is_empty(){
                println!("build have no arge");
            }else {
                println!("build and run with args");
            }
        }
        None =>{
             // 显示帮助信息
            let mut cmd = Cli::command();
            cmd.print_help().unwrap();
        }
    }
    Ok(())
}
