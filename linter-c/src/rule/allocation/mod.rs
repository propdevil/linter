use crate::Analysis;
use linter::{Error, Project, Rule, RuleResult, Status};
mod config;
mod flow;
use config::Assertion;
pub use config::Config;

pub struct Allocation {
    assertions: Vec<Assertion>,
}
impl Rule for Allocation {
    const ID: &'static str = "c/unchecked-allocation";
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
        for source in &analysis.sources {
            for assertion in self.assertions.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                flow::inspect(source, assertion, &mut findings);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::Allocation>()?
            .check(root)
    }
    fn run(source: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"c/unchecked-allocation\"]]\ntarget = '**/*.c'\nfunctions = ['allocate']",
        )
        .unwrap();
        fs::write(root.path().join("allocation.c"), source).unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn accepts_prior_guards_unused_allocations_and_sizeof() {
        for body in [
            "int *p = allocate(4); if (!p) return; p[0] = 1;",
            "int *p = allocate(4); if (p) { *p = 1; }",
            "int *p = allocate(4); if (p != NULL) { *p = 1; }",
            "int *p = allocate(4); if (NULL == p) return; *p = 1;",
            "int *p = allocate(sizeof *p); if (!p || initialize(p) != 0) return; p[0] = 1;",
            "int *p = allocate(4); consume(p);",
            "int *p = allocate(4); if (p && *p) consume(p);",
            "int *p = allocate(4); int *q = p; if (!q) return; *p = 1;",
            "int *p = allocate(4); { int *p; consume(p); } if (!p) return; *p = 1;",
        ] {
            assert!(
                run(&format!("void run(void) {{ {body} }}"))
                    .findings
                    .is_empty(),
                "{body}"
            );
        }
    }
    #[test]
    fn reports_late_conditional_unrelated_and_invalidated_guards() {
        for body in [
            "int *p = allocate(4); *p = 1;",
            "int *p = allocate(4); p[0] = 1; if (!p) return;",
            "int *p = allocate(4); if (requested && !p) return; *p = 1;",
            "int *p = allocate(4); if (p) consume(p); *p = 1;",
            "int *p = allocate(4); if (requested) { if (!p) return; } *p = 1;",
            "int *p = allocate(4); if (!p) consume(p); *p = 1;",
            "int *p = allocate(4); if (!p) return; p = allocate(4); *p = 1;",
            "int *p = allocate(4); int *q = p; *q = 1;",
            "int *p = allocate(4); if (p || *p) consume(p);",
            "int *p = allocate(4); /* if (!p) return; */ *p = 1;",
        ] {
            let report = run(&format!("void run(int requested) {{ {body} }}"));
            assert_eq!(report.findings.len(), 1, "{body}");
            assert_eq!(report.findings[0].related.len(), 1);
            assert!(
                report.findings[0].span.as_ref().unwrap().start
                    < report.findings[0].related[0].span.as_ref().unwrap().start
            );
        }
    }
    #[test]
    fn handles_assignments_casts_fields_and_shadowing() {
        for body in [
            "int *p; p = (int *)allocate(4); *p = 1;",
            "struct Value *p = allocate(4); p->field = 1;",
            "int *p = allocate(4); if (!p) return; { int *p = allocate(4); *p = 1; }",
        ] {
            assert_eq!(
                run(&format!("void run(void) {{ {body} }}")).findings.len(),
                1,
                "{body}"
            );
        }
        assert!(
            run("void run(void) { int *p = other_allocate(4); *p = 1; }")
                .findings
                .is_empty()
        );
    }
    #[test]
    fn directives_suppress_allocation_and_stale_directives_fail() {
        let report = run(
            "void run(void) {\n// linter:disable c/unchecked-allocation -- allocator aborts on exhaustion\nint *p = allocate(4);\n*p = 1;\n}",
        );
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.suppressed.len(), 1);
        let report = run(
            "void run(void) {\n// linter:disable c/unchecked-allocation -- allocator aborts on exhaustion\nint *p = allocate(4);\nif (!p) return;\n*p = 1;\n}",
        );
        assert!(!report.findings.is_empty());
    }
    #[test]
    fn configuration_and_target_exclusions_are_enforced() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "target = '*'",
            "target = '*'\nfunctions = []",
            "target = '*'\nfunctions = ['bad-name']",
            "target = []\nfunctions = ['allocate']",
            "target = '*'\nfunctions = ['allocate']\nexclude = []",
            "target = '*'\nfunctions = ['allocate']\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"c/unchecked-allocation\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
        fs::write(root.path().join("linter.toml"), "[[rules.\"c/unchecked-allocation\"]]\ntarget = ['*.c']\nexclude = 'skip.c'\nfunctions = ['allocate']").unwrap();
        fs::write(
            root.path().join("skip.c"),
            "void run(void) { int *p = allocate(4); *p = 1; }",
        )
        .unwrap();
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
}
