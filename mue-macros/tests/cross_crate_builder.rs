use std::{path::PathBuf, process::Command};

fn check_fixture(bin: &str, should_succeed: bool, expected_errors: &[&str]) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ordinary-consumer/Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            if should_succeed { "run" } else { "check" },
            "--offline",
            "--manifest-path",
        ])
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
    for &expected_error in expected_errors {
        assert!(
            stderr.contains(expected_error),
            "expected {expected_error:?}\n{stderr}"
        );
    }
}

#[test]
fn ordinary_builder_macros_build_from_renamed_external_crate() {
    check_fixture("expand", true, &[]);
}

#[test]
fn removed_theme_attributes_fail_across_crate_boundary() {
    for bin in ["removed_function_attribute", "removed_parameter_attribute"] {
        check_fixture(bin, false, &["theme attribute is no longer supported"]);
    }
}

#[test]
fn injected_context_and_reserved_parameters_have_clear_diagnostics_across_crates() {
    check_fixture(
        "invalid_build_parameter",
        false,
        &[
            "#[node] functions require an injected first parameter",
            "mismatched types",
            "expected `bool`, found `NodeContext`",
            "the injected context parameter cannot have #[default] or #[model] attributes",
            "style is reserved for the generated builder style field",
            "default expressions cannot reference parameters",
        ],
    );
}

#[test]
fn rust_checks_injected_context_and_node_return_types() {
    check_fixture(
        "wrong_build_type",
        false,
        &["mismatched types", "expected `u32`, found `NodeContext`"],
    );
    check_fixture(
        "wrong_return_type",
        false,
        &["mismatched types", "expected `Node`, found `u32`"],
    );
}
