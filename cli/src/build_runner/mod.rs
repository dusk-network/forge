pub mod wasm_opt;

use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    error::{CliError, Result},
    project::metadata::ProjectMetadata,
    toolchain::{self, WASM_TARGET},
};

const CONTRACT_FEATURE: &str = "contract";
const DATA_DRIVER_FEATURE: &str = "data-driver-js";
const STACK_SIZE: u32 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTarget {
    Contract,
    DataDriver,
}

impl BuildTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::DataDriver => "data-driver",
        }
    }

    pub fn wasm_path(self, project: &ProjectMetadata) -> PathBuf {
        match self {
            Self::Contract => project.contract_wasm_path.clone(),
            Self::DataDriver => project.data_driver_wasm_path.clone(),
        }
    }
}

pub fn build(project: &ProjectMetadata, target: BuildTarget, verbose: bool) -> Result<PathBuf> {
    let mut cmd = Command::new("cargo");
    let toolchain_arg = toolchain::cargo_toolchain_arg(&project.project_dir)?;

    cmd.arg(&toolchain_arg)
        .arg("build")
        .arg("--release")
        .arg("--locked")
        .arg("--target")
        .arg(WASM_TARGET)
        .arg("--features")
        .arg(match target {
            BuildTarget::Contract => CONTRACT_FEATURE,
            BuildTarget::DataDriver => DATA_DRIVER_FEATURE,
        })
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .arg("--color=always");

    if target == BuildTarget::Contract {
        cmd.arg("-Z").arg("build-std=core,alloc");
    }

    let target_dir = match target {
        BuildTarget::Contract => &project.contract_target_dir,
        BuildTarget::DataDriver => &project.data_driver_target_dir,
    };

    cmd.env("CARGO_TARGET_DIR", target_dir)
        .env("RUSTFLAGS", compose_rustflags(target))
        .current_dir(&project.project_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit());
    apply_local_forge_overrides(&mut cmd, verbose);

    if verbose {
        eprintln!("Running: {}", crate::ui::format_command(&cmd));
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            program: "cargo build".to_string(),
            code: status.code().unwrap_or(1),
        });
    }

    let wasm_path = target.wasm_path(project);
    ensure_file_exists(&wasm_path)?;

    Ok(wasm_path)
}

pub fn apply_local_forge_overrides(cmd: &mut Command, verbose: bool) {
    let mut applied = Vec::new();

    if let Some((forge_root, macro_root)) = local_forge_paths() {
        append_patch_config(cmd, "dusk-forge", &forge_root);
        append_patch_config(cmd, "dusk-forge-contract", &macro_root);
        applied.push(format!("dusk-forge -> {}", forge_root.display()));
        applied.push(format!("dusk-forge-contract -> {}", macro_root.display()));
    }

    if verbose && !applied.is_empty() {
        eprintln!("Applying local overrides: {}", applied.join(", "));
    }
}

fn local_forge_paths() -> Option<(PathBuf, PathBuf)> {
    let cli_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forge_root = cli_dir.parent()?.to_path_buf();
    let macro_root = forge_root.join("contract-macro");

    if forge_root.join("Cargo.toml").is_file() && macro_root.join("Cargo.toml").is_file() {
        Some((forge_root, macro_root))
    } else {
        None
    }
}

fn append_patch_config(cmd: &mut Command, crate_name: &str, path: &Path) {
    let path_escaped = toml_escape(path.as_os_str());
    cmd.arg("--config").arg(format!(
        "patch.crates-io.{crate_name}.path=\"{path_escaped}\""
    ));
}

fn toml_escape(value: &OsStr) -> String {
    let raw = value.to_string_lossy();
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn compose_rustflags(target: BuildTarget) -> String {
    let mut parts: Vec<String> = env::var("RUSTFLAGS")
        .ok()
        .map(|existing| {
            existing
                .split_whitespace()
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            parts.push("--remap-path-prefix".to_string());
            parts.push(format!("{home}="));
        }
    }

    if target == BuildTarget::Contract {
        parts.push("-C".to_string());
        parts.push(format!("link-args=-zstack-size={STACK_SIZE}"));
    }

    parts.join(" ")
}

fn ensure_file_exists(path: &Path) -> Result<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(CliError::Message(format!(
            "expected build artifact not found: {}",
            path.display()
        )))
    }
}
