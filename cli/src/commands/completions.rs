use std::io;

use clap::CommandFactory;
use clap_complete::generate;

use crate::{
    cli::{Cli, CompletionsArgs},
    error::Result,
};

pub fn run(args: CompletionsArgs) -> Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, name, &mut io::stdout());
    Ok(())
}
