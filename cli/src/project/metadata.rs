use std::{
    fs,
    path::{Path, PathBuf},
};

use cargo_metadata::{MetadataCommand, Package};

use crate::error::{CliError, Result};
use crate::toolchain::WASM_TARGET;

#[derive(Debug, Clone)]
pub struct ProjectMetadata {
    pub project_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub crate_name: String,
    pub contract_target_dir: PathBuf,
    pub data_driver_target_dir: PathBuf,
    pub contract_wasm_path: PathBuf,
    pub data_driver_wasm_path: PathBuf,
}

pub fn load(project_dir: &Path) -> Result<ProjectMetadata> {
    let project_dir = fs::canonicalize(project_dir)?;
    let manifest_path = project_dir.join("Cargo.toml");
    if !manifest_path.exists() {
        return Err(CliError::Message(format!(
            "missing Cargo.toml at {}",
            manifest_path.display()
        )));
    }

    let manifest_utf8 = cargo_metadata::camino::Utf8PathBuf::from_path_buf(manifest_path.clone())
        .map_err(|_| {
        CliError::Message(format!(
            "manifest path contains invalid UTF-8: {}",
            manifest_path.display()
        ))
    })?;

    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()?;

    let package = select_package(&metadata.packages, &manifest_utf8).ok_or_else(|| {
        CliError::Message(format!(
            "unable to resolve package metadata for {}",
            manifest_path.display()
        ))
    })?;

    let crate_name = package.name.clone();
    let crate_name_snake = crate_name.replace('-', "_");
    let workspace_root = PathBuf::from(metadata.workspace_root.as_std_path());
    let contract_target_dir = workspace_root.join("target/contract");
    let data_driver_target_dir = workspace_root.join("target/data-driver");

    let contract_wasm_path = contract_target_dir
        .join(WASM_TARGET)
        .join("release")
        .join(format!("{crate_name_snake}.wasm"));
    let data_driver_wasm_path = data_driver_target_dir
        .join(WASM_TARGET)
        .join("release")
        .join(format!("{crate_name_snake}.wasm"));

    Ok(ProjectMetadata {
        project_dir,
        manifest_path,
        crate_name,
        contract_target_dir,
        data_driver_target_dir,
        contract_wasm_path,
        data_driver_wasm_path,
    })
}

fn select_package<'a>(
    packages: &'a [Package],
    manifest_path: &cargo_metadata::camino::Utf8PathBuf,
) -> Option<&'a Package> {
    packages
        .iter()
        .find(|pkg| pkg.manifest_path == *manifest_path)
        .or_else(|| packages.first())
}
