//! HuiMake (hk) —— YAML 驱动的 C/C++ 构建工具。

mod app;
mod build;
#[cfg(test)]
mod example_tests;
mod cli;
mod config;
mod error;
mod glob;
mod log;
mod project;
mod util;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    match app::run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            crate::log_error!("{err}");
            std::process::exit(1);
        }
    }
}
