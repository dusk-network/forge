//! Compile-pass harness.
//!
//! Builds the `tests/compile-pass/` sub-crate via `cargo check` and asserts
//! success. Each `.rs` file under `tests/compile-pass/src/` pins one valid
//! contract shape end-to-end (macro expansion + downstream type-check).
//!
//! The sub-crate pattern is used (instead of trybuild's `compile_pass`
//! glob) because the macro emits a `compile_error!` when neither
//! `contract` nor `data-driver` is enabled, and trybuild has no
//! per-fixture feature configuration. The sub-crate sets the `contract`
//! feature by default and mirrors the topic structure of
//! `tests/compile-fail/`.

#[test]
fn compile_pass_tests() {
    let output = std::process::Command::new("cargo")
        .arg("check")
        .arg("--all-targets")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/compile-pass"))
        .output()
        .expect("failed to run cargo check");

    assert!(
        output.status.success(),
        "compile-pass fixtures failed to build:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn compile_pass_data_driver_tests() {
    let output = std::process::Command::new("cargo")
        .arg("check")
        .arg("--all-targets")
        .arg("--no-default-features")
        .arg("--features")
        .arg("data-driver")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/compile-pass"))
        .output()
        .expect("failed to run cargo check");

    assert!(
        output.status.success(),
        "compile-pass data-driver fixtures failed to build:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
