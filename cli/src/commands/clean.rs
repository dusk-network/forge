use std::fs;

use crate::{
    cli::ProjectOptions,
    error::Result,
    project::{detect, metadata},
    ui,
};

pub fn run(args: ProjectOptions) -> Result<()> {
    let project = metadata::load(&args.path)?;
    detect::ensure_forge_project(&project.project_dir)?;

    remove_if_exists(&project.contract_target_dir)?;
    remove_if_exists(&project.data_driver_target_dir)?;

    ui::success("Cleaned target/contract and target/data-driver");
    Ok(())
}

fn remove_if_exists(path: &std::path::Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
        ui::status(format!("Removed {}", path.display()));
    } else {
        ui::status(format!("Skipped {}, not present", path.display()));
    }
    Ok(())
}
