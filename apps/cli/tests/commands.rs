use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn check(root: &Path, json: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_linter"));
    command.arg("check").arg(root);
    if json {
        command.arg("--json");
    }
    command.output().unwrap()
}

#[test]
fn validates_then_reports_missing_rule_file_with_exit_one() {
    let root = tempfile::tempdir().unwrap();
    let policy = r#"
[[rules.layout]]
target = "rules/*"
files.required = ["mod.rs", "config.rs", "readme.md"]
"#;
    fs::write(root.path().join("linter.toml"), policy).unwrap();
    let rule = root.path().join("rules/layout");
    fs::create_dir_all(&rule).unwrap();
    for file in ["mod.rs", "config.rs", "readme.md"] {
        fs::write(rule.join(file), "").unwrap();
    }
    let clean = check(root.path(), false);
    assert_eq!(clean.status.code(), Some(0));
    assert!(
        String::from_utf8(clean.stdout)
            .unwrap()
            .contains("layout: 0 finding(s)\n")
    );

    fs::remove_file(rule.join("readme.md")).unwrap();
    let failed = check(root.path(), true);
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stderr.is_empty());
    let actual: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(
        actual,
        serde_json::to_value(
            linter::register(linter::Registry::default())
                .and_then(linter_rust::register)
                .and_then(linter_c::register)
                .and_then(linter_markdown::register)
                .unwrap()
                .check(root.path())
                .unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        actual["findings"][0]["message"],
        "missing required file readme.md"
    );
    let text = String::from_utf8(check(root.path(), false).stdout).unwrap();
    assert!(text.contains("Create file rules/layout/readme.md."));
    assert_eq!(
        fs::read_to_string(root.path().join("linter.toml")).unwrap(),
        policy
    );
    assert!(!rule.join("readme.md").exists());
}

#[test]
fn invalid_configuration_and_missing_root_exit_two() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("linter.toml"),
        "[rules.typo]\nenabled = true",
    )
    .unwrap();
    let output = check(root.path(), true);
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["error"].as_str().unwrap().contains("unknown rule"));
    let missing = check(&root.path().join("absent"), false);
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    assert!(!missing.stderr.is_empty());
}

#[test]
fn disabled_and_unconfigured_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    let output = check(root.path(), true);
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["rules"]["layout"], "unconfigured");
    fs::write(
        root.path().join("linter.toml"),
        "[rules.layout]\nenabled = false",
    )
    .unwrap();
    let output = check(root.path(), true);
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["rules"]["layout"], "disabled");
}

#[test]
fn embeds_presets_and_initializes_without_overwriting() {
    for (preset, expected) in [
        ("default", include_str!("../../../configs/default.toml")),
        ("rust", include_str!("../../../configs/rust.toml")),
    ] {
        let root = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_linter"))
            .current_dir(root.path())
            .args(["config", preset])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(!root.path().join("linter.toml").exists());

        let output = Command::new(env!("CARGO_BIN_EXE_linter"))
            .current_dir(root.path())
            .args(["init", preset])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            fs::read_to_string(root.path().join("linter.toml")).unwrap(),
            expected
        );
        fs::write(root.path().join("Cargo.toml"), "[workspace]").unwrap();
        for directory in [".git", "target"] {
            fs::create_dir(root.path().join(directory)).unwrap();
            fs::write(root.path().join(directory).join("ignored.md"), "").unwrap();
        }
        assert!(check(root.path(), true).status.success());

        fs::write(
            root.path().join("linter.toml"),
            "owner's existing configuration",
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_linter"))
            .args(["init", preset])
            .arg(root.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(
            fs::read_to_string(root.path().join("linter.toml")).unwrap(),
            "owner's existing configuration"
        );
    }
}

#[test]
fn markdown_policy_produces_cli_errors_with_instructions() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("linter.toml"),
        include_str!("../../../configs/default.toml"),
    )
    .unwrap();
    fs::write(root.path().join("extra.md"), "").unwrap();
    let output = check(root.path(), true);
    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["findings"][0]["rule"], "layout");
    assert!(
        value["findings"][0]["instruction"]
            .as_str()
            .unwrap()
            .contains("purpose")
    );
}

#[test]
fn removed_rules_are_configuration_errors_even_when_disabled() {
    let root = tempfile::tempdir().unwrap();
    let configuration = r#"
[rules.markdown]
enabled = true

[[rules.markdown.config.files]]
target = "docs/*.md"
description = "Documents the project goal, linting rules, and their configuration."
"#;
    for enabled in [true, false] {
        fs::write(
            root.path().join("linter.toml"),
            configuration.replace("enabled = true", &format!("enabled = {enabled}")),
        )
        .unwrap();
        let output = check(root.path(), true);
        assert_eq!(output.status.code(), Some(2));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            value["error"]
                .as_str()
                .unwrap()
                .contains("unknown rule \"markdown\"")
        );
        assert!(value.get("findings").is_none());
        let output = check(root.path(), false);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("unknown rule \"markdown\"")
        );
    }
}
