mod serde;
mod hkmod;

use std::{error::Error, path::PathBuf};

use crate::{hkmod::single_mod::ModFile, serde::yaml};

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
    let base_path = PathBuf::from("./");
    let config = yaml::Config::from_yaml(&base_path.join("config.yaml"))?;
    println!("{:#?}",config);
    let cli = Cli::parse();
    match &cli.cmd {
        Some(Command::Build { mode }) =>{
            match mode {
                Some(value)=>{
                    if value.eq_ignore_ascii_case("Release"){
                        println!("Release Mode");
                    }else {
                        let modinfo = ModFile::get_info(&base_path)?;
                        println!("{:#?}",modinfo);
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
