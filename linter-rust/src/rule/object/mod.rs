use crate::{
    Analysis, Source,
    declaration::{Index, Structure},
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use tree_sitter::Node;
mod config;
mod methods;
pub use config::Config;
use methods::{Method, collect};

pub struct GodObject(Vec<Assertion>);
impl Rule for GodObject {
    const ID: &'static str = "rust/god-object-growth";
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
        let index = Index::new(analysis, &root);
        let masks: Vec<_> = analysis
            .sources
            .iter()
            .map(|source| {
                let mut mask = vec![false; source.text.len()];
                if integration(source, &root, analysis) {
                    mask.fill(true);
                } else {
                    mark_tests(source.syntax.root_node(), &source.text, &mut mask);
                }
                mask
            })
            .collect();
        let mut findings = Vec::new();
        for assertion in &self.0 {
            let methods = collect(analysis, &index, &masks, assertion.scope);
            findings.extend(
                index
                    .structures
                    .iter()
                    .filter(|item| selected(item, assertion))
                    .filter_map(|item| {
                        let key = format!("nominal:{}", item.id.key());
                        finding(item, methods.get(&key)?, &index, assertion)
                    }),
            );
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
fn selected(item: &Structure<'_>, assertion: &Assertion) -> bool {
    assertion.target.matches(&item.source.path)
        && !assertion
            .exclude
            .as_ref()
            .is_some_and(|exclude| exclude.matches(&item.source.path))
        && scope(item.test, assertion.scope)
        && !assertion
            .excluded_suffixes
            .iter()
            .any(|suffix| item.id.name.ends_with(suffix))
        && !attributes(item.node, item.source, "repr", Some("C"))
}
fn finding(
    item: &Structure<'_>,
    methods: &[Method],
    index: &Index<'_>,
    assertion: &Assertion,
) -> Option<Finding> {
    if methods.len() <= assertion.max_methods || item.fields.len() < assertion.min_fields {
        return None;
    }
    let origins = origins(item, index, assertion);
    let clusters = clusters(methods, &origins, assertion);
    if clusters.len() < assertion.min_clusters {
        return None;
    }
    let crosses = |method: &&Method| {
        let capabilities: BTreeSet<_> = method
            .calls
            .iter()
            .filter_map(|field| origins.get(field))
            .collect();
        method.workflow && capabilities.len() >= 2
    };
    let crossing = methods.iter().find(crosses)?;
    let evidence = clusters.iter().map(|cluster| cluster.evidence(methods));
    let mut related: Vec<_> = evidence.collect();
    related.push(Evidence {
        path: crossing.path.clone(),
        span: Some(crossing.span.clone()),
        message: format!("cross-capability workflow `{}`", crossing.name),
    });
    Some(Finding {
        rule: GodObject::ID,
        path: item.source.path.clone(),
        span: Some(Span::new(&item.source.text, item.node.byte_range())),
        related,
        configuration: format!("{}.max_methods", assertion.setting),
        message: format!(
            "`{}` \
            owns {} inherent methods across {} distinct field capabilities; `{}` coordin\
            ates unrelated groups",
            item.id.name,
            methods.len(),
            clusters.len(),
            crossing.name
        ),
        instruction: "Extract cohesive field-owned capabilities and their workflows. Keep\
            \u{20}the root responsible for construction and declarative composition."
            .into(),
    })
}
struct Cluster {
    fields: BTreeSet<String>,
    origins: BTreeSet<String>,
    methods: Vec<usize>,
}
impl Cluster {
    fn evidence(&self, methods: &[Method]) -> Evidence {
        let method = &methods[self.methods[0]];
        let fields = self.fields.iter().cloned().collect::<Vec<_>>().join(", ");
        let origins = self.origins.iter().cloned().collect::<Vec<_>>().join(", ");
        let names = self
            .methods
            .iter()
            .take(3)
            .map(|index| methods[*index].name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Evidence {
            path: method.path.clone(),
            span: Some(method.span.clone()),
            message: format!("fields [{fields}] from [{origins}] via [{names}]"),
        }
    }
    fn candidates(
        methods: &[Method],
        origins: &BTreeMap<String, String>,
        assertion: &Assertion,
    ) -> Vec<Self> {
        let mut groups = BTreeMap::<BTreeSet<String>, Vec<usize>>::new();
        for (index, method) in methods.iter().enumerate().filter(|(_, method)| {
            !method.fields.is_empty()
                && method.calls.iter().any(|field| origins.contains_key(field))
        }) {
            groups.entry(method.fields.clone()).or_default().push(index);
        }
        groups
            .into_iter()
            .filter(|(_, methods)| methods.len() >= assertion.min_methods_per_cluster)
            .map(|(fields, methods)| Cluster {
                origins: fields
                    .iter()
                    .filter_map(|field| origins.get(field).cloned())
                    .collect(),
                fields,
                methods,
            })
            .collect()
    }
}
fn clusters(
    methods: &[Method],
    origins: &BTreeMap<String, String>,
    assertion: &Assertion,
) -> Vec<Cluster> {
    let candidates = Cluster::candidates(methods, origins, assertion);
    let minimal: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            !candidates.iter().any(|other| {
                other.fields != candidate.fields && other.fields.is_subset(&candidate.fields)
            })
        })
        .collect();
    let disjoint: Vec<_> = minimal
        .iter()
        .filter(|candidate| {
            minimal.iter().all(|other| {
                other.fields == candidate.fields || other.fields.is_disjoint(&candidate.fields)
            })
        })
        .map(|cluster| Cluster {
            fields: cluster.fields.clone(),
            origins: cluster.origins.clone(),
            methods: cluster.methods.clone(),
        })
        .collect();
    if disjoint
        .iter()
        .flat_map(|cluster| &cluster.origins)
        .collect::<BTreeSet<_>>()
        .len()
        < assertion.min_clusters
    {
        return Vec::new();
    }
    disjoint
}
fn origins(
    item: &Structure<'_>,
    index: &Index<'_>,
    assertion: &Assertion,
) -> BTreeMap<String, String> {
    let Some(body) = item.node.child_by_field_name("body") else {
        return BTreeMap::new();
    };
    children(body)
        .into_iter()
        .filter_map(|field| {
            let name = field.child_by_field_name("name")?;
            let ty = field.child_by_field_name("type")?;
            origin(ty, item, index, assertion)
                .map(|origin| (item.source.text[name.byte_range()].into(), origin))
        })
        .collect()
}
fn origin(
    mut node: Node<'_>,
    item: &Structure<'_>,
    index: &Index<'_>,
    assertion: &Assertion,
) -> Option<String> {
    loop {
        if node.kind() == "dynamic_type" {
            node = node.child_by_field_name("trait")?;
            continue;
        }
        if node.kind() == "bounded_type" {
            return children(node)
                .into_iter()
                .find_map(|bound| origin(bound, item, index, assertion));
        }
        if node.kind() == "reference_type" {
            node = node.child_by_field_name("type")?;
            continue;
        }
        if node.kind() == "generic_type" {
            let constructor =
                index.resolve(item.source, node.child_by_field_name("type")?, &item.id)?;
            if assertion.unwrap_types.contains(&constructor) && constructor.starts_with("std:") {
                node = node.child_by_field_name("type_arguments")?.named_child(0)?;
                continue;
            }
        }
        let canonical = index.resolve(item.source, node, &item.id)?;
        let nominal = canonical.strip_prefix("nominal:")?;
        let parts: Vec<_> = nominal.split("::").skip(1).collect();
        let mut modules = &parts[..parts.len().checked_sub(1)?];
        if modules.starts_with(
            &item
                .id
                .module
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ) {
            modules = &modules[item.id.module.len()..];
        }
        let owner = *modules.first()?;
        return (!owner.is_empty() && !owner.starts_with('@')).then(|| owner.into());
    }
}
fn scope(test: bool, scope: Scope) -> bool {
    match scope {
        Scope::Production => !test,
        Scope::Tests => test,
        Scope::All => true,
    }
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn attributes(node: Node<'_>, source: &Source, name: &str, argument: Option<&str>) -> bool {
    std::iter::successors(node.prev_named_sibling(), |node| node.prev_named_sibling())
        .take_while(|node| {
            matches!(
                node.kind(),
                "attribute_item" | "line_comment" | "block_comment"
            )
        })
        .filter(|node| node.kind() == "attribute_item")
        .filter_map(|node| node.named_child(0))
        .filter(|meta| {
            meta.named_child(0)
                .is_some_and(|path| &source.text[path.byte_range()] == name)
        })
        .any(|meta| match argument {
            None => true,
            Some(argument) => meta.child_by_field_name("arguments").is_some_and(|args| {
                source.text[args.byte_range()]
                    .split(|ch: char| !ch.is_alphanumeric())
                    .any(|word| word == argument)
            }),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn findings(source: &str) -> Vec<Finding> {
        check(source, "").findings
    }
    fn check(source: &str, options: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        let policy = include_str!("readme.md")
            .split("```toml\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("{policy}\n{options}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<GodObject>()
            .unwrap()
            .check(root.path())
            .unwrap()
    }
    #[test]
    fn reports_capability_workflow() {
        let findings = findings(&fixture("Application", ""));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("`Application`"));
        assert!(findings[0].message.contains("22 inherent methods"));
        assert!(findings[0].message.contains("run"));
        assert_eq!(findings[0].related.len(), 4);
    }

    #[test]
    fn combines_impl_blocks() {
        let source = fixture("Application", "split");
        let findings = findings(&source);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("22 inherent methods"));
    }

    #[test]
    fn ignores_many_methods() {
        let methods = (0..24)
            .map(|index| {
                format!("fn encode_{index}(&self) {{ let _ = (&self.input, self.offset); }}")
            })
            .collect::<String>();
        let source =
            format!("struct Codec {{ input: Vec<u8>, offset: usize }} impl Codec {{ {methods} }}");
        assert!(findings(&source).is_empty());
    }

    #[test]
    fn ignores_many_setters() {
        let methods = (0..24)
            .map(|index| {
                format!("fn option_{index}(mut self) -> Self {{ self.option = {index}; self }}")
            })
            .collect::<String>();
        let source =
            format!("struct ClientBuilder {{ option: usize }} impl ClientBuilder {{ {methods} }}");
        assert!(findings(&source).is_empty());
    }

    #[test]
    fn ignores_workflow_logic() {
        let mut methods = String::new();
        for (field, prefix) in [
            ("workspaces", "workspace"),
            ("settings", "setting"),
            ("terminal", "term"),
        ] {
            for index in 0..7 {
                methods.push_str(&format!(
                    "fn {prefix}_{index}(&self) {{ self.{field}.call(); }}"
                ));
            }
        }
        methods.push_str(
            "fn wire(&self) { self.workspaces.call(); self.settings.call();\
            \u{20}self.terminal.call(); }",
        );
        let source = format!(
            "{}
         struct Application {{
             workspaces: workspace::Service,
             settings: settings::Service,
             terminal: terminal::Service,
         }}
         impl Application {{ {methods} }}",
            services(),
        );
        assert!(findings(&source).is_empty());
    }

    #[test]
    fn ignores_object_stores() {
        let methods = protocol_methods();
        let source = format!(
            "mod protocol {{
             pub mod buffer {{ pub struct Store; impl Store {{ pub fn call(&self) {{}} }} }}
             pub mod texture {{ pub struct Store; impl Store {{ pub fn call(&self) {{}} }} }}
             pub mod program {{ pub struct Store; impl Store {{ pub fn call(&self) {{}} }} }}
         }}
         struct Context {{
             buffers: protocol::buffer::Store,
             textures: protocol::texture::Store,
             programs: protocol::program::Store,
         }}
         impl Context {{ {methods} fn retire(&mut self) {{
             if true {{
                 self.buffers.call();
                 self.textures.call();
                 self.programs.call();
             }}
         }} }}"
        );
        assert!(findings(&source).is_empty());
    }

    #[test]
    fn ignores_one_domain() {
        let methods = protocol_methods();
        let source = format!(
            "mod container {{
             pub mod storage {{ pub struct Store; impl Store {{ pub fn call(&self) {{}} }} }}
             pub mod runtime {{ pub struct Runtime; impl Runtime {{ pub fn call(&self) {{}} }} }}
             pub mod logs {{ pub struct Logs; impl Logs {{ pub fn call(&self) {{}} }} }}
         }}
         struct Containers {{
             storage: container::storage::Store,
             runtime: container::runtime::Runtime,
             logs: container::logs::Logs,
         }}
         impl Containers {{ {methods} fn restore(&mut self) {{
             if true {{
                 self.storage.call();
                 self.runtime.call();
                 self.logs.call();
             }}
         }} }}"
        );
        assert!(findings(&source).is_empty());
    }

    fn fixture(name: &str, split: &str) -> String {
        let mut groups = [String::new(), String::new(), String::new()];
        for index in 0..7 {
            groups[0].push_str(&format!(
                "fn workspace_{index}(&self) {{ self.workspaces.call(); }}"
            ));
            groups[1].push_str(&format!(
                "fn setting_{index}(&self) {{ self.settings.call(); }}"
            ));
            groups[2].push_str(&format!(
                "fn terminal_{index}(&self) {{ self.terminal.call(); }}"
            ));
        }
        let run = "fn run(&mut self) { if self.settings.ready() { self.workspaces.call()\
            ; self.terminal.call(); } }";
        let impls = if split.is_empty() {
            format!(
                "impl {name} {{ {}{}{}{run} }}",
                groups[0], groups[1], groups[2]
            )
        } else {
            format!(
                "impl {name} {{ {}{} }} impl {name} {{ {}{run} }}",
                groups[0], groups[1], groups[2]
            )
        };
        format!(
            "{}
         struct {name} {{
             workspaces: workspace::Service,
             settings: settings::Service,
             terminal: terminal::Service,
         }}
         {impls}",
            services(),
        )
    }

    fn services() -> &'static str {
        "mod workspace {
         pub struct Service;
         impl Service { pub fn call(&self) {} }
     }
     mod settings {
         pub struct Service;
         impl Service {
             pub fn call(&self) {}
             pub fn ready(&self) -> bool { true }
         }
     }
     mod terminal {
         pub struct Service;
         impl Service { pub fn call(&self) {} }
     }"
    }

    fn protocol_methods() -> String {
        let fields = [
            ("buffers", "buffer"),
            ("textures", "texture"),
            ("programs", "program"),
        ];
        let mut methods = String::new();
        for (field, prefix) in fields {
            for index in 0..7 {
                methods.push_str(&format!(
                    "fn {prefix}_{index}(&self) {{ self.{field}.call(); }}"
                ));
            }
        }
        methods
    }
    #[test]
    fn payment_origins_and_unrelated_names_retain_correct_ownership() {
        let source = fixture("Runtime", "split")
            .replace("workspace", "wallet")
            .replace("settings", "indexing")
            .replace("terminal", "rpc");
        let found = findings(&source);
        assert_eq!(found.len(), 1);
        for owner in ["wallet", "indexing", "rpc"] {
            assert!(
                found[0]
                    .related
                    .iter()
                    .any(|evidence| evidence.message.contains(owner))
            );
        }
        let source = format!(
            "{} mod unrelated {{struct Application {{state:u8}}}}",
            fixture("Application", "")
        );
        assert_eq!(findings(&source).len(), 1);
    }
    #[test]
    fn standard_containers_unwrap_but_nominal_wrappers_keep_ownership() {
        let wrapped = fixture("Application", "")
            .replace(
                "workspaces: workspace::Service",
                "workspaces: std::sync::Arc<workspace::Service>",
            )
            .replace(
                "settings: settings::Service",
                "settings: std::sync::Mutex<settings::Service>",
            )
            .replace(
                "terminal: terminal::Service",
                "terminal: Box<terminal::Service>",
            );
        assert_eq!(findings(&wrapped).len(), 1);
        let local = format!(
            "mod wrapper {{pub struct Arc<T>(T);}} {}",
            fixture("Application", "")
                .replace(
                    "workspaces: workspace::Service",
                    "workspaces: wrapper::Arc<workspace::Service>"
                )
                .replace(
                    "settings: settings::Service",
                    "settings: wrapper::Arc<settings::Service>"
                )
                .replace(
                    "terminal: terminal::Service",
                    "terminal: wrapper::Arc<terminal::Service>"
                )
        );
        assert!(findings(&local).is_empty());
    }
    #[test]
    fn limits_scope_and_suppression_are_enforced() {
        let source = fixture("Application", "");
        assert!(check(&source, "max_methods=22").findings.is_empty());
        assert_eq!(check(&source, "max_methods=21").findings.len(), 1);
        assert!(check(&source, "exclude='lib.rs'").findings.is_empty());
        let test_only = format!("#[cfg(test)] mod tests {{{source}}}");
        assert!(findings(&test_only).is_empty());
        assert_eq!(check(&test_only, "scope='tests'").findings.len(), 1);
        let source = source.replace(
            "struct Application",
            "// linter:disable rust/god-objec\
            t-growth -- External composition root requires this workflow.\nstruct Applic\
            ation",
        );
        assert_eq!(check(&source, "").suppressed.len(), 1);
    }
    #[test]
    fn receiver_workflow_is_not_invented_from_deferred_closures() {
        let source = fixture("Application", "").replace(
            "if self.settings.ready() { self.wo\
            rkspaces.call(); self.terminal.call(); }",
            "\
            let later=|| { if self.settings.ready() { self.workspaces.call(); self.termi\
            nal.call(); } };",
        );
        assert!(findings(&source).is_empty());
        assert!(findings(&fixture("ApplicationBuilder", "")).is_empty());
        assert!(
            findings(
                &fixture("Application", "")
                    .replace("struct Application", "#[repr(C)] struct Application")
            )
            .is_empty()
        );
    }
    #[test]
    fn invalid_thresholds_and_unknown_settings_fail() {
        for setting in [
            "max_methods=0",
            "min_fields=0",
            "min_clusters=1",
            "min_methods_per_cluster=0",
            "minimum=3",
            "scope='maybe'",
            "unwrap_types=['nominal:Wrapper']",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/god-object-growth\"]]\ntarget='**/*.rs'\n{setting}"),
            )
            .unwrap();
            assert!(matches!(
                linter::Registry::default()
                    .register::<GodObject>()
                    .unwrap()
                    .check(root.path()),
                Err(Error::Configuration(_))
            ));
        }
    }
    #[test]
    fn owned_implementation_is_not_a_god_object() {
        assert!(findings(include_str!("mod.rs")).is_empty());
        assert!(findings(include_str!("config.rs")).is_empty());
        assert!(findings(include_str!("methods.rs")).is_empty());
    }
}
