use serde::de::DeserializeOwned;

use crate::{Error, Finding, Project, Status};

pub(crate) mod layout;

/// A configured check. Registration supplies its settings and controls enabling.
pub trait Rule: Send + Sync + Sized + 'static {
    const ID: &'static str;
    type Analysis: Analysis;
    type Config: DeserializeOwned + Default;

    fn new(config: Self::Config) -> Result<Self, Error>;
    fn configured(&self) -> bool {
        true
    }
    fn check(&self, project: &Project, analysis: &Self::Analysis) -> Result<RuleResult, Error>;
}

#[derive(Debug)]
pub struct RuleResult {
    pub status: Status,
    pub findings: Vec<Finding>,
}

/// Input prepared once per check run and shared by rules with the same analysis type.
pub trait Analysis: Send + Sync + 'static {
    fn load(project: &Project) -> Result<Self, Error>
    where
        Self: Sized;
}

impl Analysis for () {
    fn load(_: &Project) -> Result<Self, Error> {
        Ok(())
    }
}

pub(crate) mod affix;
pub(crate) mod filename;
pub(crate) mod forbidden_words;
