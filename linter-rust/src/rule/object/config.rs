use linter::{Error, Selector, Target};
use serde::Deserialize;

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
    #[serde(default = "default_limit")]
    max_methods: usize,
    #[serde(default = "default_fields")]
    min_fields: usize,
    #[serde(default = "default_fields")]
    min_clusters: usize,
    #[serde(default = "default_cluster_methods")]
    min_methods_per_cluster: usize,
    #[serde(default)]
    unwrap_types: Vec<String>,
    #[serde(default)]
    excluded_suffixes: Vec<String>,
}

fn default_fields() -> usize {
    3
}
fn default_cluster_methods() -> usize {
    2
}
fn default_limit() -> usize {
    20
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scope {
    #[default]
    Production,
    Tests,
    All,
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub scope: Scope,
    pub max_methods: usize,
    pub min_fields: usize,
    pub min_clusters: usize,
    pub min_methods_per_cluster: usize,
    pub unwrap_types: Vec<String>,
    pub excluded_suffixes: Vec<String>,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/god-object-growth\"[{index}]");
                if definition.max_methods == 0
                    || definition.min_fields == 0
                    || definition.min_clusters < 2
                    || definition.min_methods_per_cluster == 0
                    || definition
                        .unwrap_types
                        .iter()
                        .chain(&definition.excluded_suffixes)
                        .any(|value| value.trim().is_empty())
                {
                    return Err(Error::Configuration(format!(
                        "{setting}.max_methods: expected a positive integer"
                    )));
                }
                if definition.unwrap_types.iter().any(|name| {
                    !matches!(
                        name.as_str(),
                        "std:Box"
                            | "std:Option"
                            | "std:Arc"
                            | "std:Rc"
                            | "std:Mutex"
                            | "std:RwLock"
                            | "std:RefCell"
                            | "std:SyncWeak"
                            | "std:RcWeak"
                    )
                }) {
                    return Err(Error::Configuration(format!(
                        "{setting}.unwrap_types: expected a supported standard ownership container"
                    )));
                }
                Ok(Assertion {
                    target: definition
                        .target
                        .compile(&format!("{setting}.target"), true)?,
                    exclude: definition
                        .exclude
                        .map(|value| value.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    scope: definition.scope,
                    max_methods: definition.max_methods,
                    min_fields: definition.min_fields,
                    min_clusters: definition.min_clusters,
                    min_methods_per_cluster: definition.min_methods_per_cluster,
                    unwrap_types: definition.unwrap_types,
                    excluded_suffixes: definition.excluded_suffixes,
                    setting,
                })
            })
            .collect()
    }
}
