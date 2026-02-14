use crate::{cli::SchemaArgs, error::Result};

#[cfg(feature = "schema")]
use crate::{
    build_runner::{self, BuildTarget},
    data_driver_wasm::DataDriverWasm,
    project::{detect, metadata},
    toolchain, ui,
};

#[cfg(feature = "schema")]
pub fn run(args: SchemaArgs) -> Result<()> {
    let project = metadata::load(&args.project.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    toolchain::ensure_build(&project.project_dir, false)?;

    ui::status("Building data-driver WASM");
    let wasm_path = build_runner::build(&project, BuildTarget::DataDriver, args.project.verbose)?;
    let optimized =
        build_runner::wasm_opt::optimize_if_available(&wasm_path, args.project.verbose)?;
    if !optimized {
        ui::warn("wasm-opt not found, skipping optimization");
    }

    let mut driver = DataDriverWasm::load(&wasm_path)?;
    let schema_json = driver.get_schema_json()?;
    let parsed: serde_json::Value = serde_json::from_str(&schema_json)?;

    if args.pretty {
        println!("{}", serde_json::to_string_pretty(&parsed)?);
    } else {
        println!("{}", serde_json::to_string(&parsed)?);
    }

    Ok(())
}

#[cfg(not(feature = "schema"))]
pub fn run(_args: SchemaArgs) -> Result<()> {
    Err(crate::error::CliError::Message(
        "schema command is disabled (build with --features schema)".to_string(),
    ))
}
