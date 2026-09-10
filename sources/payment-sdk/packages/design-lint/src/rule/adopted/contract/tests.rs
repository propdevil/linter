use crate::{policy::Policy, test_support::Fixture};

fn findings(source: &str) -> Vec<crate::Finding> {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    super::check(&fixture.workspace(), &Policy::default()).unwrap()
}

#[test]
fn reports_substantial_unrelated_capability_clusters() {
    let findings = findings(
        r#"
pub trait RuntimeService {
    fn create(&self, config: Config) -> Result<Id, Error>;
    fn remove(&self, id: Id) -> Result<(), Error>;
    fn start(&self, id: Id) -> Result<(), Error>;
    fn stop(&self, id: Id) -> Result<(), Error>;
    fn configure(&self, id: Id, config: Config) -> Result<(), Error>;
    fn update(&self, id: Id, config: Config) -> Result<(), Error>;
    fn inspect(&self, id: Id) -> Result<Snapshot, Error>;
    fn status(&self, id: Id) -> Result<Status, Error>;
}

impl RuntimeService for Host {
    fn create(&self, _: Config) -> Result<Id, Error> { todo!() }
    fn remove(&self, _: Id) -> Result<(), Error> { todo!() }
    fn start(&self, _: Id) -> Result<(), Error> { todo!() }
    fn stop(&self, _: Id) -> Result<(), Error> { todo!() }
    fn configure(&self, _: Id, _: Config) -> Result<(), Error> { todo!() }
    fn update(&self, _: Id, _: Config) -> Result<(), Error> { todo!() }
    fn inspect(&self, _: Id) -> Result<Snapshot, Error> { todo!() }
    fn status(&self, _: Id) -> Result<Status, Error> { todo!() }
}
"#,
    );
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.subject, "RuntimeService");
    assert!(finding.message.contains("4 distinct capability clusters"));
    assert!(
        finding
            .related
            .iter()
            .any(|related| related.label.contains("implemented by `Host`"))
    );
    let review = finding.review.as_ref().unwrap();
    assert!(
        review
            .metadata
            .iter()
            .any(|(name, value)| name == "Capability clusters"
                && value.contains("lifecycle: start, stop"))
    );
}

#[test]
fn noun_clusters_expose_cross_capability_presenters() {
    let findings = findings(
        r#"
pub trait Presenter {
    fn poll_events(&mut self);
    fn take_events(&mut self) -> Vec<Event>;
    fn set_clipboard_text(&mut self, text: &str);
    fn take_clipboard_text(&mut self) -> Option<String>;
    fn reconcile_window(&mut self, state: &WindowState);
    fn destroy_window(&mut self, id: SurfaceId);
    fn begin_interaction(&mut self, id: SurfaceId, interaction: Interaction);
    fn present(&mut self, image: Image) -> Feedback;
}
"#,
    );
    assert_eq!(findings.len(), 1);
    let clusters = findings[0]
        .review
        .as_ref()
        .unwrap()
        .metadata
        .iter()
        .find(|(name, _)| name == "Capability clusters")
        .map(|(_, value)| value.as_str())
        .unwrap();
    assert!(clusters.contains("clipboard: set_clipboard_text, take_clipboard_text"));
    assert!(clusters.contains("events: poll_events, take_events"));
    assert!(clusters.contains("window: reconcile_window, destroy_window, begin_interaction"));
}

#[test]
fn method_count_and_vague_name_are_not_sufficient() {
    let findings = findings(
        r#"
pub trait RecordRepository {
    fn create(&self, value: Record) -> Result<Id, Error>;
    fn open(&self, id: Id) -> Result<Record, Error>;
    fn read(&self, id: Id) -> Result<Record, Error>;
    fn write(&self, value: Record) -> Result<(), Error>;
    fn save(&self, value: Record) -> Result<(), Error>;
    fn load(&self, id: Id) -> Result<Record, Error>;
    fn delete(&self, id: Id) -> Result<(), Error>;
    fn list(&self) -> Result<Vec<Record>, Error>;
    fn find(&self, query: Query) -> Result<Vec<Record>, Error>;
}
"#,
    );
    assert!(findings.is_empty());
}

#[test]
fn preserves_cohesive_protocol_codec_visitor_and_renderer_contracts() {
    let findings = findings(
        r#"
pub trait Codec {
    fn encode_header(&self, value: Header) -> Bytes;
    fn encode_body(&self, value: Body) -> Bytes;
    fn encode_tail(&self, value: Tail) -> Bytes;
    fn decode_header(&self, value: Bytes) -> Header;
    fn decode_body(&self, value: Bytes) -> Body;
    fn decode_tail(&self, value: Bytes) -> Tail;
    fn serialize(&self, value: Frame) -> Bytes;
    fn deserialize(&self, value: Bytes) -> Frame;
}
pub trait Visitor {
    fn visit_a(&mut self, value: A);
    fn visit_b(&mut self, value: B);
    fn visit_c(&mut self, value: C);
    fn visit_d(&mut self, value: D);
    fn visit_e(&mut self, value: E);
    fn visit_f(&mut self, value: F);
    fn visit_g(&mut self, value: G);
    fn visit_h(&mut self, value: H);
}
pub trait Renderer {
    fn render_a(&mut self, value: A);
    fn render_b(&mut self, value: B);
    fn draw_a(&mut self, value: A);
    fn draw_b(&mut self, value: B);
    fn present_a(&mut self, value: A);
    fn present_b(&mut self, value: B);
    fn frame_a(&mut self, value: A);
    fn frame_b(&mut self, value: B);
}
pub trait SurfaceProtocol {
    fn create_surface(&self, id: SurfaceId);
    fn remove_surface(&self, id: SurfaceId);
    fn configure_surface(&self, id: SurfaceId);
    fn update_surface(&self, id: SurfaceId);
    fn present_surface(&self, id: SurfaceId);
    fn commit_surface(&self, id: SurfaceId);
    fn inspect_surface(&self, id: SurfaceId) -> SurfaceSnapshot;
    fn status_surface(&self, id: SurfaceId) -> SurfaceStatus;
}
"#,
    );
    assert!(findings.is_empty());
}

#[test]
fn ignores_unsafe_generated_sealed_and_test_traits() {
    let findings = findings(
        r#"
pub unsafe trait Abi {
    fn create(&self); fn remove(&self);
    fn start(&self); fn stop(&self);
    fn inspect(&self); fn status(&self);
    fn configure(&self); fn update(&self);
}
pub trait Hidden: sealed::Sealed {
    fn create(&self); fn remove(&self);
    fn start(&self); fn stop(&self);
    fn inspect(&self); fn status(&self);
    fn configure(&self); fn update(&self);
}
#[cfg(test)]
pub trait Fixture {
    fn create(&self); fn remove(&self);
    fn start(&self); fn stop(&self);
    fn inspect(&self); fn status(&self);
    fn configure(&self); fn update(&self);
}
"#,
    );
    assert!(findings.is_empty());
}

#[test]
fn does_not_report_sparse_name_guessing() {
    let findings = findings(
        r#"
pub trait Odd {
    fn create(&self);
    fn remove(&self);
    fn start(&self);
    fn stop(&self);
    fn alpha(&self);
    fn beta(&self);
    fn gamma(&self);
    fn delta(&self);
    fn epsilon(&self);
    fn zeta(&self);
}
"#,
    );
    assert!(findings.is_empty());
}

#[test]
fn sdk_wallet_and_sync_contract_collects_related_capabilities() {
    let values = findings(
        r#"
trait Runtime {
    fn load_wallet(&self, id: WalletId) -> Wallet;
    fn save_wallet(&self, wallet: Wallet);
    fn start_sync(&self, scope: Scope);
    fn stop_sync(&self, scope: Scope);
    fn inspect_checkpoint(&self, scope: Scope) -> Checkpoint;
    fn query_height(&self, scope: Scope) -> Height;
    fn configure_rpc(&self, endpoint: Endpoint);
    fn update_rpc(&self, timeout: Timeout);
}
"#,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].related.len(), 8);
    assert_eq!(values[0].subject, "Runtime");
    assert!(
        values[0]
            .review
            .as_ref()
            .unwrap()
            .metadata
            .iter()
            .any(|(name, value)| name == "Capability clusters" && value.contains("lifecycle"))
    );
}

#[test]
fn four_method_sdk_trait_is_left_to_the_existing_strict_gate() {
    assert!(findings(
        "trait Wallet { fn address(&self); fn history(&self); fn transfer(&self); fn balance(&self); }",
    ).is_empty());
}

#[test]
fn retained_trait_rule_keeps_maximum_three_and_attaches_decorated_trait_evidence() {
    let fixture = Fixture::new(&[(
        "src/lib.rs",
        r#"
trait Wallet {
    fn address(&self);
    fn history(&self);
    fn transfer(&self);
    fn balance(&self);
}
/// Coordinates distinct capabilities.
#[allow(dead_code)]
#[must_use]
#[doc = "Each capability has its own input and output."]
trait Runtime {
    fn load_wallet(&self, id: WalletId) -> Wallet;
    fn save_wallet(&self, wallet: Wallet);
    fn start_sync(&self, scope: Scope);
    fn stop_sync(&self, scope: Scope);
    fn inspect_checkpoint(&self, scope: Scope) -> Checkpoint;
    fn query_height(&self, scope: Scope) -> Height;
    fn configure_rpc(&self, endpoint: Endpoint);
    fn update_rpc(&self, timeout: Timeout);
}
"#,
    )]);
    let workspace = fixture.workspace();
    let registry = crate::Registry::all().unwrap();
    let rule = registry
        .iter()
        .find(|rule| rule.id() == "trait-method-count")
        .unwrap();
    let found = rule.check(&workspace, &Policy::default()).unwrap();
    assert_eq!(found.len(), 2);
    let wallet = found
        .iter()
        .find(|finding| finding.subject == "Wallet")
        .unwrap();
    assert_eq!(wallet.rule, "trait-method-count");
    assert_eq!(wallet.severity, crate::Severity::Error);
    assert!(wallet.message.contains("4 functions; maximum is 3"));
    assert!(wallet.related.is_empty());
    let runtime = found
        .iter()
        .find(|finding| finding.subject == "Runtime")
        .unwrap();
    assert_eq!(runtime.rule, "trait-method-count");
    assert_eq!(runtime.severity, crate::Severity::Error);
    assert_eq!(runtime.location.line, 12);
    assert_eq!(runtime.related.len(), 8);
    assert!(runtime.review.is_some());
}

#[test]
fn test_only_methods_do_not_supply_capability_evidence() {
    assert!(
        findings(
            r#"
trait Runtime {
    fn load_wallet(&self, id: WalletId) -> Wallet;
    fn save_wallet(&self, wallet: Wallet);
    #[cfg(test)] fn start_sync(&self, scope: Scope);
    #[cfg(test)] fn stop_sync(&self, scope: Scope);
    #[cfg(test)] fn inspect_checkpoint(&self, scope: Scope) -> Checkpoint;
    #[cfg(test)] fn query_height(&self, scope: Scope) -> Height;
    #[cfg(test)] fn configure_rpc(&self, endpoint: Endpoint);
    #[cfg(test)] fn update_rpc(&self, timeout: Timeout);
}
"#,
        )
        .is_empty()
    );
}
