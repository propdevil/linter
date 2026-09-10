use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Project {
    root: PathBuf,
}
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "design-lint-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.0.0'",
        )
        .unwrap();
        Self { root }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_design-lint"))
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn policy_precedence_and_failures_are_observable() {
    let project = Project::new();
    fs::write(
        project.root.join("src/lib.rs"),
        "struct Selected { id: u8 }",
    )
    .unwrap();
    assert!(
        project.run(&["check", "."]).status.success(),
        "standalone defaults"
    );
    fs::write(
        project.root.join("lint.toml"),
        "[rules]\nenabled=['struct-noun-naming']",
    )
    .unwrap();
    let diagnostic = project.run(&["check", "."]);
    assert!(!diagnostic.status.success());
    assert!(
        String::from_utf8(diagnostic.stderr)
            .unwrap()
            .contains("error[struct-noun-naming]")
    );
    fs::write(project.root.join("explicit.toml"), "").unwrap();
    assert!(
        project
            .run(&["--policy", "explicit.toml", "check", "."])
            .status
            .success()
    );
    assert!(
        !project
            .run(&["--policy", "missing.toml", "check", "."])
            .status
            .success()
    );
    fs::write(project.root.join("lint.toml"), "[rules]\nenabled=['typo']").unwrap();
    assert!(!project.run(&["check", "."]).status.success());
}
#[test]
fn warnings_are_visible_without_error_exit_and_cases_remain_a_review_command() {
    let project = Project::new();
    fs::write(
        project.root.join("src/lib.rs"),
        "fn next(value: u8) -> u8 { value + 1 }\npub fn total() -> u8 { next(1) }",
    )
    .unwrap();
    fs::write(
        project.root.join("lint.toml"),
        "[rules]\nenabled=['single-use-free-function']",
    )
    .unwrap();
    let warning = project.run(&["check", "."]);
    assert!(warning.status.success());
    assert!(
        String::from_utf8(warning.stderr)
            .unwrap()
            .contains("warning[single-use-free-function]")
    );
    let markdown = project.run(&["--markdown", "."]);
    assert!(markdown.status.success());
    assert!(
        String::from_utf8(markdown.stdout)
            .unwrap()
            .contains("Severity: `warning`")
    );
    fs::write(
        project.root.join("src/lib.rs"),
        "struct Selected { id: u8 }",
    )
    .unwrap();
    fs::write(
        project.root.join("lint.toml"),
        "[rules]\nenabled=['struct-noun-naming']",
    )
    .unwrap();
    assert!(project.run(&["--cases", "lint", "."]).status.success());
    assert_eq!(
        fs::read_dir(project.root.join("lint/errors"))
            .unwrap()
            .count(),
        1
    );
}
