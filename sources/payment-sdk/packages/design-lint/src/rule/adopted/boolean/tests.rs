use crate::{policy::Policy, test_support::Fixture};

fn findings(source: &str) -> Vec<crate::Finding> {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    super::check(&fixture.workspace(), &Policy::default()).unwrap()
}

#[test]
fn reports_repeated_mutually_exclusive_constructions() {
    let findings = findings(
        r#"
struct Connection {
    disconnected: bool,
    connecting: bool,
    connected: bool,
}
fn disconnected() -> Connection {
    Connection { disconnected: true, connecting: false, connected: false }
}
fn connected() -> Connection {
    Connection { disconnected: false, connecting: false, connected: true }
}
"#,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].subject, "Connection");
    assert_eq!(findings[0].related.len(), 2);
}

#[test]
fn reports_method_that_coordinates_state_flags() {
    let findings = findings(
        r#"
struct Session { idle: bool, opening: bool, active: bool }
impl Session {
    fn activate(&mut self) {
        self.idle = false;
        self.opening = false;
        self.active = true;
    }
}
"#,
    );
    assert_eq!(findings.len(), 1);
    assert!(findings[0].related[0].label.contains("activate"));
}

#[test]
fn ignores_three_independent_capabilities() {
    assert!(
        findings(
            r#"
struct Permissions { readable: bool, writable: bool, executable: bool }
fn owner() -> Permissions {
    Permissions { readable: true, writable: true, executable: true }
}
"#
        )
        .is_empty()
    );
}

#[test]
fn ignores_independent_feature_toggles() {
    assert!(
        findings(
            r#"
struct Features { clipboard: bool, audio: bool, gpu: bool }
impl Features { fn disable_audio(&mut self) { self.audio = false; } }
"#
        )
        .is_empty()
    );
}

#[test]
fn ignores_all_false_reset_of_independent_features() {
    assert!(
        findings(
            r#"
struct Features { clipboard: bool, audio: bool, gpu: bool }
impl Features {
    fn disable_all(&mut self) {
        self.clipboard = false;
        self.audio = false;
        self.gpu = false;
    }
}
"#
        )
        .is_empty()
    );
}

#[test]
fn reports_distinct_one_hot_transition_methods() {
    let findings = findings(
        r#"
struct Phase { queued: bool, running: bool, finished: bool }
impl Phase {
    fn start(&mut self) {
        self.queued = false;
        self.running = true;
        self.finished = false;
    }
    fn finish(&mut self) {
        self.queued = false;
        self.running = false;
        self.finished = true;
    }
}
"#,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].related.len(), 2);
}

#[test]
fn ignores_bitflag_like_protocol_fields() {
    assert!(
        findings(
            r#"
struct ProtocolFlags { urgent: bool, acknowledged: bool, compressed: bool }
fn decode(bits: u8) -> ProtocolFlags {
    ProtocolFlags {
        urgent: bits & 1 != 0,
        acknowledged: bits & 2 != 0,
        compressed: bits & 4 != 0,
    }
}
"#
        )
        .is_empty()
    );
}

#[test]
fn one_construction_is_not_enough_evidence() {
    assert!(
        findings(
            r#"
struct View { loading: bool, ready: bool, failed: bool }
fn initial() -> View { View { loading: true, ready: false, failed: false } }
"#
        )
        .is_empty()
    );
}

#[test]
fn ignores_two_coordinated_fields_and_unrelated_third_flag() {
    assert!(
        findings(
            r#"
struct Transfer { paused: bool, active: bool, verbose: bool }
impl Transfer {
    fn resume(&mut self) {
        self.paused = false;
        self.active = true;
    }
}
"#
        )
        .is_empty()
    );
}

#[test]
fn reports_explicit_invalid_combinations() {
    let findings = findings(
        r#"
struct Phase { queued: bool, running: bool, finished: bool }
impl Phase {
    fn valid(&self) -> bool {
        !(self.queued && self.running)
            && !(self.running && self.finished)
    }
}
"#,
    );
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].related[0]
            .label
            .contains("rejects mutually active")
    );
}

#[test]
fn multiple_active_actions_are_not_mutually_exclusive_state() {
    assert!(
        findings(
            r#"
struct Actions { notify: bool, persist: bool, retry: bool }
fn success() -> Actions {
    Actions { notify: true, persist: true, retry: false }
}
fn failure() -> Actions {
    Actions { notify: true, persist: false, retry: true }
}
"#
        )
        .is_empty()
    );
}

#[test]
fn sdk_sync_phase_evidence_remains_a_warning() {
    let values = findings(
        r#"
struct Synchronizer { catching_up: bool, ready: bool, failed: bool }
impl Synchronizer {
    fn ready(&mut self) {
        self.catching_up = false;
        self.ready = true;
        self.failed = false;
    }
}
"#,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].rule, super::ID);
    assert_eq!(values[0].severity, crate::Severity::Warning);
    assert_eq!(values[0].location.line, 2);
    assert!(values[0].review.as_ref().unwrap().questions.len() >= 2);
}

#[test]
fn ignores_test_only_constructions_and_transitions() {
    assert!(
        findings(
            r#"
struct Phase { queued: bool, running: bool, finished: bool }
#[test]
fn combinations() {
    let _ = Phase { queued: true, running: false, finished: false };
    let _ = Phase { queued: false, running: true, finished: false };
}
impl Phase {
    #[cfg(test)]
    fn fixture(&mut self) {
        self.queued = false;
        self.running = true;
        self.finished = false;
    }
}
"#,
        )
        .is_empty()
    );
}
