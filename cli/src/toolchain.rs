use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::{CliError, Result};
use crate::tools;

pub const WASM_TARGET: &str = "wasm32-unknown-unknown";

#[derive(Debug, Clone)]
pub struct ToolchainStatus {
    pub channel: String,
    pub installed: bool,
    pub wasm_target: bool,
    pub rust_src: bool,
    pub wasm_opt: Option<PathBuf>,
}

pub fn configured_channel(project_dir: &Path) -> Result<String> {
    let toolchain_file = resolve_toolchain_file(project_dir).ok_or_else(|| {
        CliError::Message(format!(
            "missing rust-toolchain.toml (or rust-toolchain) in {} or its parents",
            project_dir.display()
        ))
    })?;

    read_toolchain_channel(&toolchain_file).ok_or_else(|| {
        CliError::Message(format!(
            "unable to read toolchain channel from {}",
            toolchain_file.display()
        ))
    })
}

pub fn cargo_toolchain_arg(project_dir: &Path) -> Result<String> {
    Ok(format!("+{}", configured_channel(project_dir)?))
}

pub fn inspect(project_dir: &Path) -> Result<ToolchainStatus> {
    let channel = configured_channel(project_dir)?;

    let installed = command_success("rustc", &[&format!("+{channel}"), "--version"]);

    let wasm_target = command_contains(
        "rustup",
        &["target", "list", "--installed", "--toolchain", &channel],
        WASM_TARGET,
    );

    let rust_src = command_contains(
        "rustup",
        &["component", "list", "--installed", "--toolchain", &channel],
        "rust-src",
    );

    let wasm_opt = tools::find_in_path("wasm-opt");

    Ok(ToolchainStatus {
        channel,
        installed,
        wasm_target,
        rust_src,
        wasm_opt,
    })
}

pub fn ensure_build(project_dir: &Path, needs_rust_src: bool) -> Result<ToolchainStatus> {
    let status = inspect(project_dir)?;

    if !status.installed {
        return Err(CliError::Message(format!(
            "missing Rust toolchain '{}'. Install with: rustup toolchain install {}",
            status.channel, status.channel
        )));
    }

    if !status.wasm_target {
        return Err(CliError::Message(format!(
            "missing {WASM_TARGET} target for toolchain '{}'. Install with: rustup target add {WASM_TARGET} --toolchain {}",
            status.channel, status.channel
        )));
    }

    if needs_rust_src && !status.rust_src {
        return Err(CliError::Message(format!(
            "missing rust-src component for toolchain '{}'. Install with: rustup component add rust-src --toolchain {}",
            status.channel, status.channel
        )));
    }

    Ok(status)
}

fn resolve_toolchain_file(project_dir: &Path) -> Option<PathBuf> {
    for dir in project_dir.ancestors() {
        let toolchain_toml = dir.join("rust-toolchain.toml");
        if toolchain_toml.is_file() {
            return Some(toolchain_toml);
        }

        let toolchain_plain = dir.join("rust-toolchain");
        if toolchain_plain.is_file() {
            return Some(toolchain_plain);
        }
    }
    None
}

fn read_toolchain_channel(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    if path.file_name()?.to_str()? == "rust-toolchain.toml" {
        parse_toolchain_toml_channel(&content)
    } else {
        parse_toolchain_plain_channel(&content)
    }
}

fn parse_toolchain_plain_channel(content: &str) -> Option<String> {
    let value = content.lines().next()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_toolchain_toml_channel(content: &str) -> Option<String> {
    let value: toml::Value = toml::from_str(content).ok()?;
    value
        .get("toolchain")?
        .get("channel")?
        .as_str()
        .map(ToString::to_string)
}

fn command_success(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn command_contains(program: &str, args: &[&str], needle: &str) -> bool {
    let output = Command::new(program).args(args).output();
    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.lines().any(|line| line.contains(needle))
        }
        _ => false,
    }
}
