mod support;

use support::{SETTINGS_ENV, invoke};

#[test]
fn help_and_version_work_without_database_settings() {
    for (label, arguments, expected) in [
        ("help-root", vec!["--help"], "Usage: darmok <COMMAND>"),
        (
            "help-init",
            vec!["init", "--help"],
            "--database-url-env <ENV_NAME>",
        ),
        (
            "help-schema",
            vec!["schema", "--help"],
            "Usage: darmok schema <COMMAND>",
        ),
        (
            "help-verify",
            vec!["schema", "verify", "--help"],
            "--database-url-env <ENV_NAME>",
        ),
    ] {
        let output = invoke(label, &arguments, None);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
    }
    let version = invoke("version", &["--version"], None);
    assert_eq!(version.status.code(), Some(0), "{version:?}");
    assert!(version.stderr.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout),
        concat!("darmok ", env!("CARGO_PKG_VERSION"), "\n")
    );
}

#[test]
fn incomplete_or_unimplemented_commands_are_usage_errors() {
    for (label, arguments) in [
        ("usage-root", vec![]),
        ("usage-init", vec!["init"]),
        ("usage-schema", vec!["schema"]),
        ("usage-verify", vec!["schema", "verify"]),
        ("usage-empty-name", vec!["init", "--database-url-env", ""]),
        ("usage-serve", vec!["serve"]),
    ] {
        let output = invoke(label, &arguments, None);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn settings_errors_are_reported_before_connecting() {
    let init = ["init", "--database-url-env", SETTINGS_ENV];
    let verify = ["schema", "verify", "--database-url-env", SETTINGS_ENV];
    for (label, arguments, settings, expected) in [
        (
            "settings-missing-init",
            init.as_slice(),
            None,
            format!("environment variable {SETTINGS_ENV} is not set"),
        ),
        (
            "settings-missing-verify",
            verify.as_slice(),
            None,
            format!("environment variable {SETTINGS_ENV} is not set"),
        ),
        (
            "settings-invalid",
            init.as_slice(),
            Some("not a connection string"),
            format!(
                "invalid PostgreSQL connection settings in environment variable {SETTINGS_ENV}"
            ),
        ),
        (
            "settings-keyword-no-database",
            init.as_slice(),
            Some("host=localhost user=postgres"),
            format!(
                "PostgreSQL connection settings in {SETTINGS_ENV} must select an explicit database"
            ),
        ),
        (
            "settings-uri-no-database",
            verify.as_slice(),
            Some("postgresql://localhost"),
            format!(
                "PostgreSQL connection settings in {SETTINGS_ENV} must select an explicit database"
            ),
        ),
    ] {
        let output = invoke(label, arguments, settings);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("darmok: {expected}\n")
        );
    }
}
