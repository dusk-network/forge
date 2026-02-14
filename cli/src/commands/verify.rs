#[cfg(feature = "schema")]
use std::fs;

use crate::{cli::VerifyArgs, error::Result};

#[cfg(feature = "schema")]
use crate::{
    build_runner::{self, BuildTarget},
    data_driver_wasm::DataDriverWasm,
    error::CliError,
    project::{detect, metadata},
    toolchain, ui,
};

#[cfg(feature = "schema")]
pub fn run(args: VerifyArgs) -> Result<()> {
    let project = metadata::load(&args.project.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    let contract_wasm = if args.skip_build {
        project.contract_wasm_path.clone()
    } else {
        toolchain::ensure_build(&project.project_dir, true)?;
        ui::status("Building contract WASM for verification");
        let wasm = build_runner::build(&project, BuildTarget::Contract, args.project.verbose)?;
        let optimized = build_runner::wasm_opt::optimize_if_available(&wasm, args.project.verbose)?;
        if !optimized {
            ui::warn("wasm-opt not found, skipping optimization");
        }
        wasm
    };

    let data_driver_wasm = if args.skip_build {
        project.data_driver_wasm_path.clone()
    } else {
        toolchain::ensure_build(&project.project_dir, false)?;
        ui::status("Building data-driver WASM for verification");
        let wasm = build_runner::build(&project, BuildTarget::DataDriver, args.project.verbose)?;
        let optimized = build_runner::wasm_opt::optimize_if_available(&wasm, args.project.verbose)?;
        if !optimized {
            ui::warn("wasm-opt not found, skipping optimization");
        }
        wasm
    };

    if !contract_wasm.exists() {
        return Err(CliError::Message(format!(
            "contract WASM not found: {}",
            contract_wasm.display()
        )));
    }

    if !data_driver_wasm.exists() {
        return Err(CliError::Message(format!(
            "data-driver WASM not found: {}",
            data_driver_wasm.display()
        )));
    }

    DataDriverWasm::validate_module(&contract_wasm)?;
    ui::success(format!("Valid WASM module: {}", contract_wasm.display()));

    DataDriverWasm::validate_module(&data_driver_wasm)?;
    ui::success(format!("Valid WASM module: {}", data_driver_wasm.display()));

    let contract_bytes = fs::read(&contract_wasm)?;
    let actual_hash = blake3::hash(&contract_bytes).to_hex().to_string();

    if let Some(expected) = args.expected_blake3 {
        let expected_normalized = expected.trim_start_matches("0x").to_ascii_lowercase();
        if actual_hash != expected_normalized {
            return Err(CliError::Message(format!(
                "BLAKE3 mismatch: expected {expected_normalized}, got {actual_hash}"
            )));
        }
        ui::success("Contract BLAKE3 hash matches expected value");
    }

    let mut driver = DataDriverWasm::load(&data_driver_wasm)?;
    let schema_json = driver.get_schema_json()?;
    let schema: serde_json::Value = serde_json::from_str(&schema_json)?;

    let contract_name = schema
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CliError::Message("schema is missing 'name'".to_string()))?;

    let function_count = schema
        .get("functions")
        .and_then(serde_json::Value::as_array)
        .map(std::vec::Vec::len)
        .ok_or_else(|| CliError::Message("schema is missing 'functions' array".to_string()))?;

    ui::success(format!(
        "Schema loaded for {contract_name} with {function_count} function(s)"
    ));

    if function_count == 0 {
        return Err(CliError::Message(
            "schema contains zero functions".to_string(),
        ));
    }

    println!("contract_wasm: {}", contract_wasm.display());
    println!("data_driver_wasm: {}", data_driver_wasm.display());
    println!("contract_blake3: {actual_hash}");
    println!("schema_contract: {contract_name}");
    println!("schema_functions: {function_count}");

    ui::success("Verification passed");
    Ok(())
}

#[cfg(not(feature = "schema"))]
pub fn run(_args: VerifyArgs) -> Result<()> {
    Err(crate::error::CliError::Message(
        "verify command is disabled (build with --features schema)".to_string(),
    ))
}
