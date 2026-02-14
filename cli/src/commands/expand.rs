use std::process::{Command, Stdio};

use crate::{
    build_runner,
    cli::ExpandArgs,
    error::{CliError, Result},
    project::{detect, metadata},
    toolchain::{self, WASM_TARGET},
    tools, ui,
};

pub fn run(args: ExpandArgs) -> Result<()> {
    let project = metadata::load(&args.project.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    if tools::find_in_path("cargo-expand").is_none() {
        return Err(CliError::MissingTool {
            tool: "cargo-expand",
            hint: "Install with: cargo install cargo-expand",
        });
    }

    let feature = if args.data_driver {
        "data-driver-js"
    } else {
        "contract"
    };

    ui::status(format!("Expanding macros with feature '{feature}'"));

    let mut cmd = Command::new("cargo");
    cmd.arg(toolchain::cargo_toolchain_arg(&project.project_dir)?)
        .arg("expand")
        .arg("--release")
        .arg("--locked")
        .arg("--features")
        .arg(feature)
        .arg("--target")
        .arg(WASM_TARGET)
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .current_dir(&project.project_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit());
    build_runner::apply_local_forge_overrides(&mut cmd, args.project.verbose);

    if args.project.verbose {
        eprintln!("Running: {}", ui::format_command(&cmd));
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            program: "cargo expand".to_string(),
            code: status.code().unwrap_or(1),
        });
    }

    Ok(())
}
