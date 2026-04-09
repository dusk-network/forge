use std::fs;

use crate::cli::BuildArgs;
use crate::error::Result;
use crate::project::{detect, metadata};
use crate::{build_runner, toolchain, ui};

pub fn run(args: BuildArgs) -> Result<()> {
    let project = metadata::load(&args.project.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    toolchain::ensure_build(&project.project_dir, args.target.needs_rust_src())?;

    for target in args.target.expand() {
        ui::status(format!(
            "Building {} WASM ({})",
            target.label(),
            project.crate_name
        ));

        let wasm_path = build_runner::build(&project, target, args.project.verbose)?;
        let optimized =
            build_runner::wasm_opt::optimize_if_available(&wasm_path, args.project.verbose)?;

        let size = fs::metadata(&wasm_path)?.len();
        if !optimized {
            ui::warn("wasm-opt not found, skipping optimization");
        }

        ui::success(format!(
            "{} wasm: {} ({})",
            target.label(),
            wasm_path.display(),
            ui::format_bytes(size)
        ));
    }

    Ok(())
}
