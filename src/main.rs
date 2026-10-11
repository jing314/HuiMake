//! HuiMake (hk) — a YAML-driven C/C++ build tool.

mod app;
mod build;
mod cli;
mod config;
mod error;
#[cfg(test)]
mod example_tests;
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
