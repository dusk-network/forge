use std::{path::Path, process::Command};

use crate::error::{CliError, Result};
use crate::tools;

pub fn optimize_if_available(wasm_path: &Path, verbose: bool) -> Result<bool> {
    let wasm_opt = match tools::find_in_path("wasm-opt") {
        Some(path) => path,
        None => return Ok(false),
    };

    let mut cmd = Command::new(&wasm_opt);
    cmd.arg("-Oz")
        .arg("--strip-debug")
        .arg(wasm_path)
        .arg("-o")
        .arg(wasm_path);

    if verbose {
        eprintln!(
            "Running: {} -Oz --strip-debug {} -o {}",
            wasm_opt.display(),
            wasm_path.display(),
            wasm_path.display()
        );
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            program: wasm_opt.display().to_string(),
            code: status.code().unwrap_or(1),
        });
    }

    Ok(true)
}
