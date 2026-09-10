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
    #[serde(default)]
    ignored_names: Vec<String>,
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
    pub ignored_names: std::collections::BTreeSet<String>,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/receiver-name-repetition\"[{index}]");
                let mut words = std::collections::BTreeSet::new();
                for word in definition.ignored_names {
                    if word.is_empty()
                        || syn::parse_str::<syn::Ident>(&word).is_err()
                        || !words.insert(word.trim_start_matches("r#").to_owned())
                    {
                        return Err(Error::Configuration(format!(
                            "{setting}.ignored_names: expected unique Rust method identifiers"
                        )));
                    }
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
                    ignored_names: words,
                    setting,
                })
            })
            .collect()
    }
}
