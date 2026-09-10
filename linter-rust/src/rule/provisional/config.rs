use linter::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    exclude: Option<Target>,
    terms: Vec<String>,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub terms: Vec<String>,
    pub setting: String,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| value.compile(index))
            .collect()
    }
}
impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let setting = format!("rules.\"rust/provisional-diagnostic\"[{index}]");
        let mut unique = std::collections::BTreeSet::new();
        if self.terms.is_empty()
            || self
                .terms
                .iter()
                .any(|term| term.trim().is_empty() || !unique.insert(term.to_ascii_lowercase()))
        {
            return Err(Error::Configuration(format!(
                "{setting}.terms: expected nonempty unique terms"
            )));
        }
        Ok(Assertion {
            target: self.target.compile(&format!("{setting}.target"), true)?,
            exclude: self
                .exclude
                .map(|target| target.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            terms: self
                .terms
                .into_iter()
                .map(|term| term.to_ascii_lowercase())
                .collect(),
            setting,
        })
    }
}
