use crate::{
    cli::ProjectOptions,
    error::{CliError, Result},
    project::{detect, metadata},
    toolchain, ui,
};

pub fn run(args: ProjectOptions) -> Result<()> {
    let project = metadata::load(&args.path)?;
    let checks = detect::inspect_manifest(&project.project_dir)?;
    let toolchain = toolchain::inspect(&project.project_dir)?;

    ui::status(format!(
        "Checking project at {}",
        project.project_dir.display()
    ));

    let mut failures = 0;

    record(
        "dusk-forge dependency present",
        checks.has_dusk_forge_dependency,
        &mut failures,
    );
    record(
        "lib crate-type includes cdylib",
        checks.has_cdylib,
        &mut failures,
    );
    record(
        "feature 'contract' exists",
        checks.has_contract_feature,
        &mut failures,
    );
    record(
        "feature 'data-driver' or 'data-driver-js' exists",
        checks.has_data_driver_feature,
        &mut failures,
    );
    record(
        "profile.release.overflow-checks = true",
        checks.has_release_overflow_checks,
        &mut failures,
    );

    record(
        "src/lib.rs exists",
        project.project_dir.join("src/lib.rs").exists(),
        &mut failures,
    );
    record(
        "tests/ directory exists",
        project.project_dir.join("tests").exists(),
        &mut failures,
    );
    record(
        "rust-toolchain.toml exists",
        project.project_dir.join("rust-toolchain.toml").exists(),
        &mut failures,
    );
    record(
        "Cargo.lock exists",
        project.project_dir.join("Cargo.lock").exists(),
        &mut failures,
    );

    let toolchain_check = format!("toolchain '{}' available", toolchain.channel);
    record(&toolchain_check, toolchain.installed, &mut failures);
    let target_check = format!(
        "wasm32-unknown-unknown target installed for {}",
        toolchain.channel
    );
    record(&target_check, toolchain.wasm_target, &mut failures);
    let rust_src_check = format!("rust-src component installed for {}", toolchain.channel);
    record(&rust_src_check, toolchain.rust_src, &mut failures);

    if let Some(path) = toolchain.wasm_opt {
        ui::success(format!("wasm-opt found at {}", path.display()));
    } else {
        ui::warn("wasm-opt not found (optional, but recommended for smaller binaries)");
    }

    if failures > 0 {
        return Err(CliError::Message(format!(
            "check failed with {failures} issue(s)"
        )));
    }

    ui::success("All checks passed");
    Ok(())
}

fn record(name: &str, ok: bool, failures: &mut usize) {
    if ok {
        ui::success(name);
    } else {
        *failures += 1;
        ui::error(name);
    }
}
