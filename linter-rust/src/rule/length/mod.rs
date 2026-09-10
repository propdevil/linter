use std::fs;

use crate::scope::{integration, mark_tests};
use linter::{Error, Finding, Project, Rule, RuleResult, Status};

use crate::{Analysis, Source};
mod config;
pub use config::Config;

pub struct FileLength {
    max_lines: usize,
    selector: linter::Selector,
}

impl Rule for FileLength {
    const ID: &'static str = "rust/file-length";
    type Analysis = Analysis;
    type Config = Config;

    fn new(config: Config) -> Result<Self, Error> {
        let config = config.validate()?;
        Ok(Self {
            max_lines: config.max_lines,
            selector: config.target.compile("rust/file-length.target", true)?,
        })
    }

    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root =
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let mut findings = Vec::new();
        for source in analysis
            .sources
            .iter()
            .filter(|source| self.selector.matches(&source.path))
        {
            if integration(source, &root, analysis) {
                continue;
            }
            let lines = production_lines(source);
            if lines > self.max_lines {
                findings.push(Finding {
                    rule: Self::ID,
                    path: source.path.clone(),
                    configuration: "rules.\"rust/file-length\".config.max_lines".into(),
                    message: format!("Rust file has {lines} production lines ({} total); maximum is {}", source.text.lines().count(), self.max_lines),
                    instruction: "Split production code by cohesive responsibility. Test code is already excluded; do not use include! or numbered fragments to evade the limit.".into(),
                });
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn production_lines(source: &Source) -> usize {
    let mut excluded = vec![false; source.text.len()];
    mark_tests(source.syntax.root_node(), &source.text, &mut excluded);
    let mut offset = 0;
    source
        .text
        .split_inclusive('\n')
        .filter(|line| {
            let mask = &excluded[offset..offset + line.len()];
            offset += line.len();
            // A line containing any production token or comment still counts.
            line.bytes()
                .zip(mask)
                .any(|(byte, excluded)| !excluded && !byte.is_ascii_whitespace())
                || !mask.iter().any(|excluded| *excluded)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(text: &str) -> usize {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let syntax = parser.parse(text, None).unwrap();
        assert!(!syntax.root_node().has_error());
        production_lines(&Source {
            path: "lib.rs".into(),
            text: text.into(),
            syntax,
        })
    }

    #[test]
    fn excludes_test_modules_functions_and_their_attributes() {
        assert_eq!(
            count("fn production() {}\n#[cfg(test)]\nmod tests {\n\n#[test]\nfn check() {}\n}\n"),
            1
        );
        assert_eq!(count("#[test]\nfn check() {}\nfn production() {}\n"), 1);
        assert_eq!(count("#![cfg(test)]\nfn helper() {}\n"), 0);
        assert_eq!(count("mod tests {\n#![cfg(test)]\nfn helper() {}\n}\n"), 0);
        assert_eq!(
            count("impl Example {\n#[cfg(test)]\nfn helper() {}\n}\n"),
            2
        );
    }

    #[test]
    fn counts_production_on_shared_lines_and_ignores_nested_tests_once() {
        assert_eq!(count("fn production() {} #[cfg(test)] mod tests {}\n"), 1);
        assert_eq!(
            count("#[cfg(test)] mod tests { #[test] fn check() {} } fn production() {}"),
            1
        );
        assert_eq!(
            count("#[cfg(test)]\nmod tests {\n#[cfg(test)]\nmod nested {}\n}\n"),
            0
        );
        assert_eq!(
            count("fn production() {}\r\n#[cfg(test)]\r\nmod tests {}\r\n"),
            1
        );
    }

    #[test]
    fn cfg_exclusion_requires_test_under_every_production_configuration() {
        for predicate in [
            "test",
            "all(test, feature = \"extra\")",
            "any(test, all(test, unix))",
            "not(not(test))",
        ] {
            assert_eq!(
                count(&format!("#[cfg({predicate})]\nfn helper() {{}}\n")),
                0,
                "{predicate}"
            );
        }
        for predicate in [
            "not(test)",
            "any(test, feature = \"extra\")",
            "all(unix, feature = \"extra\")",
        ] {
            assert_eq!(
                count(&format!("#[cfg({predicate})]\nfn production() {{}}\n")),
                2,
                "{predicate}"
            );
        }
    }

    #[test]
    fn counts_comments_blanks_declarations_and_attribute_text_in_strings() {
        assert_eq!(
            count("// comment\n\nstruct Value;\nconst TEXT: &str = \"#[cfg(test)]\";\n"),
            4
        );
        assert_eq!(count(""), 0);
        assert_eq!(count("\n"), 1);
        assert_eq!(count("// #[cfg(test)]\nfn production() {}\n"), 2);
    }
    #[test]
    fn production_budget_ignores_five_hundred_lines_of_tests() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            "[rules.\"rust/file-length\".config]\nmax_lines = 600",
        )
        .unwrap();
        let production = "// production\n".repeat(500);
        let tests = format!(
            "#[cfg(test)]\nmod tests {{\n{}}}\n",
            "// test\n".repeat(497)
        );
        std::fs::write(root.path().join("lib.rs"), format!("{production}{tests}")).unwrap();
        let registry = linter::Registry::default()
            .register::<FileLength>()
            .unwrap();
        assert!(registry.check(root.path()).unwrap().findings.is_empty());
        std::fs::write(
            root.path().join("lib.rs"),
            format!("{}{}", "// production\n".repeat(600), tests),
        )
        .unwrap();
        assert!(registry.check(root.path()).unwrap().findings.is_empty());
        std::fs::write(
            root.path().join("lib.rs"),
            format!("{}{}", "// production\n".repeat(601), tests),
        )
        .unwrap();
        let report = registry.check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].message,
            "Rust file has 601 production lines (1101 total); maximum is 600"
        );
    }

    #[test]
    fn ignores_crate_and_global_integration_sources_and_validates_configuration() {
        let root = tempfile::tempdir().unwrap();
        for path in ["src", "tests/support"] {
            std::fs::create_dir_all(root.path().join(path)).unwrap();
        }
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='example'\nversion='0.1.0'",
        )
        .unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        std::fs::write(
            root.path().join("tests/support/mod.rs"),
            "// tests\n".repeat(1000),
        )
        .unwrap();
        let registry = linter::Registry::default()
            .register::<FileLength>()
            .unwrap();
        assert!(registry.check(root.path()).unwrap().findings.is_empty());
        for config in [
            "max_lines = 0",
            "max_lines = -1",
            "max_lines = 'large'",
            "maximum = 500",
        ] {
            std::fs::write(
                root.path().join("linter.toml"),
                format!("[rules.\"rust/file-length\".config]\n{config}"),
            )
            .unwrap();
            assert!(matches!(
                registry.check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }
}
