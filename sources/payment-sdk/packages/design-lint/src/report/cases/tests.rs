use std::{collections::BTreeMap, fs, path::Path};

use super::{Cases, MARKER, Staging};
use crate::{
    Severity,
    report::{
        Reporter,
        tests::{BrokenOutput, example},
    },
    test_support::Fixture,
};

fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .expect("queue")
        .map(|entry| {
            let entry = entry.expect("entry");
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("case bytes"),
            )
        })
        .collect()
}

#[test]
fn replaces_only_marked_cases_and_renders_warnings_and_evidence() {
    let fixture = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let root = fixture.path().join("lint");
    for queue in ["errors", "check"] {
        fs::create_dir_all(root.join(queue)).expect("queue");
        fs::write(root.join(queue).join(".gitkeep"), "keep").expect("gitkeep");
        fs::write(root.join(queue).join("notes.md"), "human review").expect("notes");
        fs::write(
            root.join(queue).join("stale.md"),
            format!("{MARKER}old case"),
        )
        .expect("old report");
    }
    let mut reporter = Cases::with_output(root.clone(), Vec::new());
    reporter.begin(&fixture.workspace()).expect("begin");
    assert!(
        root.join("errors/stale.md").exists(),
        "begin must not delete old cases"
    );
    reporter
        .finding(&example(Severity::Warning))
        .expect("warning");
    reporter.finding(&example(Severity::Error)).expect("error");
    reporter.finish(&[]).expect("publish");
    for queue in ["errors", "check"] {
        assert_eq!(
            fs::read_to_string(root.join(queue).join("notes.md")).expect("notes"),
            "human review"
        );
        assert_eq!(
            fs::read_to_string(root.join(queue).join(".gitkeep")).expect("gitkeep"),
            "keep"
        );
        assert!(!root.join(queue).join("stale.md").exists());
    }
    let generated: Vec<_> = files(&root.join("errors"))
        .into_values()
        .filter(|bytes| bytes.starts_with(MARKER.as_bytes()))
        .collect();
    assert_eq!(generated.len(), 2);
    let text = generated
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes))
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "Severity: `warning`",
        "Severity: `error`",
        "call site",
        "Owner: Wallet",
        "`.address`",
        "Does the receiver own the behavior?",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[test]
fn names_are_stable_across_order_and_checkout_roots() {
    let left = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let right = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let mut names = Vec::new();
    for (fixture, reverse) in [(&left, false), (&right, true)] {
        let mut reporter = Cases::with_output(fixture.path().join("lint"), Vec::new());
        reporter.begin(&fixture.workspace()).expect("begin");
        let mut findings = [example(Severity::Warning), example(Severity::Error)];
        if reverse {
            findings.reverse();
        }
        for mut finding in findings {
            finding.location.path = fixture
                .path()
                .canonicalize()
                .expect("canonical fixture")
                .join("src/wallet.rs");
            reporter.finding(&finding).expect("render");
        }
        names.push(reporter.pending.keys().cloned().collect::<Vec<_>>());
    }
    assert_eq!(names[0], names[1]);
    assert!(
        names[0]
            .iter()
            .all(|name| !name.contains('/') && !name.contains('\\'))
    );
}

#[test]
fn output_failure_before_replacement_preserves_previous_cases_and_notes() {
    let fixture = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let root = fixture.path().join("lint");
    fs::create_dir_all(root.join("errors")).expect("queue");
    fs::write(root.join("errors/previous.md"), format!("{MARKER}previous")).expect("old case");
    fs::write(root.join("errors/notes.md"), "human review").expect("notes");
    let before = files(&root.join("errors"));
    let mut reporter = Cases::with_output(root.clone(), BrokenOutput);
    reporter.begin(&fixture.workspace()).expect("begin");
    reporter.finding(&example(Severity::Error)).expect("render");
    assert!(reporter.finish(&[]).is_err());
    assert_eq!(files(&root.join("errors")), before);
    assert!(fs::read_dir(&root).expect("root").all(|entry| {
        !entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .starts_with(".design-lint-stage")
    }));
}

#[test]
fn directory_failure_before_staging_keeps_previous_reports() {
    let fixture = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let root = fixture.path().join("lint");
    fs::create_dir_all(root.join("errors")).expect("queue");
    fs::write(root.join("errors/previous.md"), format!("{MARKER}previous")).expect("old case");
    fs::write(root.join("check"), "user owned file").expect("collision");
    let before = files(&root.join("errors"));
    let mut reporter = Cases::with_output(root.clone(), Vec::new());
    reporter.begin(&fixture.workspace()).expect("begin");
    reporter.finding(&example(Severity::Error)).expect("render");
    assert!(reporter.finish(&[]).is_err());
    assert_eq!(files(&root.join("errors")), before);
    assert_eq!(
        fs::read_to_string(root.join("check")).expect("collision"),
        "user owned file"
    );
}

#[test]
fn filename_collision_with_an_unmarked_note_is_rejected_without_changes() {
    let fixture = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let root = fixture.path().join("lint");
    let mut reporter = Cases::with_output(root.clone(), Vec::new());
    reporter.begin(&fixture.workspace()).expect("begin");
    let finding = example(Severity::Error);
    let name = reporter.name(&finding);
    fs::create_dir_all(root.join("errors")).expect("queue");
    fs::write(root.join("errors").join(name), "human review").expect("note");
    let before = files(&root.join("errors"));
    reporter.finding(&finding).expect("render");
    assert!(reporter.finish(&[]).is_err());
    assert_eq!(files(&root.join("errors")), before);
}

#[test]
fn partial_publication_failure_restores_previous_cases() {
    let fixture = Fixture::new(&[("src/lib.rs", "pub struct Wallet;")]);
    let root = fixture.path().join("lint");
    fs::create_dir_all(root.join("errors")).expect("queue");
    fs::create_dir_all(root.join("check")).expect("queue");
    fs::write(root.join("errors/previous.md"), format!("{MARKER}previous")).expect("old case");
    let before = files(&root.join("errors"));
    let mut stage = Staging::new(&root).expect("staging");
    fs::write(stage.path.join("new/first.md"), format!("{MARKER}new")).expect("stage first");
    let names = ["first.md".to_owned(), "missing.md".to_owned()];
    assert!(
        stage
            .publish(&root, &["errors/previous.md".into()], names.iter())
            .is_err()
    );
    assert_eq!(files(&root.join("errors")), before);
}

#[test]
fn case_identity_is_stable_for_repository_and_focused_scans() {
    let fixture = Fixture::new(&[
        ("lint.toml", ""),
        (
            "src/wallet.rs",
            "struct Wallet; impl Wallet { fn wallet_address(&self) {} }",
        ),
    ]);
    let policy = crate::Policy::default();
    let roots = [
        fixture.path().to_owned(),
        fixture.path().join("src"),
        fixture.path().join("src/wallet.rs"),
    ];
    let mut names = Vec::new();
    for root in roots {
        let workspace =
            crate::source::Workspace::load(vec![root], &policy.source).expect("workspace");
        let mut reporter = Cases::with_output(fixture.path().join("lint"), Vec::new());
        reporter.begin(&workspace).expect("begin");
        let mut finding = example(Severity::Error);
        finding.location.path = fixture
            .path()
            .join("src/wallet.rs")
            .canonicalize()
            .expect("source");
        names.push(reporter.name(&finding));
    }
    assert_eq!(names[0], names[1]);
    assert_eq!(names[0], names[2]);
}
