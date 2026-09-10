use super::{Rule, adopted};
use crate::{LintError, Policy, Result};
use std::collections::BTreeSet;

/// Ordered collection of rules, validated before a linter runs them.
#[derive(Default)]
pub struct Registry {
    rules: Vec<Box<dyn Rule>>,
}
impl Registry {
    /// Starts an empty registry for explicit rule composition.
    ///
    /// ```
    /// use design_lint::{Registry, rule};
    ///
    /// let registry = Registry::new()
    ///     .register(rule::DependencyDirection)
    ///     .register(rule::StructNaming);
    /// registry.validate()?;
    /// # Ok::<(), design_lint::LintError>(())
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a rule in execution order; validation rejects invalid identifiers.
    #[must_use]
    pub fn register(mut self, rule: impl Rule + 'static) -> Self {
        self.rules.push(Box::new(rule));
        self
    }

    /// Rejects empty or duplicate rule identifiers before analysis begins.
    pub fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        for rule in self.iter() {
            let id = rule.id();
            if id.is_empty() || !ids.insert(id) {
                return Err(LintError::configuration(format!(
                    "empty or duplicate rule ID `{id}`"
                )));
            }
        }
        Ok(())
    }

    /// Original SDK checks plus explicitly enabled adopted checks.
    pub fn standard(policy: &Policy) -> Result<Self> {
        policy.validate()?;
        let mut registry = Self::all()?;
        registry.rules.retain(|rule| {
            !adopted::contains(rule.id()) || policy.rules.enabled.iter().any(|id| id == rule.id())
        });
        registry.validate()?;
        Ok(registry)
    }

    /// Full review selection, retaining the original severity of every rule.
    pub fn all() -> Result<Self> {
        let registry = crate::standard_rules();
        registry.validate()?;
        Ok(registry)
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Rule> {
        self.rules.iter().map(Box::as_ref)
    }
}
