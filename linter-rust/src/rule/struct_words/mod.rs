use std::fs;

use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use tree_sitter::Node;

use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};

mod config;
pub use config::Config;

pub struct StructWords(Vec<Assertion>);

impl Rule for StructWords {
    const ID: &'static str = "rust/struct-word-count";
    type Analysis = Analysis;
    type Config = Config;

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }

    fn configured(&self) -> bool {
        !self.0.is_empty()
    }

    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root =
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    &tests,
                    assertion,
                    &mut findings,
                );
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn inspect(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "struct_item" {
        let test = tests[node.start_byte()];
        let selected = match assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        };
        if let Some(name) = node.child_by_field_name("name").filter(|_| selected) {
            let name = &source.text[name.byte_range()];
            let count = name_words(name).len();
            if count > assertion.max_words {
                findings.push(Finding {
                    span: Some(linter::Span::new(&source.text, node.byte_range())),
                    related: Vec::new(),
                    rule: StructWords::ID,
                    path: source.path.clone(),
                    configuration: assertion.setting.clone(),
                    message: format!(
                        "Rust struct `{name}` has {count} semantic words; maximum is {}",
                        assertion.max_words
                    ),
                    instruction: "Remove repeated module context and choose a concise struct name."
                        .into(),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}

fn name_words(name: &str) -> Vec<String> {
    let digit_start = name
        .trim_end_matches(|value: char| value.is_ascii_digit())
        .len();
    let name = if digit_start < name.len()
        && name
            .as_bytes()
            .get(digit_start.saturating_sub(1))
            .is_some_and(|value| *value == b'V' || *value == b'v')
    {
        &name[..digit_start - 1]
    } else {
        name
    };
    let chars = name.chars().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut start = 0;
    for index in 1..chars.len() {
        let boundary = (chars[index].is_ascii_uppercase()
            && (chars[index - 1].is_ascii_lowercase()
                || chars
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_lowercase())))
            || (!chars[index - 1].is_ascii_digit() && chars[index].is_ascii_digit());
        if boundary {
            output.push(
                chars[start..index]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
            );
            start = index;
        }
    }
    output.push(
        chars[start..]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase(),
    );
    output.into_iter().filter(|word| !word.is_empty()).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(path: &str, text: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/struct-word-count\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<StructWords>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    #[test]
    fn preserves_donor_semantic_tokenization() {
        assert_eq!(name_words("HTTPServerV2"), ["http", "server"]);
        assert_eq!(name_words("RecordV1"), ["record"]);
        assert_eq!(
            name_words("BitcoinRPCClientV1"),
            ["bitcoin", "rpc", "client"]
        );
        assert_eq!(name_words("SHA256Hash"), ["sha", "256", "hash"]);
        assert_eq!(name_words("Recordv12"), ["record"]);
        assert_eq!(name_words("RecordV"), ["record", "v"]);
    }
    #[test]
    fn checks_named_unit_tuple_structs_but_not_other_declarations() {
        let found=check("lib.rs","struct ThreeWordRecord{x:u8} struct AnotherLongName; struct ThirdLongName(u8); struct RecordV2{x:u8} trait ThreeWordTrait{} enum ThreeWordEnum{A} type ThreeWordAlias=u8;", "").unwrap();
        assert_eq!(found.len(), 3);
        assert!(
            found
                .iter()
                .all(|finding| finding.message.contains("3 semantic words; maximum is 2"))
        );
    }
    #[test]
    fn threshold_scope_and_exclusions_are_observable() {
        let text = "struct ThreeWordRecord; #[cfg(test)] mod tests {struct AnotherLongName;}";
        assert_eq!(check("src/lib.rs", text, "").unwrap().len(), 1);
        assert!(check("src/lib.rs", text, "max_words=3").unwrap().is_empty());
        assert_eq!(check("src/lib.rs", text, "scope='all'").unwrap().len(), 2);
        assert_eq!(check("src/lib.rs", text, "scope='tests'").unwrap().len(), 1);
        assert!(
            check("src/lib.rs", text, "exclude='src/**'")
                .unwrap()
                .is_empty()
        );
        assert!(check("tests/input.rs", text, "").unwrap().is_empty());
        assert_eq!(
            check("tests/input.rs", text, "scope='tests'")
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn reports_precise_span_and_accepts_reasoned_directive() {
        let text = "struct ThreeWordRecord;";
        let findings = check("lib.rs", text, "").unwrap();
        let span = findings[0].span.as_ref().unwrap();
        assert_eq!(&text[span.start..span.end], text);
        let suppressed = format!(
            "// linter:disable rust/struct-word-count -- external protocol fixes type name\n{text}"
        );
        assert!(check("lib.rs", &suppressed, "").unwrap().is_empty());
    }
    #[test]
    fn rejects_invalid_configuration() {
        for config in [
            "max_words=0",
            "max_words=-1",
            "max_words='two'",
            "scope='unknown'",
            "exclude=[]",
            "unknown=true",
        ] {
            assert!(
                matches!(check("lib.rs", "", config), Err(Error::Configuration(_))),
                "{config}"
            );
        }
    }
    #[test]
    fn own_implementation_has_concise_struct_names() {
        for source in [include_str!("mod.rs"), include_str!("config.rs")] {
            assert!(check("lib.rs", source, "").unwrap().is_empty());
        }
    }
}
