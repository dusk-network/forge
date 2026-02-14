use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

use crate::build_runner::BuildTarget;

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum BuildScope {
    /// Build the on-chain contract WASM.
    Contract,
    /// Build the off-chain data-driver WASM.
    DataDriver,
    /// Build both contract and data-driver WASMs.
    #[default]
    All,
}

impl BuildScope {
    pub fn expand(self) -> Vec<BuildTarget> {
        match self {
            Self::Contract => vec![BuildTarget::Contract],
            Self::DataDriver => vec![BuildTarget::DataDriver],
            Self::All => vec![BuildTarget::Contract, BuildTarget::DataDriver],
        }
    }

    pub fn needs_rust_src(self) -> bool {
        matches!(self, Self::Contract | Self::All)
    }
}

#[derive(Debug, Parser)]
#[command(name = "dusk-forge")]
#[command(bin_name = "dusk-forge")]
#[command(about = "CLI for scaffolding and building Dusk Forge contracts")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scaffold a new contract project.
    New(NewArgs),
    /// Build WASM artifacts (contract, data-driver, or all).
    Build(BuildArgs),
    /// Build contract WASM and run cargo tests.
    Test(TestArgs),
    /// Validate project structure and toolchain.
    Check(ProjectOptions),
    /// Show macro-expanded code using cargo-expand.
    Expand(ExpandArgs),
    /// Remove contract-specific build artifact directories.
    Clean(ProjectOptions),
    /// Generate shell completion scripts.
    Completions(CompletionsArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TemplateChoice {
    Counter,
    Empty,
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// Name of the new contract project (kebab-case).
    pub name: String,

    /// Directory in which the new project folder will be created.
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Built-in template to use.
    #[arg(long, value_enum, default_value_t = TemplateChoice::Counter)]
    pub template: TemplateChoice,

    /// Skip `git init` in the created project.
    #[arg(long)]
    pub no_git: bool,

    /// Enable verbose output.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub struct ProjectOptions {
    /// Path to the contract project directory.
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Enable verbose output.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    #[command(flatten)]
    pub project: ProjectOptions,

    /// Which WASM target to build.
    #[arg(value_enum, default_value_t)]
    pub target: BuildScope,
}

#[derive(Debug, Args)]
#[command(trailing_var_arg = true)]
pub struct TestArgs {
    #[command(flatten)]
    pub project: ProjectOptions,

    /// Extra args passed through to `cargo test --release`.
    pub cargo_test_args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ExpandArgs {
    #[command(flatten)]
    pub project: ProjectOptions,

    /// Expand with the data-driver feature.
    #[arg(long)]
    pub data_driver: bool,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: Shell,
}
