use std::{fs, path::Path, process::Command};

use crate::{
    build_runner,
    cli::{NewArgs, TemplateChoice},
    error::{CliError, Result},
    template::{
        embedded::TemplateKind,
        engine::{render_template, validate_contract_name},
    },
    toolchain, ui,
};

pub fn run(args: NewArgs) -> Result<()> {
    let parsed_name = validate_contract_name(&args.name)?;
    let destination = args.path.join(&parsed_name.kebab);

    if destination.exists() {
        return Err(CliError::PathAlreadyExists(destination));
    }

    ui::status(format!("Creating project at {}", destination.display()));

    fs::create_dir_all(destination.join("src"))?;
    fs::create_dir_all(destination.join("tests"))?;

    let template_kind = match args.template {
        TemplateChoice::Counter => TemplateKind::Counter,
        TemplateChoice::Empty => TemplateKind::Empty,
    };

    let rendered = render_template(template_kind, &parsed_name);

    write_file(
        &destination.join("Cargo.toml"),
        &rendered.cargo_toml,
        args.verbose,
    )?;
    write_file(
        &destination.join("src/lib.rs"),
        &rendered.lib_rs,
        args.verbose,
    )?;
    write_file(
        &destination.join("tests/contract.rs"),
        &rendered.test_rs,
        args.verbose,
    )?;
    write_file(
        &destination.join("rust-toolchain.toml"),
        &rendered.rust_toolchain_toml,
        args.verbose,
    )?;
    write_file(
        &destination.join(".gitignore"),
        &rendered.gitignore,
        args.verbose,
    )?;
    write_file(
        &destination.join("Makefile"),
        &rendered.makefile,
        args.verbose,
    )?;

    generate_lockfile(&destination, args.verbose)?;

    if !args.no_git {
        maybe_init_git(&destination, args.verbose)?;
    }

    ui::success(format!("Project '{}' created", parsed_name.kebab));
    println!("Next steps:");
    println!("  cd {}", destination.display());
    println!("  dusk-forge check");
    println!("  dusk-forge build");

    Ok(())
}

fn write_file(path: &Path, content: &str, verbose: bool) -> Result<()> {
    fs::write(path, content)?;
    if verbose {
        ui::status(format!("Wrote {}", path.display()));
    }
    Ok(())
}

fn maybe_init_git(destination: &Path, verbose: bool) -> Result<()> {
    let output = Command::new("git")
        .arg("init")
        .current_dir(destination)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            if verbose {
                ui::status("Initialized git repository");
            }
            Ok(())
        }
        Ok(_) => {
            ui::warn("`git init` failed, continuing without git repository");
            Ok(())
        }
        Err(_) => {
            ui::warn("`git` not found, skipping git initialization");
            Ok(())
        }
    }
}

fn generate_lockfile(destination: &Path, verbose: bool) -> Result<()> {
    ui::status("Generating Cargo.lock");

    let mut cmd = Command::new("cargo");
    cmd.arg(toolchain::cargo_toolchain_arg(destination)?)
        .arg("generate-lockfile")
        .current_dir(destination);
    build_runner::apply_local_forge_overrides(&mut cmd, verbose);

    let status = cmd.status()?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            program: "cargo generate-lockfile".to_string(),
            code: status.code().unwrap_or(1),
        });
    }

    if verbose {
        ui::status(format!(
            "Wrote {}",
            destination.join("Cargo.lock").display()
        ));
    }

    Ok(())
}
