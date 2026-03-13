use std::process::{Command, Stdio};

use crate::{
    build_runner::{self, BuildTarget},
    cli::TestArgs,
    error::{CliError, Result},
    project::{detect, metadata},
    toolchain, ui,
};

pub fn run(args: TestArgs) -> Result<()> {
    let project = metadata::load(&args.project.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    toolchain::ensure_build(&project.project_dir, true)?;

    ui::status("Building contract WASM for tests");
    let wasm_path = build_runner::build(&project, BuildTarget::Contract, args.project.verbose)?;
    let optimized =
        build_runner::wasm_opt::optimize_if_available(&wasm_path, args.project.verbose)?;
    if !optimized {
        ui::warn("wasm-opt not found, skipping optimization");
    }

    ui::status("Running cargo test --release");
    let mut cmd = Command::new("cargo");
    cmd.arg(toolchain::cargo_toolchain_arg(&project.project_dir)?)
        .arg("test")
        .arg("--release")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .args(&args.cargo_test_args)
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
            program: "cargo test".to_string(),
            code: status.code().unwrap_or(1),
        });
    }

    ui::success("Tests completed");
    Ok(())
}
