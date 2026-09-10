use super::{repository, rust};
use crate::{
    Policy,
    policy::{Dependency, Layer, PackageLayer, Repository, Source, VocabularyOwner},
    source::Workspace,
};
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn fixture(files: &[(&str, &str)]) -> (std::path::PathBuf, Workspace) {
    let root = std::env::temp_dir().join(format!(
        "design-lint-focused-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let workspace = Workspace::load(vec![root.clone()], &Source::default()).unwrap();
    (root, workspace)
}

fn clean(root: std::path::PathBuf) {
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn renamed_and_target_dependencies_keep_package_identity() {
    for declaration in [
        "[dependencies]\nalias = { package = 'high', path = '../../sdk/high' }",
        "[target.'cfg(unix)'.dependencies]\nhigh = { path = '../../sdk/high' }",
    ] {
        let manifest = format!("[package]\nname='low'\nversion='0.0.0'\n{declaration}\n");
        let (root, workspace) = fixture(&[
            ("packages/low/Cargo.toml", &manifest),
            (
                "sdk/high/Cargo.toml",
                "[package]\nname='high'\nversion='0.0.0'\n",
            ),
        ]);
        let mut policy = Policy::default();
        policy.dependency.layers = vec![
            Layer {
                name: "low".into(),
                directory: Some("packages".into()),
                may_depend_on: vec![],
            },
            Layer {
                name: "high".into(),
                directory: Some("sdk".into()),
                may_depend_on: vec![],
            },
        ];
        let findings = repository::dependencies(&workspace, &policy).unwrap();
        assert_eq!(findings.len(), 1, "{declaration}");
        assert_eq!(findings[0].subject, "low -> high");
        clean(root);
    }
}

#[test]
fn vocabulary_checks_production_after_test_syntax_and_marker_text() {
    for source in [
        "#[cfg(test)] mod tests {}\npub struct BitcoinValue { value: u8 }",
        "const MARKER: &str = \"#[cfg(test)]\";\npub struct BitcoinValue { value: u8 }",
    ] {
        let (root, workspace) = fixture(&[("src/lib.rs", source)]);
        let mut policy = Policy::default();
        policy.vocabulary.owners.push(VocabularyOwner {
            words: vec!["bitcoin".into()],
            allowed_paths: vec!["sdk/chains/bitcoin/".into()],
        });
        let findings = repository::vocabulary(&workspace, &policy).unwrap();
        assert_eq!(findings.len(), 1, "{source}");
        assert_eq!(findings[0].rule, "owned-vocabulary");
        clean(root);
    }
}

#[test]
fn version_suffix_is_not_a_word() {
    assert_eq!(rust::name_words("HTTPServerV2"), ["http", "server"]);
    assert_eq!(rust::name_words("RecordV1"), ["record"]);
    assert_eq!(
        rust::name_words("BitcoinRPCClientV1"),
        ["bitcoin", "rpc", "client"]
    );
}

#[test]
fn api_rules_reject_only_requested_shapes() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        (
            "src/lib.rs",
            "trait Large { fn a(&self); fn b(&self); fn c(&self); fn d(&self); } struct Empty; struct ThreeWordRecord { x: u8 } struct RecordV2 { x: u8 }",
        ),
    ]);
    let policy = Policy::default();
    assert_eq!(
        rust::check_kind(&workspace, &policy, "traits")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        rust::check_kind(&workspace, &policy, "empty")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        rust::check_kind(&workspace, &policy, "names")
            .unwrap()
            .len(),
        1
    );
    clean(root);
}

#[test]
fn constructor_suppression_requires_reason() {
    let source = "struct Value(u8); impl Value {\n// design-lint: allow self-constructor-static -- consuming parser convention\nfn parse(self) -> Result<Self, ()> { Ok(self) }\n\n\nfn new(self) -> Self { self }\n}";
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", source),
    ]);
    let findings = rust::check_kind(&workspace, &Policy::default(), "constructors").unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].subject, "new");
    clean(root);
}

#[test]
fn vocabulary_understands_camel_case_and_ownership() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", "struct BitcoinClient { value: u8 }"),
    ]);
    let mut policy = Policy::default();
    policy.vocabulary.owners.push(VocabularyOwner {
        words: vec!["bitcoin".into(), "btc".into()],
        allowed_paths: vec!["sdk/chains/bitcoin/".into()],
    });
    assert_eq!(
        repository::vocabulary(&workspace, &policy).unwrap().len(),
        1
    );
    clean(root);
}

#[test]
fn solana_vocabulary_allows_only_owners_and_reasoned_alloy_suppressions() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        (
            "apps/api/src/lib.rs",
            "struct SolanaConfig { enabled: bool }",
        ),
        (
            "sdk/chains/solana/src/lib.rs",
            "struct SolanaClient { endpoint: String }",
        ),
        (
            "sdk/chains/ethereum/src/lib.rs",
            "// design-lint: allow owned-vocabulary -- standard Alloy Solidity ABI import\nuse alloy_sol_types::sol;\n// design-lint: allow owned-vocabulary -- standard Alloy Solidity ABI invocation\nsol! {}",
        ),
        (
            "sdk/indexing/src/lib.rs",
            "struct SolanaCursor { position: u64 }",
        ),
    ]);
    let mut policy = Policy::default();
    policy.vocabulary.owners.push(VocabularyOwner {
        words: vec!["solana".into(), "sol".into()],
        allowed_paths: vec!["apps/".into(), "sdk/chains/solana/".into()],
    });

    let findings = repository::vocabulary(&workspace, &policy).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].subject, "solana");
    assert!(
        findings[0]
            .location
            .path
            .ends_with("sdk/indexing/src/lib.rs")
    );
    clean(root);
}

#[test]
fn repository_policy_limits_solana_dependencies_and_vocabulary() {
    let policy_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("lint.toml");
    let policy = Policy::load(policy_path).expect("repository policy");
    let layer = policy
        .dependency
        .layers
        .iter()
        .find(|layer| layer.name == "solana-chain")
        .expect("Solana chain layer");
    assert_eq!(
        layer.may_depend_on,
        ["package", "base", "indexing", "wallets"]
    );
    let consumers = policy
        .dependency
        .layers
        .iter()
        .filter(|layer| {
            layer
                .may_depend_on
                .iter()
                .any(|name| name == "solana-chain")
        })
        .map(|layer| layer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(consumers, ["application", "acceptance"]);
    assert!(
        policy.dependency.package_layers.iter().any(|mapping| {
            mapping.package == "chain-solana" && mapping.layer == "solana-chain"
        })
    );
    let owner = policy
        .vocabulary
        .owners
        .iter()
        .find(|owner| owner.words == ["solana", "sol"])
        .expect("Solana vocabulary owner");
    assert_eq!(owner.allowed_paths, ["apps/", "sdk/chains/solana/"]);
}

#[test]
fn dependencies_follow_configured_layers() {
    let files = [
        (
            "packages/low/Cargo.toml",
            "[package]\nname='low'\nversion='0.0.0'\n[dependencies]\nhigh={path='../../sdk/high'}",
        ),
        ("packages/low/src/lib.rs", ""),
        (
            "sdk/high/Cargo.toml",
            "[package]\nname='high'\nversion='0.0.0'",
        ),
        ("sdk/high/src/lib.rs", ""),
    ];
    let (root, workspace) = fixture(&files);
    let policy = Policy {
        dependency: Dependency {
            ignored_packages: vec![],
            layers: vec![
                Layer {
                    name: "package".into(),
                    directory: Some("packages".into()),
                    may_depend_on: vec!["package".into()],
                },
                Layer {
                    name: "sdk".into(),
                    directory: Some("sdk".into()),
                    may_depend_on: vec!["package".into()],
                },
            ],
            package_layers: vec![PackageLayer {
                package: "high".into(),
                layer: "sdk".into(),
            }],
        },
        ..Policy::default()
    };
    assert_eq!(
        repository::dependencies(&workspace, &policy).unwrap().len(),
        1
    );
    clean(root);
}

#[test]
fn repository_limits_are_policy_owned() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", "fn a() {}\nfn b() {}"),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 1,
            forbidden_paths: vec![],
            ..Repository::default()
        },
        ..Policy::default()
    };
    assert_eq!(
        repository::file_length(&workspace, &policy).unwrap().len(),
        1
    );
    clean(root);
}

#[test]
fn file_length_ignores_inline_test_module_lines() {
    let mut source = "fn production() {}\n".repeat(500);
    source.push_str("#[cfg(test)]\nmod tests {\n");
    source.push_str(&"    // test-only coverage\n".repeat(997));
    source.push_str("}\n");
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", &source),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 500,
            ..Repository::default()
        },
        ..Policy::default()
    };
    assert!(
        repository::file_length(&workspace, &policy)
            .unwrap()
            .is_empty()
    );
    clean(root);
}

#[test]
fn file_length_counts_production_after_inline_test_module() {
    let source =
        "#[cfg(test)]\nmod tests {\n    fn scenario() {}\n}\nfn first() {}\nfn second() {}\n";
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", source),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 1,
            ..Repository::default()
        },
        ..Policy::default()
    };
    let findings = repository::file_length(&workspace, &policy).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("2 production lines"));
    clean(root);
}

#[test]
fn file_length_ignores_proven_test_only_items() {
    let source = "fn production() {}\n#[cfg(all(test, unix))]\nfn platform_test() {}\n#[test]\nfn direct_test() {}\n";
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", source),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 1,
            ..Repository::default()
        },
        ..Policy::default()
    };
    assert!(
        repository::file_length(&workspace, &policy)
            .unwrap()
            .is_empty()
    );
    clean(root);
}

#[test]
fn file_length_counts_items_that_can_compile_without_tests() {
    let source = "#[cfg(any(test, feature = \"support\"))]\nfn support() {}\n";
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", source),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 1,
            ..Repository::default()
        },
        ..Policy::default()
    };
    let findings = repository::file_length(&workspace, &policy).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("2 production lines"));
    clean(root);
}

#[test]
fn file_length_ignores_standalone_test_sources() {
    let test_source = "fn scenario() {}\n".repeat(20);
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", "fn production() {}\n"),
        ("src/lib_test.rs", &test_source),
        ("tests/workflow.rs", &test_source),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 1,
            ..Repository::default()
        },
        ..Policy::default()
    };
    assert!(
        repository::file_length(&workspace, &policy)
            .unwrap()
            .is_empty()
    );
    clean(root);
}

#[test]
fn file_length_does_not_treat_cfg_text_as_an_attribute() {
    let source = "const MARKER: &str = \"#[cfg(test)]\";\nfn production() {}\n";
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", source),
    ]);
    let policy = Policy {
        repository: Repository {
            maximum_rust_lines: 1,
            ..Repository::default()
        },
        ..Policy::default()
    };
    assert_eq!(
        repository::file_length(&workspace, &policy).unwrap().len(),
        1
    );
    clean(root);
}

#[test]
fn chain_layout_reports_missing_paths() {
    let (root, workspace) = fixture(&[
        (
            "sdk/chains/base/Cargo.toml",
            "[package]\nname='base'\nversion='0.0.0'",
        ),
        ("sdk/chains/base/src/lib.rs", ""),
        (
            "sdk/chains/coin/Cargo.toml",
            "[package]\nname='coin'\nversion='0.0.0'",
        ),
        ("sdk/chains/coin/src/lib.rs", ""),
    ]);
    let findings = repository::chain_layout(&workspace, &Policy::default()).unwrap();
    assert_eq!(findings.len(), 9);
    assert!(
        findings
            .iter()
            .all(|finding| finding.subject.starts_with("coin/"))
    );
    clean(root);
}

#[test]
fn chain_layout_rejects_file_form_complex_module() {
    let files = [
        (
            "sdk/chains/coin/Cargo.toml",
            "[package]\nname='coin'\nversion='0.0.0'",
        ),
        ("sdk/chains/coin/src/lib.rs", ""),
        ("sdk/chains/coin/src/address.rs", ""),
        ("sdk/chains/coin/src/batch.rs", ""),
        ("sdk/chains/coin/src/error.rs", ""),
        ("sdk/chains/coin/src/indexer.rs", ""),
    ];
    let (root, workspace) = fixture(&files);
    let policy = Policy {
        repository: Repository {
            chain_skeleton: vec!["src/indexer/mod.rs".to_owned()],
            chain_directories: vec!["src/indexer".to_owned()],
            ..Repository::default()
        },
        ..Policy::default()
    };
    let findings = repository::chain_layout(&workspace, &policy).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("src/indexer.rs"));
    assert!(findings[0].message.contains("src/indexer/mod.rs"));
    clean(root);
}

#[test]
fn chain_layout_rejects_unexpected_nested_directory() {
    let files = [
        (
            "sdk/chains/coin/Cargo.toml",
            "[package]\nname='coin'\nversion='0.0.0'",
        ),
        ("sdk/chains/coin/src/rpc/mod.rs", ""),
        ("sdk/chains/coin/src/rpc/internal/mod.rs", ""),
    ];
    let (root, workspace) = fixture(&files);
    let policy = Policy {
        repository: Repository {
            chain_skeleton: vec!["src/rpc/mod.rs".to_owned()],
            chain_directories: vec!["src/rpc".to_owned()],
            ..Repository::default()
        },
        ..Policy::default()
    };
    let findings = repository::chain_layout(&workspace, &policy).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("unexpected nested directory"));
    assert!(findings[0].message.contains("src/rpc/internal"));
    clean(root);
}

#[test]
fn concrete_chains_cannot_depend_on_siblings() {
    let files = [
        (
            "sdk/chains/alpha/Cargo.toml",
            "[package]\nname='alpha'\nversion='0.0.0'\n[dependencies]\nbeta={path='../beta'}",
        ),
        ("sdk/chains/alpha/src/lib.rs", ""),
        (
            "sdk/chains/beta/Cargo.toml",
            "[package]\nname='beta'\nversion='0.0.0'",
        ),
        ("sdk/chains/beta/src/lib.rs", ""),
    ];
    let (root, workspace) = fixture(&files);
    let findings = repository::dependencies(&workspace, &Policy::default()).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("sibling concrete chain"));
    clean(root);
}

#[test]
fn single_file_source_directory_is_rejected() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", ""),
        ("src/model.rs", ""),
        ("src/operation/mod.rs", ""),
    ]);
    let findings = repository::single_file_directories(&workspace, &Policy::default()).unwrap();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("src/operation"));
    clean(root);
}

#[test]
fn source_directory_with_two_rust_files_is_allowed() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", ""),
        ("src/model.rs", ""),
    ]);
    assert!(
        repository::single_file_directories(&workspace, &Policy::default())
            .unwrap()
            .is_empty()
    );
    clean(root);
}

#[test]
fn crate_source_root_with_only_lib_is_allowed() {
    let (root, workspace) = fixture(&[
        ("Cargo.toml", "[package]\nname='sample'\nversion='0.0.0'"),
        ("src/lib.rs", ""),
    ]);
    assert!(
        repository::single_file_directories(&workspace, &Policy::default())
            .unwrap()
            .is_empty()
    );
    clean(root);
}

#[test]
fn unicode_test_masking_keeps_exact_production_text_and_lines() {
    let text = "const CAFÉ: &str = \"ქართული\"; #[cfg(test)] struct BitcoinFixture;\n#[cfg(test)] mod tests {\n const TEXT: &str = \"é\";\n}\npub struct EthereumValue;\n";
    let (root, workspace) = fixture(&[("src/lib.rs", text)]);
    let source = &workspace.sources()[0];
    let masked = super::production::text(source);
    assert!(masked.contains("ქართული"));
    assert!(!masked.contains("BitcoinFixture"));
    assert!(!masked.contains("const TEXT"));
    assert!(masked.contains("EthereumValue"));
    assert_eq!(super::production::line_count(source), 2);
    clean(root);
}

#[test]
fn test_only_constructor_methods_and_literal_allow_markers_do_not_affect_production() {
    let text = r##"
struct Value(u8);
impl Value {
    #[cfg(test)] fn new(self) -> Self { self }
    const NOTE: &str = "// design-lint: allow self-constructor-static -- literal";
    fn parse(self) -> Self { self }
}
const RAW: &str = r#"
// design-lint: allow empty-struct -- raw string
"#;
struct Empty;
"##;
    let (root, workspace) = fixture(&[("src/lib.rs", text)]);
    let constructors = rust::check_kind(&workspace, &Policy::default(), "constructors").unwrap();
    assert_eq!(constructors.len(), 1);
    assert_eq!(constructors[0].subject, "parse");
    assert_eq!(
        rust::check_kind(&workspace, &Policy::default(), "empty")
            .unwrap()
            .len(),
        1
    );
    clean(root);
}
