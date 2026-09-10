use crate::{policy::Policy, test_support::Fixture};

fn findings(source: &str) -> Vec<crate::Finding> {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    super::check(&fixture.workspace(), &Policy::default()).unwrap()
}

#[test]
fn reports_large_state_bag_with_cross_capability_workflow() {
    let findings = findings(&fixture("Application", ""));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].subject, "Application");
    assert!(findings[0].message.contains("22 inherent methods"));
    assert!(findings[0].message.contains("run"));
    assert_eq!(findings[0].related.len(), 4);
}

#[test]
fn combines_inherent_methods_across_impl_blocks() {
    let source = fixture("Application", "split");
    let findings = findings(&source);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("22 inherent methods"));
}

#[test]
fn ignores_cohesive_codec_with_many_methods() {
    let methods = (0..24)
        .map(|index| format!("fn encode_{index}(&self) {{ let _ = (&self.input, self.offset); }}"))
        .collect::<String>();
    let source =
        format!("struct Codec {{ input: Vec<u8>, offset: usize }} impl Codec {{ {methods} }}");
    assert!(findings(&source).is_empty());
}

#[test]
fn ignores_builder_even_with_many_setters() {
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
fn ignores_thin_delegation_facade_without_workflow_logic() {
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
        "fn wire(&self) { self.workspaces.call(); self.settings.call(); self.terminal.call(); }",
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
fn ignores_one_protocol_context_with_many_object_stores() {
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
fn ignores_plural_service_that_orchestrates_one_domain() {
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
    let run = "fn run(&mut self) { if self.settings.ready() { self.workspaces.call(); self.terminal.call(); } }";
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
fn sdk_wallet_index_and_rpc_workflows_report_origin_evidence() {
    let source = fixture("Runtime", "split")
        .replace("workspace", "wallet")
        .replace("settings", "indexing")
        .replace("terminal", "rpc");
    let values = findings(&source);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].rule, super::ID);
    assert_eq!(values[0].severity, crate::Severity::Warning);
    let metadata = &values[0].review.as_ref().unwrap().metadata;
    let (_, clusters) = metadata
        .iter()
        .find(|(name, _)| name == "Capability clusters")
        .unwrap();
    assert!(clusters.contains("wallet"));
    assert!(clusters.contains("indexing"));
    assert!(clusters.contains("rpc"));
}

#[test]
fn ambiguous_same_named_types_do_not_share_methods() {
    let source = format!(
        "{} mod unrelated {{ struct Application {{ state: u8 }} }}",
        fixture("Application", "")
    );
    assert!(findings(&source).is_empty());
}
