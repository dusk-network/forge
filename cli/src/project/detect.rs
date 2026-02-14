use std::{fs, path::Path};

use toml::Value;

use crate::error::{CliError, Result};

#[derive(Debug, Clone)]
pub struct ManifestChecks {
    pub has_dusk_forge_dependency: bool,
    pub has_cdylib: bool,
    pub has_contract_feature: bool,
    pub has_data_driver_feature: bool,
    pub has_release_overflow_checks: bool,
}

pub fn ensure_forge_project(project_dir: &Path) -> Result<ManifestChecks> {
    let checks = inspect_manifest(project_dir)?;

    if !checks.has_dusk_forge_dependency || !checks.has_cdylib {
        return Err(CliError::NotAForgeProject(project_dir.to_path_buf()));
    }

    Ok(checks)
}

pub fn inspect_manifest(project_dir: &Path) -> Result<ManifestChecks> {
    let manifest = load_manifest(project_dir)?;

    Ok(ManifestChecks {
        has_dusk_forge_dependency: has_dusk_forge_dependency(&manifest),
        has_cdylib: has_cdylib(&manifest),
        has_contract_feature: has_feature(&manifest, "contract"),
        has_data_driver_feature: has_feature(&manifest, "data-driver")
            || has_feature(&manifest, "data-driver-js"),
        has_release_overflow_checks: has_release_overflow_checks(&manifest),
    })
}

pub fn load_manifest(project_dir: &Path) -> Result<Value> {
    let manifest_path = project_dir.join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path)?;
    Ok(content.parse::<Value>()?)
}

fn has_dusk_forge_dependency(manifest: &Value) -> bool {
    has_dependency(manifest.get("dependencies"), "dusk-forge")
        || manifest
            .get("target")
            .and_then(Value::as_table)
            .and_then(|target| target.get("cfg(target_family = \"wasm\")"))
            .and_then(|cfg| cfg.get("dependencies"))
            .is_some_and(|deps| has_dependency(Some(deps), "dusk-forge"))
}

fn has_dependency(table: Option<&Value>, name: &str) -> bool {
    table
        .and_then(Value::as_table)
        .is_some_and(|deps| deps.contains_key(name))
}

fn has_cdylib(manifest: &Value) -> bool {
    manifest
        .get("lib")
        .and_then(|lib| lib.get("crate-type"))
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|ty| ty.as_str() == Some("cdylib")))
}

fn has_feature(manifest: &Value, name: &str) -> bool {
    manifest
        .get("features")
        .and_then(Value::as_table)
        .is_some_and(|features| features.contains_key(name))
}

fn has_release_overflow_checks(manifest: &Value) -> bool {
    manifest
        .get("profile")
        .and_then(|p| p.get("release"))
        .and_then(|release| release.get("overflow-checks"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
