use std::{collections::BTreeSet, path::Path};

use syn::spanned::Spanned;

use crate::{Policy, test_support::Fixture};

use super::{SourceFile, Workspace};

fn source<'a>(workspace: &'a Workspace, suffix: &str) -> &'a SourceFile {
    workspace
        .sources()
        .iter()
        .find(|source| source.path.ends_with(suffix))
        .unwrap()
}

#[test]
fn excerpts_use_exact_unicode_character_spans_and_multiline_text() {
    let text = "struct Café { résumé: String }\nfn render() -> String {\n    \"ქართული é\".to_owned()\n}\n";
    let fixture = Fixture::new(&[("src/lib.rs", text)]);
    let workspace = fixture.workspace();
    let source = source(&workspace, "src/lib.rs");
    let syn::Item::Struct(structure) = &source.syntax.items[0] else {
        panic!("struct");
    };
    let field = structure.fields.iter().next().unwrap();
    assert_eq!(source.excerpt(structure.ident.span()), "Café");
    assert_eq!(
        source.excerpt(field.ident.as_ref().unwrap().span()),
        "résumé"
    );
    assert_eq!(source.excerpt(field.ty.span()), "String");
    assert_eq!(source.location(field.ty.span()).line, 1);
    assert_eq!(source.location(field.ty.span()).column, 23);
    let syn::Item::Fn(function) = &source.syntax.items[1] else {
        panic!("function");
    };
    assert_eq!(
        source.excerpt(function.span()),
        "fn render() -> String {\n    \"ქართული é\".to_owned()\n}"
    );
    assert_eq!(source.location(function.span()).line, 2);
    assert_eq!(source.location(function.span()).column, 1);
}

#[test]
fn source_views_keep_test_files_available_without_treating_them_as_production() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "pub struct Library;"),
        ("src/testing.rs", "pub struct Testing;"),
        ("src/cache_test.rs", "struct Fixture;"),
        ("src/test.rs", "struct Fixture;"),
        ("tests/contract.rs", "#[test] fn contract() {}"),
    ]);
    let workspace = fixture.workspace();
    assert_eq!(workspace.sources().len(), 5);
    let production = workspace
        .production()
        .map(|source| source.path.file_name().unwrap().to_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(production, BTreeSet::from(["lib.rs", "testing.rs"]));
    assert!(source(&workspace, "src/cache_test.rs").test);
    assert!(source(&workspace, "src/test.rs").test);
    assert!(source(&workspace, "tests/contract.rs").test);
}

#[test]
fn inline_test_items_leave_the_surrounding_production_file_visible() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        r##"
pub struct Before;
#[cfg(test)] mod tests { #[test] fn fixture() {} }
const TEXT: &str = "#[cfg(test)]";
pub struct After;
"##,
    )]);
    let workspace = fixture.workspace();
    assert_eq!(workspace.production().count(), 1);
    assert!(!source(&workspace, "src/lib.rs").test);
}

#[test]
fn propagates_test_only_modules_through_nested_and_path_based_children() {
    let fixture = Fixture::new(&[
        (
            "src/lib.rs",
            r#"
#[cfg(test)] mod support;
mod internal { #[cfg(all(test, feature = "fixtures"))] mod cases; }
#[cfg(test)] #[path = "custom_cases.rs"] mod custom;
#[cfg(any(test, feature = "runtime"))] mod optional;
#[cfg(not(test))] mod runtime;
"#,
        ),
        ("src/support.rs", "mod child;"),
        ("src/support/child.rs", "struct Child;"),
        ("src/internal/cases.rs", "struct Case;"),
        ("src/custom_cases.rs", "struct Custom;"),
        ("src/optional.rs", "struct Optional;"),
        ("src/runtime.rs", "struct Runtime;"),
    ]);
    let workspace = fixture.workspace();
    for suffix in [
        "src/support.rs",
        "src/support/child.rs",
        "src/internal/cases.rs",
        "src/custom_cases.rs",
    ] {
        assert!(
            source(&workspace, suffix).test,
            "{suffix} must be test-only"
        );
    }
    assert!(!source(&workspace, "src/optional.rs").test);
    assert!(!source(&workspace, "src/runtime.rs").test);
    assert_eq!(workspace.production().count(), 3);
}

#[test]
fn file_level_test_attribute_propagates_to_its_children() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "pub struct Library;"),
        ("src/fixture.rs", "#![cfg(test)]\nmod child;"),
        ("src/fixture/child.rs", "pub struct Child;"),
    ]);
    let workspace = fixture.workspace();
    assert!(source(&workspace, "src/fixture.rs").test);
    assert!(source(&workspace, "src/fixture/child.rs").test);
    assert_eq!(workspace.production().count(), 1);
}

#[test]
fn a_production_path_prevents_shared_test_module_from_being_excluded() {
    let fixture = Fixture::new(&[
        (
            "src/lib.rs",
            "#[cfg(test)] #[path=\"shared.rs\"] mod fixture;\n#[path=\"shared.rs\"] mod shared;",
        ),
        ("src/shared.rs", "pub struct Shared;"),
    ]);
    let workspace = fixture.workspace();
    assert!(!source(&workspace, "src/shared.rs").test);
    assert_eq!(workspace.production().count(), 2);
}

fn dependency_fixture() -> Fixture {
    Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers=['consumer','domain','common','builder','dev','target','target-build','target-dev']\n[workspace.dependencies]\ninherited={package='common-library',path='common'}\n",
        ),
        (
            "consumer/Cargo.toml",
            r#"
[package]
name='consumer'
version='0.0.0'
[dependencies]
service={package='domain-library',path='../domain'}
inherited={workspace=true}
[build-dependencies]
builder={path='../builder'}
[dev-dependencies]
dev={path='../dev'}
[target.'cfg(unix)'.dependencies]
target_alias={package='target-library',path='../target'}
[target.'cfg(unix)'.build-dependencies]
target_builder={path='../target-build'}
[target.'cfg(unix)'.dev-dependencies]
target_dev={path='../target-dev'}
"#,
        ),
        ("consumer/src/lib.rs", "pub struct Consumer;"),
        (
            "domain/Cargo.toml",
            "[package]\nname='domain-library'\nversion='0.0.0'\n[dependencies]\ncommon={package='common-library',path='../common'}\n",
        ),
        ("domain/src/lib.rs", "pub struct Domain;"),
        (
            "common/Cargo.toml",
            "[package]\nname='common-library'\nversion='0.0.0'\n",
        ),
        ("common/src/lib.rs", "pub struct Common;"),
        (
            "builder/Cargo.toml",
            "[package]\nname='builder'\nversion='0.0.0'\n",
        ),
        ("builder/src/lib.rs", "pub struct Builder;"),
        ("dev/Cargo.toml", "[package]\nname='dev'\nversion='0.0.0'\n"),
        ("dev/src/lib.rs", "pub struct Development;"),
        (
            "target/Cargo.toml",
            "[package]\nname='target-library'\nversion='0.0.0'\n",
        ),
        ("target/src/lib.rs", "pub struct Target;"),
        (
            "target-build/Cargo.toml",
            "[package]\nname='target_builder'\nversion='0.0.0'\n",
        ),
        ("target-build/src/lib.rs", "pub struct TargetBuilder;"),
        (
            "target-dev/Cargo.toml",
            "[package]\nname='target_dev'\nversion='0.0.0'\n",
        ),
        ("target-dev/src/lib.rs", "pub struct TargetDevelopment;"),
    ])
}

fn assert_dependency_identity(workspace: &Workspace, root: &Path) {
    let root = root.canonicalize().unwrap();
    let consumer = workspace
        .packages
        .iter()
        .find(|package| package.name == "consumer")
        .unwrap();
    assert_eq!(consumer.root, root.join("consumer"));
    assert_eq!(
        consumer.dependencies,
        [
            "builder",
            "common-library",
            "dev",
            "domain-library",
            "target-library",
            "target_builder",
            "target_dev"
        ]
    );
    assert_eq!(
        consumer.build_dependencies,
        [
            "builder",
            "common-library",
            "domain-library",
            "target-library",
            "target_builder"
        ]
    );
    let domain = workspace
        .packages
        .iter()
        .find(|package| package.name == "domain-library")
        .unwrap();
    assert_eq!(domain.dependencies, ["common-library"]);
}

#[test]
fn cargo_alias_target_build_dev_and_workspace_edges_use_actual_package_identity() {
    let fixture = dependency_fixture();
    let workspace = fixture.workspace();
    assert_dependency_identity(&workspace, fixture.path());
    assert_eq!(
        source(&workspace, "consumer/src/lib.rs").package,
        "consumer"
    );
    assert_eq!(
        source(&workspace, "domain/src/lib.rs").package,
        "domain-library"
    );
}

#[test]
fn focused_subtree_and_file_loading_preserve_the_owning_package_graph() {
    let fixture = dependency_fixture();
    for suffix in ["consumer/src", "consumer/src/lib.rs"] {
        let workspace =
            Workspace::load(vec![fixture.path().join(suffix)], &Policy::default().source).unwrap();
        assert_eq!(workspace.sources().len(), 1);
        assert_dependency_identity(&workspace, fixture.path());
        assert_eq!(
            workspace.policy_roots,
            [fixture.path().canonicalize().unwrap()]
        );
    }
}

#[test]
fn external_dependency_is_not_confused_with_a_local_same_named_package() {
    let fixture = Fixture::new(&[
        ("Cargo.toml", "[workspace]\nmembers=['consumer','serde']\n"),
        (
            "consumer/Cargo.toml",
            "[package]\nname='consumer'\nversion='0.0.0'\n[dependencies]\nserde='1'\n",
        ),
        ("consumer/src/lib.rs", "pub struct Consumer;"),
        (
            "serde/Cargo.toml",
            "[package]\nname='serde'\nversion='0.0.0'\n",
        ),
        ("serde/src/lib.rs", "pub struct Local;"),
    ]);
    let workspace = fixture.workspace();
    let consumer = workspace
        .packages
        .iter()
        .find(|package| package.name == "consumer")
        .unwrap();
    assert!(consumer.dependencies.is_empty());
    assert!(consumer.build_dependencies.is_empty());
}

#[test]
fn ignored_self_package_is_skipped_before_parsing_its_sources() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "pub struct Product;"),
        (
            "tool/Cargo.toml",
            "[package]\nname='design-lint'\nversion='0.0.0'\n",
        ),
        ("tool/src/lib.rs", "this is deliberately invalid Rust"),
        ("target/output.rs", "invalid generated Rust"),
    ]);
    let mut policy = Policy::default();
    policy.source.self_packages.push("design-lint".to_owned());
    let workspace = Workspace::load(vec![fixture.path().to_path_buf()], &policy.source).unwrap();
    assert_eq!(workspace.sources().len(), 1);
    assert_eq!(workspace.sources()[0].package, "fixture");
    assert!(
        workspace
            .packages
            .iter()
            .any(|package| package.name == "design-lint")
    );
}

#[test]
fn overlapping_scan_roots_do_not_duplicate_sources_or_packages() {
    let fixture = Fixture::new(&[
        ("src/lib.rs", "mod child;"),
        ("src/child.rs", "pub struct Child;"),
    ]);
    let workspace = Workspace::load(
        vec![
            fixture.path().to_path_buf(),
            fixture.path().join("src"),
            fixture.path().join("src/lib.rs"),
        ],
        &Policy::default().source,
    )
    .unwrap();
    assert_eq!(workspace.sources().len(), 2);
    assert_eq!(workspace.packages.len(), 1);
    assert!(workspace.sources()[0].path < workspace.sources()[1].path);
}

#[cfg(unix)]
#[test]
fn nested_symlinks_are_skipped_but_explicit_targets_are_resolved() {
    use std::os::unix::fs::symlink;

    let external = Fixture::new(&[("src/lib.rs", "pub struct External;")]);
    let fixture = Fixture::new(&[("src/lib.rs", "pub struct Product;")]);
    let link = fixture.path().join("linked");
    symlink(external.path(), &link).unwrap();
    let workspace = fixture.workspace();
    assert_eq!(workspace.sources().len(), 1);
    assert_eq!(workspace.sources()[0].package, "fixture");
    let explicit = Workspace::load(vec![link], &Policy::default().source).unwrap();
    assert_eq!(explicit.sources().len(), 1);
    assert_eq!(
        explicit.sources()[0].path,
        external.path().join("src/lib.rs").canonicalize().unwrap()
    );
}

#[test]
fn nested_workspace_cannot_fall_back_to_outer_dependency_definitions() {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[workspace]\n[workspace.dependencies]\nouter='1'",
        ),
        (
            "nested/Cargo.toml",
            "[workspace]\n[package]\nname='nested'\nversion='0.0.0'\n[dependencies]\nouter.workspace=true",
        ),
        ("nested/src/lib.rs", "pub struct Value(u8);"),
    ]);
    let error = match Workspace::load(
        vec![fixture.path().join("nested")],
        &Policy::default().source,
    ) {
        Ok(_) => panic!("nested workspace accepted an undefined inherited dependency"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("missing workspace dependency `outer`")
    );
}
