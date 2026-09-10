use std::{io, path::PathBuf};

use super::{Diagnostic, Markdown, Reporter};
use crate::{Finding, Location, Related, Review, Severity, Summary};

pub(super) fn example(severity: Severity) -> Finding {
    let location = Location {
        path: PathBuf::from("src/wallet.rs"),
        line: 7,
        column: 3,
        source: "fn wallet_address(&self) {}".into(),
    };
    let mut finding = match severity {
        Severity::Error => Finding::error(
            "receiver-name-repetition",
            "Wallet::wallet_address",
            location,
        ),
        Severity::Warning => Finding::warning("single-use-free-function", "decode", location),
    };
    finding.message = "review the owner".into();
    finding.help = "use the existing domain receiver".into();
    finding.related.push(Related {
        label: "call site".into(),
        location: Location {
            path: PathBuf::from("src/api.rs"),
            line: 11,
            column: 2,
            source: "wallet.wallet_address()".into(),
        },
    });
    finding.review = Some(Review {
        metadata: vec![("Owner".into(), "Wallet".into())],
        dependencies: vec![".address".into()],
        questions: vec!["Does the receiver own the behavior?".into()],
    });
    finding
}

#[test]
fn diagnostics_render_both_severities_and_all_evidence() {
    let mut reporter = Diagnostic::new(Vec::new());
    reporter
        .finding(&example(Severity::Warning))
        .expect("warning");
    reporter.finding(&example(Severity::Error)).expect("error");
    reporter
        .finish(&[Summary {
            rule: "receiver-name-repetition",
            severity: Severity::Error,
            findings: 1,
        }])
        .expect("summary");
    let text = String::from_utf8(reporter.into_inner()).expect("UTF-8 report");
    for expected in [
        "warning[single-use-free-function]",
        "error[receiver-name-repetition]",
        "src/api.rs:11:2",
        "wallet.wallet_address()",
        "Owner: Wallet",
        "dependency: .address",
        "Does the receiver own the behavior?",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[test]
fn markdown_contains_all_evidence_and_safe_source_fences() {
    let mut finding = example(Severity::Warning);
    finding.location.source = "const TEXT: &str = \"````\";".into();
    let mut reporter = Markdown::new(Vec::new());
    reporter.finding(&finding).expect("finding");
    reporter.finish(&[]).expect("summary");
    let text = String::from_utf8(reporter.into_inner()).expect("UTF-8 report");
    for expected in [
        "Severity: `warning`",
        "Related: call site",
        "src/api.rs:11:2",
        "Owner: Wallet",
        "`.address`",
        "Does the receiver own the behavior?",
        "`````rust",
        "## Summary",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[test]
fn empty_markdown_still_has_heading_and_summary() {
    let mut reporter = Markdown::new(Vec::new());
    reporter.finish(&[]).expect("summary");
    let text = String::from_utf8(reporter.into_inner()).expect("UTF-8 report");
    assert!(text.starts_with("# Linting review\n"));
    assert!(text.contains("## Summary"));
}

pub(super) struct BrokenOutput;

impl io::Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("injected output failure"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("injected output failure"))
    }
}

#[test]
fn reporter_output_errors_propagate() {
    let finding = example(Severity::Error);
    assert!(Diagnostic::new(BrokenOutput).finding(&finding).is_err());
    assert!(Markdown::new(BrokenOutput).finding(&finding).is_err());
}

#[test]
fn all_reporters_display_project_relative_locations_for_each_scan_scope() {
    let fixture = crate::test_support::Fixture::new(&[
        (
            "Payment-SDK/Cargo.toml",
            "[workspace]\nmembers = [\"sdk/wallets\", \"apps/api\"]\nresolver = \"3\"\n",
        ),
        (
            "Payment-SDK/sdk/wallets/Cargo.toml",
            "[package]\nname = \"fixture-wallets\"\nversion = \"0.1.0\"\n",
        ),
        ("Payment-SDK/sdk/wallets/src/lib.rs", "pub struct Wallet;"),
        (
            "Payment-SDK/apps/api/Cargo.toml",
            "[package]\nname = \"fixture-api\"\nversion = \"0.1.0\"\n",
        ),
        ("Payment-SDK/apps/api/src/lib.rs", "pub struct Api;"),
    ]);
    let root = fixture
        .path()
        .join("Payment-SDK")
        .canonicalize()
        .expect("project");
    let mut finding = example(Severity::Error);
    finding.location.path = root.join("sdk/wallets/src/lib.rs");
    finding.related[0].location.path = root.join("apps/api/src/lib.rs");
    let original = finding.clone();
    let mut names = Vec::new();
    for (index, scan) in [
        root.clone(),
        root.join("sdk/wallets"),
        root.join("sdk/wallets/src/lib.rs"),
    ]
    .into_iter()
    .enumerate()
    {
        let workspace =
            crate::source::Workspace::load(vec![scan], &crate::Policy::default().source)
                .expect("workspace");
        let mut diagnostic = Diagnostic::new(Vec::new());
        diagnostic.begin(&workspace).expect("diagnostic context");
        diagnostic.finding(&finding).expect("diagnostic finding");
        let mut markdown = Markdown::new(Vec::new());
        markdown.begin(&workspace).expect("Markdown context");
        markdown.finding(&finding).expect("Markdown finding");
        let cases_root = fixture.path().join(format!("cases-{index}"));
        let mut cases = super::Cases::with_output(cases_root.clone(), Vec::new());
        cases.begin(&workspace).expect("case context");
        cases.finding(&finding).expect("case finding");
        cases.finish(&[]).expect("publish case");
        let case = std::fs::read_dir(cases_root.join("errors"))
            .expect("cases")
            .next()
            .expect("one case")
            .expect("case entry");
        names.push(case.file_name());
        for output in [
            diagnostic.into_inner(),
            markdown.into_inner(),
            std::fs::read(case.path()).expect("case content"),
        ] {
            let text = String::from_utf8(output).expect("UTF-8 output");
            assert!(
                text.contains("Payment-SDK/sdk/wallets/src/lib.rs:7:3"),
                "missing project primary path: {text}"
            );
            assert!(
                text.contains("Payment-SDK/apps/api/src/lib.rs:11:2"),
                "missing project related path: {text}"
            );
            assert!(
                !text.contains(&root.parent().expect("project parent").display().to_string()),
                "private prefix remained: {text}"
            );
        }
        assert_eq!(
            finding, original,
            "formatting must preserve canonical paths"
        );
    }
    assert_eq!(names[0], names[1]);
    assert_eq!(names[0], names[2]);
}

#[test]
fn display_paths_use_the_most_specific_known_project() {
    let roots = [
        PathBuf::from("/checkout/Outer"),
        PathBuf::from("/checkout/Outer/Inner"),
        PathBuf::from("/elsewhere/Other"),
    ];
    assert_eq!(
        super::display_path(
            std::path::Path::new("/checkout/Outer/Inner/src/lib.rs"),
            &roots
        ),
        PathBuf::from("Inner/src/lib.rs")
    );
    assert_eq!(
        super::display_path(std::path::Path::new("/checkout/Outer/src/lib.rs"), &roots),
        PathBuf::from("Outer/src/lib.rs")
    );
    assert_eq!(
        super::display_path(std::path::Path::new("/elsewhere/Other/src/lib.rs"), &roots),
        PathBuf::from("Other/src/lib.rs")
    );
}

#[test]
fn display_paths_preserve_relative_and_unknown_locations() {
    let roots = [PathBuf::from("/checkout/Project")];
    for path in [
        "src/wallet.rs",
        "Project/src/wallet.rs",
        "/unknown/src/wallet.rs",
        "/checkout/Project-other/src/wallet.rs",
        "/checkout/Project/../outside.rs",
    ] {
        let path = std::path::Path::new(path);
        assert_eq!(super::display_path(path, &roots), path);
    }
}
