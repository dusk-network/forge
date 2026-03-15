use crate::{cli::CallArgs, error::Result};

#[cfg(feature = "schema")]
use crate::{
    build_runner::{self, BuildTarget},
    data_driver_wasm::DataDriverWasm,
    project::{detect, metadata},
    toolchain, ui,
};

#[cfg(feature = "schema")]
pub fn run(args: CallArgs) -> Result<()> {
    let project = metadata::load(&args.project.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    toolchain::ensure_build(&project.project_dir, false)?;

    ui::status(format!(
        "Building data-driver WASM for function '{}'",
        args.function
    ));

    let wasm_path = build_runner::build(&project, BuildTarget::DataDriver, args.project.verbose)?;
    let optimized =
        build_runner::wasm_opt::optimize_if_available(&wasm_path, args.project.verbose)?;
    if !optimized {
        ui::warn("wasm-opt not found, skipping optimization");
    }

    let mut driver = DataDriverWasm::load(&wasm_path)?;
    let encoded = driver.encode_input(&args.function, &args.input)?;

    if args.project.verbose {
        ui::status(format!(
            "Encoded {} bytes for '{}'",
            encoded.len(),
            args.function
        ));
    }

    println!("{}", to_hex_prefixed(&encoded));
    ui::success("Call payload encoded");
    Ok(())
}

#[cfg(not(feature = "schema"))]
pub fn run(_args: CallArgs) -> Result<()> {
    Err(crate::error::CliError::Message(
        "call command is disabled (build with --features schema)".to_string(),
    ))
}

#[cfg(feature = "schema")]
fn to_hex_prefixed(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");

    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }

    out
}
