use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
mod config;
use config::Assertion;
pub use config::Config;

pub struct Examples {
    assertions: Vec<Assertion>,
}

impl Rule for Examples {
    const ID: &'static str = "markdown/examples";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            assertions: config.compile()?,
        })
    }
    fn configured(&self) -> bool {
        !self.assertions.is_empty()
    }
    fn check(&self, _: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for assertion in &self.assertions {
            for source in analysis.sources.iter().filter(|source| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|selector| selector.matches(&source.path))
            }) {
                assertion.inspect(source, &mut findings);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

impl Assertion {
    fn inspect(&self, source: &Source, findings: &mut Vec<Finding>) {
        let mut depth = 0;
        let mut titles = Vec::new();
        let mut cases = 0;
        for (event, range) in &source.events {
            match event {
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H1,
                    ..
                }) if depth == 0 => {
                    titles.push(range.clone());
                }
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H2,
                    ..
                }) if depth == 0 => {
                    cases += 1;
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => {
                    self.fence(source, range.clone(), findings);
                }
                _ => {}
            }
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => depth -= 1,
                _ => {}
            }
        }
        if self.require_title && !leading_title(source, &titles) {
            findings.push(self.finding(
                source,
                0..0,
                "Document needs exactly one leading level-one title".into(),
            ));
        }
        if cases < self.min_cases {
            findings.push(self.finding(
                source,
                0..0,
                format!(
                    "Document has {cases} level-two cases; minimum is {}",
                    self.min_cases
                ),
            ));
        }
    }

    fn fence(&self, source: &Source, range: std::ops::Range<usize>, findings: &mut Vec<Finding>) {
        if !self.require_closed_fences || closed(source, &range) {
            return;
        }
        findings.push(self.finding(source, range, "Example code fence is unclosed".into()));
    }

    fn finding(&self, source: &Source, range: std::ops::Range<usize>, message: String) -> Finding {
        Finding {
            rule: Examples::ID,
            path: source.path.clone(),
            span: Some(Span::new(&source.text, range)),
            related: Vec::new(),
            configuration: self.setting.clone(),
            message,
            instruction: "Use one leading title, level-two cases, and closed fenced examples."
                .into(),
        }
    }
}

fn leading_title(source: &Source, titles: &[std::ops::Range<usize>]) -> bool {
    let [title] = titles else {
        return false;
    };
    let metadata_end = match source.events.first() {
        Some((Event::Start(Tag::MetadataBlock(_)), _)) => source
            .events
            .iter()
            .position(|(event, _)| matches!(event, Event::End(TagEnd::MetadataBlock(_))))
            .map_or(source.events.len(), |position| position + 1),
        _ => 0,
    };
    source
        .events
        .iter()
        .skip(metadata_end)
        .find(|(event, _)| !matches!(event, Event::End(TagEnd::HtmlBlock)))
        .is_some_and(|(event, range)| {
            matches!(
                event,
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H1,
                    ..
                })
            ) && range == title
        })
}

fn closed(source: &Source, range: &std::ops::Range<usize>) -> bool {
    let block = &source.text[range.clone()];
    let mut lines = block.lines();
    let opening = lines.next().unwrap_or_default().trim_start();
    let Some(marker @ ('`' | '~')) = opening.chars().next() else {
        return false;
    };
    let size = opening.chars().take_while(|c| *c == marker).count();
    let Some(last) = lines.last() else {
        return false;
    };
    let closing = last.trim_start_matches(|c: char| c.is_whitespace() || c == '>');
    let count = closing.chars().take_while(|c| *c == marker).count();
    count >= size && closing[count..].trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(text: &str, fields: &str) -> Result<linter::Report, Error> {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("case.md"), text).unwrap();
        std::fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"markdown/examples\"]]\ntarget = '**/*.md'\n{fields}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<Examples>()?
            .check(root.path())
    }
    #[test]
    fn donor_contract_and_closed_case() {
        assert_eq!(
            check("# Example\n\n```rust\nfn open() {}\n", "")
                .unwrap()
                .findings
                .len(),
            2
        );
        assert!(
            check(
                "# Examples\n\n## One case\n\n```rust\nfn closed() {}\n```\n",
                ""
            )
            .unwrap()
            .findings
            .is_empty()
        );
    }
    #[test]
    fn parser_distinguishes_code_quotes_and_setext_headings() {
        let text = "Title\n=====\n\nCase\n----\n\n````md\n# Fake\n## Fake\n```\n````\n";
        assert!(check(text, "").unwrap().findings.is_empty());
        assert_eq!(
            check("# Title\n> ## Quoted\n\n```md\n## Code\n```", "")
                .unwrap()
                .findings
                .len(),
            1
        );
        assert_eq!(
            check("Introduction\n\n# Title\n\n## Case", "")
                .unwrap()
                .findings
                .len(),
            1
        );
    }
    #[test]
    fn fences_respect_marker_lengths_and_nested_containers() {
        for text in [
            "# Title\n## Case\n~~~rust\na\n~~~",
            "# Title\n## Case\n> ```\n> a\n> ```",
        ] {
            assert!(check(text, "").unwrap().findings.is_empty(), "{text}");
        }
        for text in [
            "# Title\n## Case\n````\na\n```",
            "# Title\n## Case\n~~~\na\n```",
        ] {
            assert_eq!(check(text, "").unwrap().findings.len(), 1, "{text}");
        }
    }
    #[test]
    fn skill_frontmatter_precedes_the_title_and_preserves_fence_offsets() {
        let metadata = "---\nname: code-review\ndescription: |\n  Review café changes.\n  # Not a \
            title\n---\n";
        let valid =
            format!("{metadata}# Code review\n\n## Example\n```rust\nfn main() {{}}\n```\n");
        assert!(check(&valid, "").unwrap().findings.is_empty());
        let open = format!("{metadata}# Code review\n\n## Example\n```rust\nfn main() {{}}\n");
        let findings = check(&open, "").unwrap().findings;
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("fence"));
        let span = findings[0].span.as_ref().unwrap();
        assert_eq!(span.line, 10);
        assert_eq!(span.start, open.find("```rust").unwrap());
    }
    #[test]
    fn metadata_does_not_hide_extra_titles_or_content() {
        for text in [
            "---\nname: example\n---\nIntroduction\n\n# Title\n## Case",
            "---\nname: example\n---\n# Title\n# Another\n## Case",
            "---\nname: example\n# Title\n## Case",
            "--\nname: example\n--\n# Title\n## Case",
            "Introduction\n\n---\nname: example\n---\n# Title\n## Case",
        ] {
            let findings = check(text, "").unwrap().findings;
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.message.contains("level-one title")),
                "{text}"
            );
        }
    }
    #[test]
    fn strict_configuration_and_exclusion() {
        assert!(
            check("", "exclude = '**/*.md'")
                .unwrap()
                .findings
                .is_empty()
        );
        assert!(
            check("# Title", "min_cases = 0")
                .unwrap()
                .findings
                .is_empty()
        );
        for fields in [
            "min_cases = -1",
            "unknown = true",
            "require_title = false\nmin_cases = 0\nrequire_closed_fences = false",
        ] {
            assert!(matches!(check("", fields), Err(Error::Configuration(_))));
        }
    }
}
