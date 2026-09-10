use std::collections::{BTreeMap, BTreeSet};

use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::ops::Range;

use crate::{
    Analysis,
    declaration::{Index, Structure},
};
use config::{Assertion, Scope};

mod attributes;
mod config;
mod conversion;
pub use config::Config;

pub struct ModelDuplication(Vec<Assertion>);

impl Rule for ModelDuplication {
    const ID: &'static str = "rust/wire-domain-model-duplication";
    type Analysis = Analysis;
    type Config = Config;

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }

    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root = std::fs::canonicalize(project.root())
            .map_err(|error| Error::Analysis(error.to_string()))?;
        let index = Index::new(analysis, &root);
        let evidence = conversion::Evidence::collect(&index, analysis, &root);
        let models: Vec<_> = index.structures.iter().filter_map(Model::new).collect();
        let dependencies = dependencies(analysis);
        let mut findings = Vec::new();
        for assertion in &self.0 {
            for (position, first) in models.iter().enumerate() {
                for second in &models[position + 1..] {
                    if let Some(finding) =
                        compare(first, second, assertion, &evidence, &dependencies)
                    {
                        findings.push(finding);
                    }
                }
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

struct Model<'a> {
    structure: &'a Structure<'a>,
    fields: BTreeMap<String, (String, Range<usize>)>,
    field_count: usize,
    wire: bool,
    domain: bool,
}

impl<'a> Model<'a> {
    fn new(structure: &'a Structure<'a>) -> Option<Self> {
        if structure.fields.len() < 3
            || structure.platform
            || attributes::projection(&structure.id.name)
        {
            return None;
        }
        let attrs = attributes::attributes(structure.node, structure.source);
        if attrs.iter().any(|meta| meta.path().is_ident("repr")) {
            return None;
        }
        let body = structure.node.child_by_field_name("body")?;
        if body.kind() != "field_declaration_list" {
            return None;
        }
        let mut fields = BTreeMap::new();
        let mut public = 0;
        let mut serialized = attributes::serialized(&attrs);
        let mut cursor = body.walk();
        for field in body
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "field_declaration")
        {
            let name = field.child_by_field_name("name")?;
            let name = &structure.source.text[name.byte_range()];
            let attrs = attributes::attributes(field, structure.source);
            serialized |= attrs.iter().any(|meta| meta.path().is_ident("serde"));
            let mut cursor = field.walk();
            public += usize::from(field.named_children(&mut cursor).any(|child| {
                child.kind() == "visibility_modifier"
                    && &structure.source.text[child.byte_range()] == "pub"
            }));
            if let Some(indexed) = structure
                .fields
                .get(name)
                .filter(|field| field.ty.is_some())
            {
                let ty = indexed.ty.as_ref()?;
                let name = attributes::renamed(name, &attrs);
                if fields
                    .insert(name, (ty.clone(), indexed.span.clone()))
                    .is_some()
                {
                    return None;
                }
            }
        }
        Some(Self {
            structure,
            fields,
            field_count: structure.fields.len(),
            wire: serialized && public >= 3,
            domain: !serialized && structure.fields.len() - public >= 3,
        })
    }

    fn selected(&self, assertion: &Assertion) -> bool {
        assertion.target.matches(&self.structure.source.path)
            && !assertion
                .exclude
                .as_ref()
                .is_some_and(|exclude| exclude.matches(&self.structure.source.path))
            && match assertion.scope {
                Scope::All => true,
                Scope::Production => !self.structure.test,
                Scope::Tests => self.structure.test,
            }
    }
}

fn compare(
    first: &Model<'_>,
    second: &Model<'_>,
    assertion: &Assertion,
    evidence: &conversion::Evidence<'_>,
    dependencies: &BTreeSet<(String, String)>,
) -> Option<Finding> {
    let first_id = format!("nominal:{}", first.structure.id.key());
    let second_id = format!("nominal:{}", second.structure.id.key());
    let conversions: Vec<_> = evidence
        .conversions
        .iter()
        .filter(|conversion| {
            if !scope_selected(assertion.scope, conversion.test) {
                return false;
            }
            (conversion.from == first_id && conversion.to == second_id)
                || (conversion.from == second_id && conversion.to == first_id)
        })
        .collect();
    if conversions.iter().any(|conversion| !conversion.copied) {
        return None;
    }
    let converted = !conversions.is_empty();
    let first_package = &first.structure.id.package;
    let second_package = &second.structure.id.package;
    let forward = dependencies.contains(&(first_package.clone(), second_package.clone()));
    let reverse = dependencies.contains(&(second_package.clone(), first_package.clone()));
    if first_package != second_package && !forward && !reverse && !converted {
        return None;
    }
    let (candidate, owner) = if first.wire
        && second.domain
        && evidence
            .behaviors
            .contains(&(second_id.clone(), second.structure.test))
    {
        (first, second)
    } else if second.wire
        && first.domain
        && evidence
            .behaviors
            .contains(&(first_id.clone(), first.structure.test))
    {
        (second, first)
    } else if first.wire && second.wire && first_package != second_package {
        if reverse && !forward {
            (second, first)
        } else {
            (first, second)
        }
    } else {
        return None;
    };
    if !candidate.selected(assertion) || !scope_selected(assertion.scope, owner.structure.test) {
        return None;
    }
    let concept = attributes::concept(&candidate.structure.id.name);
    if !converted
        && (concept.is_empty() || concept != attributes::concept(&owner.structure.id.name))
    {
        return None;
    }
    let common: Vec<_> = candidate
        .fields
        .iter()
        .filter(|(name, (ty, _))| {
            owner
                .fields
                .get(*name)
                .is_some_and(|(other, _)| ty == other)
        })
        .collect();
    if common.len() < assertion.min_shared_fields
        || (common.len() as u128) * 100
            < (candidate.field_count.min(owner.field_count) as u128)
                * assertion.min_overlap_percent as u128
    {
        return None;
    }
    let mut related = vec![Evidence {
        path: owner.structure.source.path.clone(),
        span: Some(Span::new(
            &owner.structure.source.text,
            owner.structure.node.byte_range(),
        )),
        message: format!("Owning model `{}`", owner.structure.id.name),
    }];
    for (name, (ty, node)) in &common {
        related.push(Evidence {
            path: candidate.structure.source.path.clone(),
            span: Some(Span::new(&candidate.structure.source.text, (*node).clone())),
            message: format!("Copied field `{name}: {ty}`"),
        });
        if let Some((_, node)) = owner.fields.get(*name) {
            related.push(Evidence {
                path: owner.structure.source.path.clone(),
                span: Some(Span::new(&owner.structure.source.text, node.clone())),
                message: format!("Matching owner field `{name}`"),
            });
        }
    }
    for conversion in conversions {
        related.push(Evidence {
            path: conversion.source.path.clone(),
            span: Some(Span::new(
                &conversion.source.text,
                conversion.node.byte_range(),
            )),
            message: "Resolved field-copy conversion connects the models".into(),
        });
    }
    Some(Finding {
        rule: ModelDuplication::ID,
        path: candidate.structure.source.path.clone(),
        span: Some(Span::new(
            &candidate.structure.source.text,
            candidate.structure.node.byte_range(),
        )),
        related,
        configuration: assertion.setting.clone(),
        message: format!(
            "\
        Wire model `{}` duplicates `{}` across {} matching named fields",
            candidate.structure.id.name,
            owner.structure.id.name,
            common.len()
        ),
        instruction: "\
        Reuse or compose the owned model; keep a separate representation only for a conc\
        rete boundary contract."
            .into(),
    })
}

fn scope_selected(scope: Scope, test: bool) -> bool {
    match scope {
        Scope::Production => !test,
        Scope::Tests => test,
        Scope::All => true,
    }
}

fn dependencies(analysis: &Analysis) -> BTreeSet<(String, String)> {
    let mut edges = BTreeSet::new();
    for (manifest, package) in &analysis.packages {
        for dependency in &package.dependencies {
            let Some(path) = &dependency.path else {
                continue;
            };
            let target = path.join("Cargo.toml").into_std_path_buf();
            let target = std::fs::canonicalize(&target).unwrap_or(target);
            if analysis.packages.contains_key(&target) {
                edges.insert((
                    manifest.to_string_lossy().into_owned(),
                    target.to_string_lossy().into_owned(),
                ));
            }
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn configured(source: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "lib.rs", source);
        write(
            root.path(),
            "linter.toml",
            &format!(
                "[[rules.\"rust/wire-domain-model-duplication\"]]\ntarget='**/*.rs'\n{config}"
            ),
        );
        linter::Registry::default()
            .register::<ModelDuplication>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    fn findings(source: &str) -> Vec<Finding> {
        configured(source, "").unwrap()
    }
    fn package_findings(
        first_domain: &str,
        first: (&str, &str),
        second_domain: &str,
        second: (&str, &str, Option<&str>),
    ) -> Vec<Finding> {
        let root = tempfile::tempdir().unwrap();
        let first_path = format!("{first_domain}/{}", first.0);
        let second_path = format!("{second_domain}/{}", second.0);
        write(
            root.path(),
            "Cargo.toml",
            &format!("[workspace]\nmembers=['{first_path}','{second_path}']\nresolver='3'"),
        );
        write(
            root.path(),
            &format!("{first_path}/Cargo.toml"),
            &format!("[package]\nname='{}'\nversion='0.0.0'", first.0),
        );
        let dependency = second
            .2
            .map(|name| {
                format!(
                    "\n[dependencies]\n{name}={{path='{}'}}",
                    root.path().join(&first_path).display()
                )
            })
            .unwrap_or_default();
        write(
            root.path(),
            &format!("{second_path}/Cargo.toml"),
            &format!(
                "[package]\nname='{}'\nversion='0.0.0'{dependency}",
                second.0
            ),
        );
        write(root.path(), &format!("{first_path}/src/lib.rs"), first.1);
        write(root.path(), &format!("{second_path}/src/lib.rs"), second.1);
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"rust/wire-domain-model-duplication\"]]\ntarget='**/*.rs'",
        );
        linter::Registry::default()
            .register::<ModelDuplication>()
            .unwrap()
            .check(root.path())
            .unwrap()
            .findings
    }
    #[test]
    fn reports_bearing_model() {
        let found = findings(
            r"
#[derive(serde::Serialize, serde::Deserialize)]
pub struct WireImage {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
    pub arch: String,
}
pub struct Image {
    id: u64,
    name: String,
    rootfs: String,
    arch: String,
}
impl Image {
    pub fn validate(&self) -> bool { !self.name.is_empty() }
}
impl From<Image> for WireImage {
    fn from(image: Image) -> Self {
        Self {
            id: image.id,
            name: image.name,
            rootfs: image.rootfs,
            arch: image.arch,
        }
    }
}
",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, ModelDuplication::ID);
        assert!(found[0].related.len() >= 8);
    }

    #[test]
    fn resolves_field_renames() {
        let found = findings(
            r#"
#[derive(serde::Serialize)]
pub struct ApiNode {
    #[serde(rename = "id")]
    pub identifier: Identifier,
    pub name: String,
    pub address: String,
}
pub struct Node {
    id: u64,
    name: String,
    address: String,
}
type Identifier = u64;
impl Node {
    pub fn name(&self) -> &str { &self.name }
}
"#,
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn ignores_abi_layouts() {
        let found = findings(
            r"
#[derive(serde::Serialize)]
pub struct ImageResponse {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
}
#[repr(C)]
#[derive(serde::Serialize)]
pub struct WireHeader {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
}
pub struct Image {
    id: u64,
    name: String,
    rootfs: String,
}
impl Image {
    pub fn validate(&self) -> bool { !self.name.is_empty() }
}
",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn ignores_concept_evidence() {
        let found = findings(
            r"
#[derive(serde::Serialize)]
pub struct WireNetwork {
    pub id: u64,
    pub name: String,
    pub path: String,
}
pub struct Volume {
    id: u64,
    name: String,
    path: String,
}
impl Volume {
    pub fn mount(&self) {}
}
",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn ignores_composes_base() {
        let found = findings(
            r"
#[derive(serde::Serialize)]
pub struct WireImage {
    pub image: Image,
    pub source: String,
    pub score: u64,
}
pub struct Image {
    id: u64,
    name: String,
    rootfs: String,
}
impl Image {
    pub fn validate(&self) -> bool { !self.name.is_empty() }
}
",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn ignores_overlap_shapes() {
        let found = findings(
            r"
#[derive(serde::Serialize)]
pub struct WireImage {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
    pub created: u64,
    pub labels: Vec<String>,
}

pub struct Image {
    id: u64,
    name: String,
    rootfs: String,
    arch: String,
    command: Vec<String>,
}
impl Image {
    pub fn validate(&self) -> bool { !self.name.is_empty() }
}
",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn reports_dependency_edge() {
        let owner = r"
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ApiImage {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
    pub arch: String,
}
";
        let client = r"
#[derive(serde::Serialize, serde::Deserialize)]
pub struct WireImage {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
    pub arch: String,
}
";
        let found = package_findings(
            "containers",
            ("api-owner", owner),
            "containers",
            ("api-client", client, Some("api-owner")),
        );
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .message
                .contains("`WireImage` duplicates `ApiImage`")
        );
    }

    #[test]
    fn ignores_packages_domains() {
        let model = r"
#[derive(serde::Serialize)]
pub struct ApiImage {
    pub id: u64,
    pub name: String,
    pub rootfs: String,
}
";
        let found = package_findings(
            "containers",
            ("api-owner", model),
            "gpu",
            ("unrelated", model, None),
        );
        assert!(found.is_empty());
    }
    const MODEL: &str = "#[derive(serde::Serialize)] struct WireImage { pub id:u64, pub \
        name:String, pub path:String } struct Image { id:u64, name:String, path:String }\
        \u{20}impl Image { fn validate(&self) {} }";

    #[test]
    fn wrappers_nominal_types_and_unrelated_records_never_merge() {
        assert!(
            findings(
                "struct Email(String); struct WalletId(String); struct One {val\
            ue:String} struct Two {value:String}"
            )
            .is_empty()
        );
        let distinct = MODEL
            .replace("pub id:u64", "pub id:Email")
            .replace("{ id:u64", "{ id:WalletId");
        assert!(
            findings(&format!(
                "struct Email(String); struct WalletId(String); {distinct}"
            ))
            .is_empty()
        );
        assert!(findings(&MODEL.replace("WireImage", "WireVolume")).is_empty());
        assert!(findings(&MODEL.replace("WireImage", "WireImagery")).is_empty());
    }

    #[test]
    fn resolved_copy_conversion_relates_names_but_transformations_are_exempt() {
        let copy = "impl From<Image> for WireVolume { fn from(value:Image)->Self { Self \
            { id:value.id, name:value.name, path:value.path } } }";
        let source = format!("{} {copy}", MODEL.replace("WireImage", "WireVolume"));
        let found = findings(&source);
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .related
                .iter()
                .any(|item| item.message.contains("field-copy conversion"))
        );
        assert!(
            findings(&source.replace("name:value.name", "name:value.name.to_uppercase()"))
                .is_empty()
        );
        assert!(
            findings(&source.replace("Self { id", "let validated = value.name.len(); Self { id"))
                .is_empty()
        );
        assert!(findings(&format!("trait From<T> {{}} {source}")).is_empty());
        let same_concept = source
            .replace("WireVolume", "WireImage")
            .replace("name:value.name", "name:value.name.to_uppercase()");
        assert!(findings(&same_concept).is_empty());
    }

    #[test]
    fn thresholds_and_scope_are_explicit() {
        assert_eq!(findings(MODEL).len(), 1);
        assert!(configured(MODEL, "min_shared_fields=4").unwrap().is_empty());
        let source = MODEL
            .replace("pub path:String", "pub path:String, pub extra:u8")
            .replace("path:String } impl", "path:String, other:u8 } impl");
        assert_eq!(
            configured(&source, "min_overlap_percent=75").unwrap().len(),
            1
        );
        assert!(
            configured(&source, "min_overlap_percent=76")
                .unwrap()
                .is_empty()
        );
        assert!(configured(MODEL, "exclude='lib.rs'").unwrap().is_empty());
        assert!(configured(MODEL, "scope='tests'").unwrap().is_empty());
        let tests = format!("#[cfg(test)] mod tests {{ {MODEL} }}");
        assert!(findings(&tests).is_empty());
        assert_eq!(configured(&tests, "scope='tests'").unwrap().len(), 1);
        assert!(findings(&MODEL.replace("impl Image", "#[cfg(test)] impl Image")).is_empty());
        assert!(findings(&MODEL.replace("struct Image", "#[cfg(test)] struct Image")).is_empty());
    }

    #[test]
    fn unresolved_fields_shadowing_and_platforms_do_not_establish_evidence() {
        assert!(findings(&MODEL.replace("u64", "Unknown")).is_empty());
        assert!(findings(&MODEL.replace("struct Image", "#[cfg(unix)] struct Image")).is_empty());
        let source = "mod a { pub struct Id(String); #[derive(serde::Serialize)] pub str\
            uct WireImage {pub id:Id,pub name:String,pub path:String} } mod b { struct I\
            d(String); struct Image {id:Id,name:String,path:String} impl Image {fn valid\
            ate(&self){}} }";
        assert!(findings(source).is_empty());
    }

    #[test]
    fn configuration_rejects_unsafe_thresholds_and_unknown_fields() {
        for config in [
            "min_shared_fields=2",
            "min_shared_fields=-1",
            "min_overlap_percent=0",
            "min_overlap_percent=101",
            "min_overlap_percent='75'",
            "scope='bad'",
            "exclude=[]",
            "unknown=true",
        ] {
            assert!(
                matches!(configured("", config), Err(Error::Configuration(_))),
                "{config}"
            );
        }
    }

    #[test]
    fn findings_include_field_locations_and_reasoned_directives_suppress() {
        let found = findings(MODEL);
        assert!(found[0].span.is_some());
        assert_eq!(found[0].related.len(), 7);
        assert!(
            found[0]
                .related
                .iter()
                .all(|evidence| evidence.span.is_some())
        );
        let suppressed = format!(
            "// linter:disable rust/wire-domain-model-duplication -- external contract f\
                ixes field layout\n{MODEL}"
        );
        assert!(findings(&suppressed).is_empty());
    }
    #[test]
    fn implementation_has_no_wire_model_duplicates() {
        for text in [
            include_str!("mod.rs"),
            include_str!("config.rs"),
            include_str!("attributes.rs"),
            include_str!("conversion.rs"),
        ] {
            assert!(findings(text).is_empty());
        }
    }
}
