use crate::{Policy, test_support::Fixture};

fn findings(source: &str) -> Vec<crate::Finding> {
    let fixture = Fixture::new(&[("src/lib.rs", source)]);
    super::check(&fixture.workspace(), &Policy::default()).unwrap()
}

// Adapted from the donor crate's src/lib.rs regression test.
#[test]
fn duplicate_entity_rule_requires_identity_relation_and_three_typed_fields() {
    let values = findings(
        r#"
struct Image { id: u64, name: String, path: String }
struct DiscoveredImage { id: u64, name: String, path: String, score: u8 }
struct Unrelated { id: u64, name: String, path: String }
struct WrongTypes { id: String, name: String, path: String }
"#,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].subject, "Image_DiscoveredImage");
    assert_eq!(values[0].related.len(), 1);
}

#[test]
fn sdk_wallet_specialization_repeats_identity_and_keeps_evidence() {
    let values = findings(
        r#"
struct Wallet { id: WalletId, address: Address, chain: Chain }
struct ImportedWallet { id: WalletId, address: Address, chain: Chain, birthday: u64 }
struct GeneratedWallet { wallet: Wallet, birthday: u64 }
"#,
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].rule, super::ID);
    assert_eq!(values[0].severity, crate::Severity::Error);
    assert_eq!(values[0].location.line, 2);
    assert_eq!(values[0].related[0].location.line, 3);
    assert!(
        values[0]
            .review
            .as_ref()
            .unwrap()
            .metadata
            .iter()
            .any(|(name, value)| name == "Common fields" && value.contains("id: WalletId"))
    );
}

#[test]
fn identical_shapes_in_distinct_packages_are_independent() {
    let model = "struct Wallet { id: u64, address: String, network: String }";
    let fixture = Fixture::new(&[
        (
            "sdk/wallets/Cargo.toml",
            "[package]\nname='wallets'\nversion='0.0.0'\n",
        ),
        ("sdk/wallets/src/lib.rs", model),
        (
            "apps/api/Cargo.toml",
            "[package]\nname='api'\nversion='0.0.0'\n",
        ),
        ("apps/api/src/lib.rs", model),
    ]);
    assert!(
        super::check(&fixture.workspace(), &Policy::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_only_local_structs_are_not_production_duplicates() {
    let values = findings(
        r#"
struct Wallet { id: u64, address: String, network: String }
#[test]
fn fixture() {
    struct ImportedWallet { id: u64, address: String, network: String }
}
"#,
    );
    assert!(values.is_empty());
}
