use linter::{Error, Selector, Target};
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    #[serde(default)]
    exclude: Option<Target>,
    #[serde(default)]
    scope: Scope,
    #[serde(default = "methods")]
    min_methods: usize,
    #[serde(default = "clusters")]
    min_clusters: usize,
    #[serde(default = "cluster_methods")]
    min_methods_per_cluster: usize,
    capabilities: Vec<Capability>,
    #[serde(default)]
    ignored_type_words: BTreeSet<String>,
    #[serde(default)]
    cohesive_suffixes: Vec<String>,
    #[serde(default)]
    generated_markers: Vec<String>,
    #[serde(default)]
    generated_attributes: Vec<String>,
}
fn methods() -> usize {
    8
}
fn clusters() -> usize {
    3
}
fn cluster_methods() -> usize {
    2
}
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scope {
    #[default]
    Production,
    Tests,
    All,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capability {
    pub name: String,
    #[serde(default)]
    pub verbs: Vec<String>,
    #[serde(default)]
    pub nouns: Vec<String>,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub scope: Scope,
    pub min_methods: usize,
    pub min_clusters: usize,
    pub min_methods_per_cluster: usize,
    pub capabilities: Vec<Capability>,
    pub ignored_type_words: BTreeSet<String>,
    pub cohesive_suffixes: Vec<String>,
    pub generated_markers: Vec<String>,
    pub generated_attributes: Vec<String>,
    pub setting: String,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                definition.compile(format!(
                    "rules.\"rust/broad-trait-responsibilities\"[{index}]"
                ))
            })
            .collect()
    }
}
impl Definition {
    fn compile(self, setting: String) -> Result<Assertion, Error> {
        if self.min_methods == 0
            || self.min_clusters < 2
            || self.min_methods_per_cluster == 0
            || self
                .min_clusters
                .checked_mul(self.min_methods_per_cluster)
                .is_none_or(|minimum| minimum > self.min_methods)
            || self.capabilities.len() < self.min_clusters
        {
            return Err(Error::Configuration(format!(
                "{setting}: positive thresholds must fit min_methods, with at least two supported clusters"
            )));
        }
        vocabulary(&self.capabilities, &setting)?;
        for values in [
            &self.cohesive_suffixes,
            &self.generated_markers,
            &self.generated_attributes,
        ] {
            if values.iter().any(|value| value.trim().is_empty()) {
                return Err(Error::Configuration(format!(
                    "{setting}: empty vocabulary entries are invalid"
                )));
            }
        }
        if self.ignored_type_words.iter().any(|word| !token(word)) {
            return Err(Error::Configuration(format!(
                "{setting}.ignored_type_words: expected lowercase words"
            )));
        }
        Ok(Assertion {
            target: self.target.compile(&format!("{setting}.target"), true)?,
            exclude: self
                .exclude
                .map(|value| value.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            scope: self.scope,
            min_methods: self.min_methods,
            min_clusters: self.min_clusters,
            min_methods_per_cluster: self.min_methods_per_cluster,
            capabilities: self.capabilities,
            ignored_type_words: self.ignored_type_words,
            cohesive_suffixes: self.cohesive_suffixes,
            generated_markers: self.generated_markers,
            generated_attributes: self.generated_attributes,
            setting,
        })
    }
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}
fn vocabulary(capabilities: &[Capability], setting: &str) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    let mut verbs = BTreeSet::new();
    let mut nouns = BTreeSet::new();
    for capability in capabilities {
        if capability.name.trim().is_empty()
            || !names.insert(&capability.name)
            || (capability.verbs.is_empty() && capability.nouns.is_empty())
        {
            return Err(Error::Configuration(format!(
                "{setting}.capabilities: expected unique names and nonempty matchers"
            )));
        }
        for (words, seen) in [
            (&capability.verbs, &mut verbs),
            (&capability.nouns, &mut nouns),
        ] {
            if words.iter().any(|word| !token(word) || !seen.insert(word)) {
                return Err(Error::Configuration(format!(
                    "{setting}.capabilities: ambiguous or invalid vocabulary"
                )));
            }
        }
    }
    Ok(())
}
