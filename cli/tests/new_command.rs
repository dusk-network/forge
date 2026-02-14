use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

#[test]
fn new_scaffolds_counter_template() {
    let tmp = tempdir().expect("tempdir");

    cargo_bin_cmd!("dusk-forge")
        .args([
            "new",
            "my-test",
            "--no-git",
            "--path",
            tmp.path().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    let project = tmp.path().join("my-test");
    assert!(project.join("Cargo.toml").exists());
    assert!(project.join("src/lib.rs").exists());
    assert!(project.join("tests/contract.rs").exists());
    assert!(project.join("rust-toolchain.toml").exists());
    assert!(project.join("Cargo.lock").exists());

    let cargo = fs::read_to_string(project.join("Cargo.toml")).expect("read Cargo.toml");
    let lib = fs::read_to_string(project.join("src/lib.rs")).expect("read lib.rs");
    let test = fs::read_to_string(project.join("tests/contract.rs")).expect("read test file");
    let rust_toolchain =
        fs::read_to_string(project.join("rust-toolchain.toml")).expect("read rust-toolchain.toml");

    assert!(cargo.contains("name = \"my-test\""));
    assert!(!cargo.contains("YOUR_CONTRACT_NAME"));
    assert!(lib.contains("mod my_test"));
    assert!(lib.contains("pub struct MyTest"));
    assert!(test.contains("release/my_test.wasm"));
    assert!(!test.contains("YOUR_CONTRACT_NAME"));
    assert!(!test.contains("TODO"));
    assert!(rust_toolchain.contains("channel = \"nightly-2024-07-30\""));
}

#[test]
fn new_scaffolds_empty_template() {
    let tmp = tempdir().expect("tempdir");

    cargo_bin_cmd!("dusk-forge")
        .args([
            "new",
            "blank-contract",
            "--template",
            "empty",
            "--no-git",
            "--path",
            tmp.path().to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    let project = tmp.path().join("blank-contract");
    let lib = fs::read_to_string(project.join("src/lib.rs")).expect("read lib.rs");
    let test = fs::read_to_string(project.join("tests/contract.rs")).expect("read test file");
    assert!(project.join("rust-toolchain.toml").exists());
    assert!(project.join("Cargo.lock").exists());

    assert!(lib.contains("mod blank_contract"));
    assert!(lib.contains("pub struct BlankContract"));
    assert!(!lib.contains("CountChanged"));
    assert!(!test.contains("TODO"));
}
