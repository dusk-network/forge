use std::{fs, path::PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::{TempDir, tempdir};

fn scaffold_project(name: &str) -> (TempDir, PathBuf) {
    let tmp = tempdir().expect("tempdir");

    cargo_bin_cmd!("dusk-forge")
        .args([
            "new",
            name,
            "--no-git",
            "--path",
            tmp.path().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    let project = tmp.path().join(name);
    (tmp, project)
}

#[test]
fn clean_removes_contract_artifacts() {
    let (_tmp, project) = scaffold_project("clean-me");

    let contract_dir = project.join("target/contract/wasm32-unknown-unknown/release");
    let data_driver_dir = project.join("target/data-driver/wasm32-unknown-unknown/release");

    fs::create_dir_all(&contract_dir).expect("create contract target dir");
    fs::create_dir_all(&data_driver_dir).expect("create data-driver target dir");
    fs::write(contract_dir.join("clean_me.wasm"), b"contract").expect("write contract wasm");
    fs::write(data_driver_dir.join("clean_me.wasm"), b"driver").expect("write data-driver wasm");

    cargo_bin_cmd!("dusk-forge")
        .args(["clean", "--path", project.to_str().expect("utf-8 path")])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Cleaned target/contract and target/data-driver",
        ));

    assert!(!project.join("target/contract").exists());
    assert!(!project.join("target/data-driver").exists());
}

#[test]
fn completions_generates_bash_output() {
    cargo_bin_cmd!("dusk-forge")
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dusk-forge"))
        .stdout(predicate::str::contains("expand"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn expand_help_describes_data_driver_mode() {
    cargo_bin_cmd!("dusk-forge")
        .args(["expand", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cargo-expand"))
        .stdout(predicate::str::contains("--data-driver"));
}
