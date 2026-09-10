use crate::{policy::Policy, test_support::Fixture};

fn findings(source: &str) -> Vec<crate::Finding> {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    super::check(&fixture.workspace(), &Policy::default()).unwrap()
}

#[test]
fn reports_each_proven_discard_form_including_closure_blocks() {
    let values = findings(
        r#"
fn fail() -> Result<(), Error> { todo!() }
fn run() {
    let _ = fail();
    fail();
    drop(fail());
    fail().ok();
    let closure = || { fail(); };
}
"#,
    );
    assert_eq!(values.len(), 5);
    assert!(values.iter().all(crate::Finding::is_violation));
}

#[test]
fn reports_unambiguous_result_methods_and_awaits() {
    let values = findings(
        r#"
struct Store;
impl Store {
    fn save(&self) -> std::io::Result<()> { todo!() }
    fn run(&self) {
        self.save();
        Store::save(self);
    }
}
async fn run() {
    async_result().await;
}
async fn async_result() -> Result<(), Error> { todo!() }
"#,
    );
    assert_eq!(values.len(), 3);
}

#[test]
fn explicit_result_constructors_need_no_declaration() {
    let values = findings(
        r#"
fn run() {
    Result::Err::<(), Error>(error());
    let _ = Result::Ok::<(), Error>(());
}
"#,
    );
    assert_eq!(values.len(), 2);
}

#[test]
fn ignores_handled_results_and_option_discards() {
    let values = findings(
        r#"
fn fail() -> Result<(), Error> { todo!() }
fn maybe() -> Option<()> { None }
fn run() -> Result<(), Error> {
    fail()?;
    match fail() { Ok(()) => {}, Err(error) => log(error) }
    if let Err(error) = fail() { log(error); }
    let result = fail();
    let _ = maybe();
    maybe();
    drop(maybe());
    Ok(())
}
"#,
    );
    assert!(values.is_empty());
}

#[test]
fn ambiguous_names_are_not_syntactic_proof() {
    let values = findings(
        r#"
fn operation() -> Result<(), Error> { todo!() }
mod other { fn operation() {} }
struct First;
impl First { fn save(&self) -> Result<(), Error> { todo!() } }
struct Second;
impl Second { fn save(&self) {} }
fn run(first: First) {
    operation();
    first.save();
    unknown_but_fallible_sounding();
}
"#,
    );
    assert!(values.is_empty());
}

#[test]
fn excludes_test_modules_and_functions() {
    let values = findings(
        r#"
fn fail() -> Result<(), Error> { todo!() }
#[cfg(test)]
mod tests { fn case() { fail(); } }
#[test]
fn test_function() { fail(); }
"#,
    );
    assert!(values.is_empty());
}

#[test]
fn sdk_broadcast_result_has_exact_error_and_declaration_evidence() {
    let values =
        findings("fn broadcast() -> Result<TxId, Error> { todo!() }\nfn submit() { broadcast(); }");
    assert_eq!(values.len(), 1);
    let finding = &values[0];
    assert_eq!(finding.rule, super::ID);
    assert_eq!(finding.severity, crate::Severity::Error);
    assert_eq!(finding.location.line, 2);
    assert_eq!(finding.related.len(), 1);
    assert_eq!(finding.related[0].location.line, 1);
    assert!(finding.related[0].location.source.contains("Result"));
}

#[test]
fn test_only_impl_does_not_prove_or_report_production_results() {
    let values = findings(
        r#"
struct Wallet;
#[cfg(all(test, feature = "fixtures"))]
impl Wallet {
    fn submit(&self) -> Result<(), Error> { todo!() }
    fn fixture(&self) { self.submit(); }
}
fn exercise(wallet: Wallet) { Wallet::submit(&wallet); }
"#,
    );
    assert!(values.is_empty());
}
