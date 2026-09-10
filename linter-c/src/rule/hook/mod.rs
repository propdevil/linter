use crate::{Analysis, Source};
use linter::{Error, Project, Rule, RuleResult, Status};
use std::collections::BTreeMap;
use tree_sitter::Node;
mod condition;
mod config;
mod corpus;
mod scan;
use config::Assertion;
pub use config::Config;
use corpus::{Context, Corpus};
use scan::FileScan;

pub struct TestState {
    assertions: Vec<Assertion>,
}
impl Rule for TestState {
    const ID: &'static str = "c/test-only-state";
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
            let mut corpus = Corpus::default();
            for source in &analysis.sources {
                let mut names = BTreeMap::new();
                symbols(source.syntax.root_node(), source, &mut names);
                let mut scan = FileScan {
                    path: &source.path,
                    source: source.text.as_bytes(),
                    macros: &assertion.macros,
                    corpus: &mut corpus,
                    names: &names,
                    locals: Vec::new(),
                };
                scan.walk(source.syntax.root_node(), &Context::default());
            }
            findings.extend(
                corpus
                    .findings(&assertion.setting)
                    .into_iter()
                    .filter(|finding| assertion.selected(&finding.path)),
            );
        }
        findings.sort_by(|left, right| {
            (
                &left.path,
                left.span.as_ref().map(|span| span.start),
                &left.configuration,
            )
                .cmp(&(
                    &right.path,
                    right.span.as_ref().map(|span| span.start),
                    &right.configuration,
                ))
        });
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn symbols(node: Node<'_>, source: &Source, names: &mut BTreeMap<String, String>) {
    if matches!(node.kind(), "declaration" | "function_definition") {
        let mut cursor = node.walk();
        let private = node.named_children(&mut cursor).any(|child| {
            child.kind() == "storage_class_specifier"
                && &source.text[child.byte_range()] == "static"
        });
        for identifier in node
            .children_by_field_name("declarator", &mut cursor)
            .filter_map(scan::declared_identifier)
        {
            let name = source.text[identifier.byte_range()].to_owned();
            if private {
                names.insert(name.clone(), format!("{}::{name}", source.path.display()));
            } else {
                names.entry(name.clone()).or_insert(name);
            }
        }
        return;
    }
    if node.kind() == "translation_unit" || node.kind().starts_with("preproc_") {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            symbols(child, source, names);
        }
    }
}

impl Assertion {
    fn selected(&self, path: &std::path::Path) -> bool {
        self.target.matches(path)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|exclude| exclude.matches(path))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::TestState>()?
            .check(root)
    }
    fn report(files: &[(&str, &str)]) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"c/test-only-state\"]]\ntarget = '**/*.c'\nmacros = ['HL_NATIVE_TEST_HOOKS']",
        )
        .unwrap();
        for (name, source) in files {
            fs::write(root.path().join(name), source).unwrap();
        }
        check(root.path()).unwrap()
    }
    fn findings(files: &[(&str, &str)]) -> Vec<linter::Finding> {
        report(files).findings
    }
    const WRITER: &str = "\
int g_member_bound;
void admit(void) { g_member_bound = 1; }
";

    #[test]
    fn production_predicate_on_state_written_only_behind_a_test_hook_is_reported() {
        let result = findings(&[
            (
                "writer.c",
                &format!(
                    "{WRITER}\n#if defined(HL_NATIVE_TEST_HOOKS)\nvoid arm(void) {{ admi\
                t(); }}\n#endif\n"
                ),
            ),
            (
                "dump.c",
                "extern int g_member_bound;\nint dump(void) { if (!g_member_bound) { ret\
                urn -1; } return 0; }\n",
            ),
        ]);
        assert_eq!(result.len(), 1, "{result:#?}");
        assert_eq!(result[0].rule, "c/test-only-state");
        assert!(result[0].message.contains("g_member_bound"));
        assert_eq!(result[0].span.as_ref().unwrap().line, 2);
        assert!(
            result[0]
                .related
                .iter()
                .any(|related| related.message.contains("admit"))
        );
    }

    #[test]
    fn one_production_call_of_the_writer_clears_the_finding() {
        let result = findings(&[
            (
                "writer.c",
                &format!(
                    "{WRITER}\nvoid boot(void) {{ admit(); }}\n#if defined(HL_NATIVE_TES\
                T_HOOKS)\nvoid arm(void) {{ admit(); }}\n#endif\n"
                ),
            ),
            (
                "dump.c",
                "extern int g_member_bound;\nint dump(void) { if (!g_member_bound) { ret\
                urn -1; } return 0; }\n",
            ),
        ]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn a_writer_reached_only_through_a_test_only_caller_chain_is_still_test_only() {
        let result = findings(&[(
            "chain.c",
            &format!(
                "{WRITER}\nvoid stage(void) {{ admit(); }}\n#if defined(HL_NATIVE_TEST_H\
                OOKS)\nvoid arm(void) {{ stage(); }}\n#endif\nint dump(void) {{ return g\
                _member_bound ? 0 : -1; }}\n"
            ),
        )]);
        assert_eq!(result.len(), 1, "{result:#?}");
        assert!(result[0].message.contains("g_member_bound"));
    }

    #[test]
    fn test_only_predicates_and_test_only_symbols_alone_are_not_reported() {
        let result = findings(&[(
            "hooks.c",
            "static int g_probe;\n#if defined(HL_NATIVE_TEST_HOOKS)\nvoid arm(void) { g_\
                probe = 1; }\nint observe(void) { if (!g_probe) { return -1; } return 0;\
                \u{20}}\n#endif\n",
        )]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn the_production_branch_of_a_test_hook_conditional_is_production() {
        let result = findings(&[(
            "either.c",
            "static int g_ready;\n#if defined(HL_NATIVE_TEST_HOOKS)\nvoid arm(void) { g_\
                ready = 2; }\n#else\nvoid arm(void) { g_ready = 1; }\n#endif\nint use(vo\
                id) { return g_ready == 1; }\n",
        )]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn a_disjunction_that_holds_without_the_test_macro_is_not_test_only() {
        let result = findings(&[(
            "either.c",
            "static int g_ready;\n#if defined(HL_NATIVE_TEST_HOOKS) || defined(HL_DIAGNOSTICS)\n\
         void arm(void) { g_ready = 1; }\n#endif\nint use(void) { return g_ready == 1; }\n",
        )]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn state_carrying_a_real_production_initializer_is_not_unwritten() {
        let result = findings(&[(
            "seeded.c",
            "static int g_ready = 1;\n#if defined(HL_NATIVE_TEST_HOOKS)\nvoid arm(void) \
                { g_ready = 0; }\n#endif\nint use(void) { if (!g_ready) { return -1; } r\
                eturn 0; }\n",
        )]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn a_function_with_no_call_site_is_a_production_entry_point() {
        let result = findings(&[(
            "exported.c",
            "static int g_ready;\nvoid hl_admit(void) { g_ready = 1; }\nint use(void) { \
                return g_ready == 1; }\n",
        )]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn local_state_that_is_not_file_scope_is_not_tracked() {
        let result = findings(&[(
            "local.c",
            "#if defined(HL_NATIVE_TEST_HOOKS)\nvoid arm(void) { int ready = 1; (void)re\
                ady; }\n#endif\nint use(void) { int ready = 0; return ready == 1; }\n",
        )]);
        assert!(result.is_empty(), "{result:#?}");
    }

    #[test]
    fn a_production_writer_that_only_takes_the_address_is_a_writer() {
        let result = findings(&[
            (
                "table.c",
                "\
struct entry *g_table;
static int g_capacity;
void reserve(void **storage, int *capacity);
void scan(void) { reserve((void **)&g_table, &g_capacity); }
#if defined(HL_NATIVE_TEST_HOOKS)
void arm(void) { g_table = 0; }
#endif
",
            ),
            (
                "read.c",
                "extern struct entry *g_table;\nint use(void) { if (!g_table) { return -\
                1; } return 0; }\n",
            ),
        ]);
        assert!(
            result.is_empty(),
            "handing a callee the address of state writes it: {result:#?}"
        );
    }

    #[test]
    fn a_production_write_through_a_subscript_and_member_is_a_writer() {
        let result = findings(&[
            (
                "table.c",
                "\
struct entry g_table[8];
void fill(int index) { g_table[index].viable = 1; }
#if defined(HL_NATIVE_TEST_HOOKS)
void arm(void) { g_table[0].viable = 0; }
#endif
",
            ),
            (
                "read.c",
                "extern struct entry g_table[8];\nint use(void) { if (!g_table[0].viable\
                ) { return -1; } return 0; }\n",
            ),
        ]);
        assert!(
            result.is_empty(),
            "an element member assignment writes the state it names: {result:#?}"
        );
    }

    #[test]
    fn a_reader_no_production_call_site_reaches_is_not_a_production_predicate() {
        let result = findings(&[(
            "resolve.c",
            "\
static int g_state;
static int resolve(void) { if (g_state) { return g_state; } g_state = 1; return g_state; }
#if defined(HL_NATIVE_TEST_HOOKS)
void arm(void) { (void)resolve(); }
#endif
",
        )]);
        assert!(
            result.is_empty(),
            "an unreachable reader leaves no production branch to be wrong: {result:#?}"
        );
    }
    #[test]
    fn inverted_and_unknown_macro_conditions_are_classified_conservatively() {
        for (condition, expected) in [
            ("#ifdef HL_NATIVE_TEST_HOOKS", 1),
            ("#if defined(HL_NATIVE_TEST_HOOKS) && UNKNOWN", 1),
            ("#ifndef HL_NATIVE_TEST_HOOKS", 0),
            ("#if !defined(HL_NATIVE_TEST_HOOKS)", 0),
            ("#if UNKNOWN", 0),
            ("#if HL_NATIVE_TEST_HOOKS || UNKNOWN", 0),
        ] {
            let source = format!(
                "static int state;\n{condition}\nvoid arm(void) {{ state = 1; }}\n#endif\
                \nint read(void) {{ return state != 0; }}"
            );
            assert_eq!(
                findings(&[("state.c", &source)]).len(),
                expected,
                "{condition}"
            );
        }
        let source = "static int state;\n#ifndef HL_NATIVE_TEST_HOOKS\nint noop;\n#else\
            \nvoid arm(void) { state = 1; }\n#endif\nint read(void) { return state != 0;\
            \u{20}}";
        assert_eq!(findings(&[("state.c", source)]).len(), 1);
    }
    #[test]
    fn local_and_parameter_shadowing_cannot_read_or_write_global_state() {
        let prefix = "static int state;\n#ifdef HL_NATIVE_TEST_HOOKS\nvoid arm(void) { s\
            tate = 1; }\n#endif\n";
        for reader in [
            "int read(int state) { return state != 0; }",
            "int read(void) { int state = 0; return state != 0; }",
        ] {
            assert!(findings(&[("state.c", &format!("{prefix}{reader}"))]).is_empty());
        }
        let source = format!(
            "{prefix}void write(void) {{ int state; state = 2; }}\nint read(void) {{ ret\
                urn state != 0; }}"
        );
        assert_eq!(findings(&[("state.c", &source)]).len(), 1);
        let source = format!(
            "{prefix}int read(void) {{ for (int state = 0; state < 1; state++) {{}} retu\
                rn state != 0; }}"
        );
        assert_eq!(findings(&[("state.c", &source)]).len(), 1);
    }
    #[test]
    fn static_state_and_static_helpers_are_isolated_per_file() {
        let writer = "static int state;\n#ifdef HL_NATIVE_TEST_HOOKS\nvoid arm(void) { s\
            tate = 1; }\n#endif\n";
        assert!(
            findings(&[
                ("writer.c", writer),
                (
                    "reader.c",
                    "extern int state; int read(void) { return state != 0; }"
                )
            ])
            .is_empty()
        );
        let first = "static int state; static void change(void) { state = 1; }\n#ifdef H\
            L_NATIVE_TEST_HOOKS\nvoid arm(void) { change(); }\n#endif\nint read(void) { \
            return state != 0; }";
        let second = "static int state; static void change(void) { state = 1; } void boo\
            t(void) { change(); }";
        assert_eq!(
            findings(&[("first.c", first), ("second.c", second)]).len(),
            1
        );
    }
    #[test]
    fn evidence_and_directives_attach_to_production_predicate() {
        let source = "static int state;\n#ifdef HL_NATIVE_TEST_HOOKS\nvoid arm(void) { s\
            tate = 1; }\n#endif\nint read(void) {\n// linter:disable c/test-only-state -\
            - compatibility test state intentionally observed\nreturn state != 0;\n}";
        let result = report(&[("state.c", source)]);
        assert!(result.findings.is_empty());
        assert_eq!(result.suppressed.len(), 1);
        let source = source.replace(
            "// linter:disable c/test-only-state -- compatibilit\
            y test state intentionally observed\n",
            "",
        );
        let result = findings(&[("state.c", &source)]);
        assert_eq!(result[0].related.len(), 2);
        assert!(result[0].related[0].message.contains("declared"));
        assert!(result[0].related[1].message.contains("arm"));
    }
    #[test]
    fn validates_configuration_and_uses_unselected_writers_as_evidence() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "target = '*'",
            "target = '*'\nmacros = []",
            "target = '*'\nmacros = ['bad-name']",
            "target = []\nmacros = ['TEST']",
            "target = '*'\nmacros = ['TEST']\nexclude = []",
            "target = '*'\nmacros = ['TEST']\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"c/test-only-state\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"c/test-only-state\"]]\ntarget = ['reader.c']\nmacros = ['TEST']",
        )
        .unwrap();
        fs::write(
            root.path().join("writer.c"),
            "int state;\n#ifdef TEST\nvoid arm(void) { state = 1; }\n#endif\n",
        )
        .unwrap();
        fs::write(
            root.path().join("reader.c"),
            "extern int state; int read(void) { return state != 0; }",
        )
        .unwrap();
        assert_eq!(check(root.path()).unwrap().findings.len(), 1);
        fs::write(
            root.path().join("writer.c"),
            "int state; void write(void) { state = 1; }",
        )
        .unwrap();
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
}
