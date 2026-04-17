#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/*.rs");
}

#[test]
fn both_features_compile_fail() {
    let output = std::process::Command::new("cargo")
        .arg("check")
        .current_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/compile-fail-both-features"
        ))
        .output()
        .expect("failed to run cargo check");

    assert!(
        !output.status.success(),
        "expected compilation to fail with both features enabled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mutually exclusive"),
        "expected 'mutually exclusive' error, got:\n{stderr}"
    );
}

/// End-to-end check that a contract with short-path handlers round-trips
/// through the macro and compiles.
///
/// This is the specific failure mode Defect 3 exposed: validator-only unit
/// tests accepted `Vec<u8>` / `Error` but the splicer didn't re-emit the
/// user's `use` items into the generated `data_driver` submodule, so
/// expansion failed with `cannot find type 'Error' in this scope`. If the
/// re-emit logic (filtering, rename preservation, path emission) regresses,
/// this test fails with a compiler error pointing at the fixture — not a
/// silent success that defers the bug to downstream integration.
#[test]
fn short_paths_compile_pass() {
    let output = std::process::Command::new("cargo")
        .arg("check")
        .current_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/compile-pass-short-paths"
        ))
        .output()
        .expect("failed to run cargo check");

    assert!(
        output.status.success(),
        "short-path handler fixture failed to compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
