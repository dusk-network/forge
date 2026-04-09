#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{env, fs};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::{TempDir, tempdir};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod +x");
}

fn shell_quote(path: &Path) -> String {
    let escaped = path.to_string_lossy().replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn resolve_program(program: &str) -> PathBuf {
    env::split_paths(&env::var_os("PATH").expect("PATH env var"))
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("unable to resolve {program} on PATH"))
}

fn create_project() -> (TempDir, PathBuf) {
    let tmp = tempdir().expect("tempdir");
    let project = tmp.path().join("smoke-contract");

    fs::create_dir_all(project.join("src")).expect("create src");
    fs::create_dir_all(project.join("tests")).expect("create tests");

    let cargo_toml = format!(
        r#"[package]
name = "smoke-contract"
version = "0.1.0"
edition = "2021"

[target.'cfg(target_family = "wasm")'.dependencies]
dusk-forge = {{ path = "{}" }}

[features]
contract = []
data-driver = []
data-driver-js = ["data-driver"]

[lib]
crate-type = ["cdylib"]

[profile.release]
overflow-checks = true
"#,
        repo_root().display()
    );

    fs::write(project.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    fs::write(project.join("src/lib.rs"), "pub fn host_only() {}\n").expect("write lib.rs");
    fs::write(
        project.join("tests/contract.rs"),
        "#[test]\nfn smoke() {}\n",
    )
    .expect("write test file");
    fs::write(
        project.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"nightly-2024-07-30\"\n",
    )
    .expect("write rust-toolchain.toml");
    fs::write(project.join("Cargo.lock"), "").expect("write Cargo.lock");

    (tmp, project)
}

struct FakeTools {
    _tmp: TempDir,
    bin_dir: PathBuf,
    log_path: PathBuf,
}

impl FakeTools {
    fn new() -> Self {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");

        let log_path = tmp.path().join("tool.log");
        let real_cargo = env::var_os("CARGO")
            .map(PathBuf::from)
            .unwrap_or_else(|| resolve_program("cargo"));
        let real_rustc = resolve_program("rustc");

        let cargo_script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail

toolchain=""
if [[ "${{1-}}" == +* ]]; then
  toolchain="$1"
  shift
fi

subcmd="${{1-}}"
if [[ -z "$subcmd" ]]; then
  exit 1
fi
shift || true

case "$subcmd" in
  metadata|generate-lockfile)
    if [[ -n "$toolchain" ]]; then
      exec {real_cargo} "$toolchain" "$subcmd" "$@"
    else
      exec {real_cargo} "$subcmd" "$@"
    fi
    ;;
  build)
    manifest=""
    feature=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --manifest-path)
          manifest="$2"
          shift 2
          ;;
        --features)
          feature="$2"
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done

    crate_name="$(sed -n 's/^name = \"\(.*\)\"$/\1/p' "$manifest" | head -n1)"
    crate_name="${{crate_name//-/_}}"
    wasm_path="${{CARGO_TARGET_DIR}}/wasm32-unknown-unknown/release/${{crate_name}}.wasm"
    mkdir -p "$(dirname "$wasm_path")"
    printf '%s\n' "$feature" > "$wasm_path"
    printf 'subcmd=build toolchain=%s feature=%s target_dir=%s manifest=%s\n' \
      "$toolchain" "$feature" "${{CARGO_TARGET_DIR-}}" "$manifest" >> {log_path}
    exit 0
    ;;
  test)
    printf 'subcmd=test toolchain=%s args=%s\n' \
      "$toolchain" "$*" >> {log_path}
    exit 0
    ;;
  *)
    if [[ -n "$toolchain" ]]; then
      exec {real_cargo} "$toolchain" "$subcmd" "$@"
    else
      exec {real_cargo} "$subcmd" "$@"
    fi
    ;;
esac
"#,
            real_cargo = shell_quote(&real_cargo),
            log_path = shell_quote(&log_path),
        );

        let rustc_script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail

if [[ "${{1-}}" == +* && "${{2-}}" == "--version" ]]; then
  echo "rustc 1.82.0-nightly (fake toolchain)"
  exit 0
fi

exec {real_rustc} "$@"
"#,
            real_rustc = shell_quote(&real_rustc),
        );

        let rustup_script = r#"#!/usr/bin/env bash
set -euo pipefail

if [[ "${1-}" == "target" && "${2-}" == "list" && "${3-}" == "--installed" ]]; then
  echo "wasm32-unknown-unknown"
  exit 0
fi

if [[ "${1-}" == "component" && "${2-}" == "list" && "${3-}" == "--installed" ]]; then
  echo "rust-src"
  exit 0
fi

exit 0
"#;

        let wasm_opt_script = r#"#!/usr/bin/env bash
set -euo pipefail

input=""
output=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -o)
      output="$2"
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      input="$1"
      shift
      ;;
  esac
done

if [[ -n "$input" && -n "$output" ]]; then
  if [[ "$input" != "$output" ]]; then
    cp "$input" "$output"
  fi
fi
"#;

        write_executable(&bin_dir.join("cargo"), &cargo_script);
        write_executable(&bin_dir.join("rustc"), &rustc_script);
        write_executable(&bin_dir.join("rustup"), rustup_script);
        write_executable(&bin_dir.join("wasm-opt"), wasm_opt_script);

        Self {
            _tmp: tmp,
            bin_dir,
            log_path,
        }
    }

    fn path(&self) -> OsString {
        let mut paths = vec![self.bin_dir.clone()];
        paths.extend(env::split_paths(
            &env::var_os("PATH").expect("PATH env var"),
        ));
        env::join_paths(paths).expect("join PATH")
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log_path).unwrap_or_default()
    }
}

#[test]
fn check_succeeds_for_valid_project() {
    let (_tmp, project) = create_project();
    let tools = FakeTools::new();

    cargo_bin_cmd!("dusk-forge")
        .args(["check", "--path", project.to_str().expect("utf-8 path")])
        .env("PATH", tools.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("All checks passed"));
}

#[test]
fn build_creates_contract_and_data_driver_artifacts() {
    let (_tmp, project) = create_project();
    let tools = FakeTools::new();

    cargo_bin_cmd!("dusk-forge")
        .args(["build", "--path", project.to_str().expect("utf-8 path")])
        .env("PATH", tools.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("contract wasm:"))
        .stderr(predicate::str::contains("data-driver wasm:"));

    assert!(
        project
            .join("target/contract/wasm32-unknown-unknown/release/smoke_contract.wasm")
            .exists()
    );
    assert!(
        project
            .join("target/data-driver/wasm32-unknown-unknown/release/smoke_contract.wasm")
            .exists()
    );

    let log = tools.log();
    assert!(log.contains("subcmd=build toolchain=+nightly-2024-07-30 feature=contract"));
    assert!(log.contains("subcmd=build toolchain=+nightly-2024-07-30 feature=data-driver-js"));
    assert!(log.contains("target/contract"));
    assert!(log.contains("target/data-driver"));
}

#[test]
fn test_builds_contract_and_runs_cargo_test() {
    let (_tmp, project) = create_project();
    let tools = FakeTools::new();

    cargo_bin_cmd!("dusk-forge")
        .args([
            "test",
            "--path",
            project.to_str().expect("utf-8 path"),
            "--",
            "--quiet",
        ])
        .env("PATH", tools.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Tests completed"));

    assert!(
        project
            .join("target/contract/wasm32-unknown-unknown/release/smoke_contract.wasm")
            .exists()
    );

    let log = tools.log();
    assert!(log.contains("subcmd=build toolchain=+nightly-2024-07-30 feature=contract"));
    assert!(log.contains("subcmd=test toolchain=+nightly-2024-07-30"));
    assert!(log.contains("--quiet"));
}
