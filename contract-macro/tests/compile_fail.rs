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
