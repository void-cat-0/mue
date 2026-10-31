use std::{path::PathBuf, process::Command};

fn check_fixture(bin: &str, should_succeed: bool, expected_error: Option<&str>) {
    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/theme-consumer/Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--manifest-path"])
        .arg(&manifest)
        .args(["--bin", bin])
        .output()
        .expect("run downstream cargo check");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.success(),
        should_succeed,
        "downstream check for {bin} had unexpected status\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout),
    );
    if let Some(expected_error) = expected_error {
        assert!(
            stderr.contains(expected_error),
            "expected {expected_error:?}\n{stderr}"
        );
    }
}

#[test]
fn theme_and_class_macros_compile_from_renamed_external_crate() {
    check_fixture("expand", true, None);
}

#[test]
fn invalid_macro_inputs_fail_across_crate_boundary() {
    for (bin, message) in [
        ("class_invalid_expr", "class name"),
        ("class_invalid_const", "class name"),
        ("missing_option", "unknown"),
        ("non_bool_predicate", "bool"),
        ("private_options", "private"),
        ("old_match", "selector"),
    ] {
        check_fixture(bin, false, Some(message));
    }
}
