use crate::{
    Analysis, Source,
    declaration::{Index, Structure},
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use contract::{Access, boundary, children};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{collections::BTreeMap, path::Path};
use tree_sitter::Node;
mod config;
mod contract;
pub use config::Config;
pub struct RedundantAccessor(Vec<Assertion>);
impl Rule for RedundantAccessor {
    const ID: &'static str = "rust/redundant-accessor";
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
        let mut findings = Vec::new();
        for assertion in &self.0 {
            let candidates = assertion.collect(analysis, &index, &root);
            for (owner, values) in candidates {
                let Some(structure) = index
                    .structures
                    .iter()
                    .find(|structure| format!("nominal:{}", structure.id.key()) == owner)
                else {
                    continue;
                };
                assertion.compare(structure, &values, &mut findings);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

impl Assertion {
    fn selected(&self, candidate: &Access<'_>) -> bool {
        self.target.matches(&candidate.source.path)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|exclude| exclude.matches(&candidate.source.path))
    }

    fn collect<'a>(
        &self,
        analysis: &'a Analysis,
        index: &Index<'a>,
        root: &Path,
    ) -> BTreeMap<String, Vec<Access<'a>>> {
        let mut candidates = BTreeMap::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            collect(
                source.syntax.root_node(),
                source,
                index,
                &tests,
                self,
                &mut candidates,
            );
        }
        candidates
    }

    fn collect_impl<'a>(
        &self,
        node: Node<'a>,
        source: &'a Source,
        index: &Index<'a>,
        tests: &[bool],
        output: &mut BTreeMap<String, Vec<Access<'a>>>,
    ) {
        if node.kind() != "impl_item"
            || node.child_by_field_name("trait").is_some()
            || boundary(node, source)
        {
            return;
        }
        let Some(structure) = index.implemented(node, source) else {
            return;
        };
        if structure.platform || boundary(structure.node, structure.source) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let owner = format!("nominal:{}", structure.id.key());
        for method in children(body)
            .into_iter()
            .filter(|child| child.kind() == "function_item")
        {
            if !self.scope.includes(tests[method.start_byte()]) {
                continue;
            }
            if let Some(candidate) = Access::new(method, source, index, structure) {
                output.entry(owner.clone()).or_default().push(candidate);
            }
        }
    }

    fn compare(
        &self,
        structure: &Structure<'_>,
        values: &[Access<'_>],
        findings: &mut Vec<Finding>,
    ) {
        for (position, candidate) in values.iter().enumerate() {
            if !self.selected(candidate) {
                continue;
            }
            if candidate.exposed {
                findings.push(candidate.exposure(structure, self));
            }
            if let Some(previous) = values[..position]
                .iter()
                .find(|previous| previous.equivalent(candidate))
            {
                findings.push(candidate.duplicate(previous, structure, self));
            }
        }
    }
}

impl<'a> Index<'a> {
    fn implemented(&self, node: Node<'a>, source: &Source) -> Option<&Structure<'a>> {
        let mut ty = node.child_by_field_name("type")?;
        if ty.kind() == "generic_type" {
            ty = ty.child_by_field_name("type").unwrap_or(ty);
        }
        let context = self.identity(source, node);
        let owner = self.resolve(source, ty, &context)?;
        let owner = owner.split('<').next().unwrap_or(&owner);
        self.structures
            .iter()
            .find(|structure| format!("nominal:{}", structure.id.key()) == owner)
    }
}

impl Scope {
    fn includes(self, test: bool) -> bool {
        match self {
            Self::Production => !test,
            Self::Tests => test,
            Self::All => true,
        }
    }
}

fn collect<'a>(
    node: Node<'a>,
    source: &'a Source,
    index: &Index<'a>,
    tests: &[bool],
    assertion: &Assertion,
    output: &mut BTreeMap<String, Vec<Access<'a>>>,
) {
    assertion.collect_impl(node, source, index, tests, output);
    for child in children(node) {
        collect(child, source, index, tests, assertion, output);
    }
}

impl Access<'_> {
    fn exposure(&self, structure: &Structure<'_>, assertion: &Assertion) -> Finding {
        self.report(
            assertion,
            format!(
                concat!(
                    "`{}` only forwards `{}.{}`, which callers can already access ",
                    "with equal or broader visibility"
                ),
                self.name, structure.id.name, self.field_name
            ),
            Evidence {
                path: structure.source.path.clone(),
                span: Some(Span::new(&structure.source.text, self.field.byte_range())),
                message: "Already exposed field".into(),
            },
        )
    }

    fn duplicate(
        &self,
        previous: &Self,
        structure: &Structure<'_>,
        assertion: &Assertion,
    ) -> Finding {
        self.report(
            assertion,
            format!(
                "`{}::{}` and `{}::{}` expose the identical field operation",
                structure.id.name, previous.name, structure.id.name, self.name
            ),
            Evidence {
                path: previous.source.path.clone(),
                span: Some(Span::new(
                    &previous.source.text,
                    previous.method.byte_range(),
                )),
                message: "Identical accessor contract".into(),
            },
        )
    }

    fn report(&self, assertion: &Assertion, message: String, evidence: Evidence) -> Finding {
        Finding {
            rule: RedundantAccessor::ID,
            path: self.source.path.clone(),
            span: Some(Span::new(&self.source.text, self.method.byte_range())),
            related: vec![evidence],
            configuration: assertion.setting.clone(),
            message,
            instruction: concat!(
                "Keep one meaningful accessor contract, or make the field private ",
                "when the method intentionally owns the public boundary."
            )
            .into(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn check(source: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/redundant-accessor\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<RedundantAccessor>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    fn findings(source: &str) -> Vec<Finding> {
        check(source, "").unwrap()
    }
    #[test]
    fn reports_public_fields() {
        let findings = findings(
            r"
pub struct Metadata {
    pub labels: Vec<String>,
    pub options: Vec<String>,
}

impl Metadata {
    pub fn labels(&self) -> &Vec<String> { &self.labels }
    pub fn options(&self) -> Vec<String> { self.options.clone() }
    pub fn set_labels(&mut self, labels: Vec<String>) { self.labels = labels; }
}
",
        );
        assert_eq!(findings.len(), 3);
        assert!(
            findings
                .iter()
                .all(|finding| finding.message.contains("already access"))
        );
    }

    #[test]
    fn reports_accessor_review() {
        let findings = findings(
            r"
pub struct Image {
    reference: String,
}

impl Image {
    pub fn reference(&self) -> &String { &self.reference }
    pub fn image_reference(&self) -> &String { &self.reference }
}
",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("Image::image_reference"));
        assert_eq!(findings[0].related.len(), 1);
    }

    #[test]
    fn preserves_field_boundaries() {
        let findings = findings(
            r"
pub struct Account {
    balance: i64,
    tags: Vec<String>,
}

impl Account {
    pub fn balance(&self) -> i64 { self.balance }
    pub fn tags(&self) -> &[String] { &self.tags }
    pub fn set_balance(&mut self, balance: i64) {
        assert!(balance >= 0);
        self.balance = balance;
    }
    pub fn debit(&mut self, amount: i64) { self.balance -= amount; }
}
",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn documentation_public_access() {
        let findings = findings(
            r"
/// Plain data.
pub struct Data {
    /// Public value.
    pub value: u32,
}

impl Data {
    /// Returns the already-public value.
    pub fn value(&self) -> u32 { self.value }
}
",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn ignores_derived_values() {
        let findings = findings(
            r#"
pub struct Value {
    pub bytes: Vec<u8>,
    pub first: u32,
    pub second: u32,
}

impl Value {
    pub fn bytes(&self) -> &[u8] { self.bytes.as_slice() }
    pub fn total(&self) -> u32 { self.first + self.second }
    pub fn encoded(&self) -> String { format!("{:?}", self.bytes) }
}
"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_compatibility_markers() {
        let findings = findings(
            r#"
pub trait Named { fn name(&self) -> &String; }
pub struct Model { pub name: String }

impl Named for Model {
    fn name(&self) -> &String { &self.name }
}

impl Model {
    #[deprecated(note = "compatibility alias")]
    pub fn old_name(&self) -> &String { &self.name }

    #[cfg(target_os = "linux")]
    pub fn platform_name(&self) -> &String { &self.name }

    #[cfg_attr(target_os = "macos", inline)]
    pub fn configured_name(&self) -> &String { &self.name }
}
"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_ffi_models() {
        let findings = findings(
            r"
#[derive(serde::Serialize)]
pub struct Wire { pub value: String }
impl Wire {
    pub fn value(&self) -> &String { &self.value }
}

#[repr(C)]
pub struct Ffi { pub value: u32 }
impl Ffi {
    pub fn value(&self) -> u32 { self.value }
}
",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn compare_shapes_duplicates() {
        let findings = findings(
            r"
pub struct Data { values: Vec<String> }
impl Data {
    pub fn values(&self) -> &Vec<String> { &self.values }
    pub fn values_owned(&self) -> Vec<String> { self.values.clone() }
}
",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn compare_contracts_duplicates() {
        let findings = findings(
            r"
pub struct Data { value: String }
impl Data {
    pub fn value(&self) -> &String { &self.value }
    pub fn value_str(&self) -> &str { &self.value }
}
",
        );
        assert!(findings.is_empty());
    }
    #[test]
    fn wallet_private_boundary_and_distinct_nominal_wrappers_are_preserved() {
        let source = "struct Address(String);struct WalletId(String);pub struct Wallet{a\
            ddress:Address,pub id:WalletId}impl Wallet{pub fn address(&self)->&Address{&\
            self.address}pub fn id(&self)->WalletId{self.id}}";
        let found = findings(source);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("Wallet.id"));
        assert!(
            findings(
                "struct Email(String);struct WalletId(String);impl Email{fn val\
            ue(&self)->&String{&self.0}}impl WalletId{fn value(&self)->&String{&self.0}}"
            )
            .is_empty()
        );
        assert!(
            findings(
                "pub struct Wallet{pub id:Missing}impl Wallet{pub fn id(&self)->Missing{self.id}}"
            )
            .is_empty()
        );
    }
    #[test]
    fn duplicate_contracts_preserve_receiver_mutability_ownership_and_conversion() {
        let source = "struct Wallet{value:String}impl Wallet{fn value(&self)->&String{&s\
            elf.value}fn other(&mut self)->&String{&self.value}fn owned(self)->String{se\
            lf.value}fn cloned(&self)->String{self.value.clone()}fn view(&self)->&str{&s\
            elf.value}}";
        assert!(findings(source).is_empty());
        assert!(
            findings(
                "pub struct Wallet{pub value:String}impl Wallet{pub fn view(&se\
            lf)->&str{&self.value}}"
            )
            .is_empty()
        );
        assert!(
            findings(
                "struct Wallet{value:String}impl Wallet{fn value(&self)->&Strin\
            g{&self.value}const fn other(&self)->&String{&self.value}}"
            )
            .is_empty()
        );
    }
    #[test]
    fn aliases_and_split_impls_resolve_without_module_name_conflation() {
        let source = "struct Wallet{value:String}type Alias=Wallet;impl Wallet{fn value(\
            &self)->&String{&self.value}}impl Alias{fn other(&self)->&String{&self.value\
            }}";
        assert_eq!(findings(source).len(), 1);
        let separate = "mod first{struct Wallet{value:String}impl Wallet{fn value(&self)\
            ->&String{&self.value}}}mod second{struct Wallet{value:String}impl Wallet{fn\
            \u{20}other(&self)->&String{&self.value}}}";
        assert!(findings(separate).is_empty());
    }
    #[test]
    fn visibility_scope_and_validation_are_not_erased() {
        assert!(
            findings(
                "struct Wallet{pub(crate) value:String}impl Wallet{pub fn value\
            (&self)->&String{&self.value}}"
            )
            .is_empty()
        );
        assert_eq!(
            findings(
                "struct Wallet{pub(crate) value:String}impl Wallet{pub(crate\
            ) fn value(&self)->&String{&self.value}}"
            )
            .len(),
            1
        );
        assert!(
            findings(
                "struct Wallet{value:u8}impl Wallet{fn set_value(&mut self,valu\
            e:u8){assert!(value>0);self.value=value;}fn update(&mut self,value:u8){self.\
            value=value;}}"
            )
            .is_empty()
        );
        let source = "#[cfg(test)]mod checks{struct Wallet{pub value:u8}impl Wallet{pub \
            fn value(&self)->u8{self.value}}}";
        assert!(findings(source).is_empty());
        assert_eq!(check(source, "scope='tests'").unwrap().len(), 1);
        assert!(
            check(source, "scope='all'\nexclude='lib.rs'")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn invalid_configuration_directives_and_selfcheck() {
        for config in ["scope='unknown'", "exclude=[]", "unknown=true"] {
            assert!(matches!(check("", config), Err(Error::Configuration(_))));
        }
        let source = "struct Wallet{pub value:u8}impl Wallet{\n// linter:disable rust/re\
            dundant-accessor -- externally fixed API boundary\nfn value(&self)->u8{self.\
            value}}";
        assert!(findings(source).is_empty());
        for source in [
            include_str!("mod.rs"),
            include_str!("config.rs"),
            include_str!("contract.rs"),
        ] {
            assert!(findings(source).is_empty());
        }
    }
}
