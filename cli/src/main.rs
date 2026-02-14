mod build_runner;
mod cli;
mod commands;
mod data_driver_wasm;
mod error;
mod project;
mod template;
mod toolchain;
mod tools;
mod ui;

use clap::Parser;
use cli::{Cli, Commands};
use error::Result;

fn main() {
    if let Err(err) = run() {
        ui::error(err.to_string());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New(args) => commands::new::run(args),
        Commands::Build(args) => commands::build::run(args),
        Commands::Test(args) => commands::test::run(args),
        Commands::Check(args) => commands::check::run(args),
        Commands::Expand(args) => commands::expand::run(args),
        Commands::Clean(args) => commands::clean::run(args),
        Commands::Schema(args) => commands::schema::run(args),
        Commands::Call(args) => commands::call::run(args),
        Commands::Verify(args) => commands::verify::run(args),
        Commands::Completions(args) => commands::completions::run(args),
    }
}
