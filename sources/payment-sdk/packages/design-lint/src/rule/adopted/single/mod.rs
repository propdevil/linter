use std::collections::HashMap;

use syn::{ItemFn, ItemMod, spanned::Spanned, visit::Visit};

use crate::{
    Policy, Result,
    model::{Finding, Related},
    rule::references::References,
    source::{SourceFile, Workspace},
};

pub(crate) const ID: &str = "single-use-free-function";

/// Reports free functions with a single local caller.
pub struct SingleUse;

impl crate::Rule for SingleUse {
    fn id(&self) -> &'static str {
        ID
    }

    fn severity(&self) -> crate::Severity {
        crate::Severity::Warning
    }

    fn check(
        &self,
        workspace: &crate::source::Workspace,
        policy: &crate::Policy,
    ) -> crate::Result<Vec<crate::Finding>> {
        check(workspace, policy)
    }
}

pub(crate) fn check(workspace: &Workspace, _policy: &Policy) -> Result<Vec<Finding>> {
    let mut definitions = Vec::new();
    let mut references = Vec::new();
    for source in workspace.production() {
        let mut visitor = Definitions {
            source,
            test_scope: false,
            values: Vec::new(),
        };
        visitor.visit_file(&source.syntax);
        definitions.extend(visitor.values);
        let mut uses = References::new(source);
        uses.visit_file(&source.syntax);
        references.extend(uses.values);
    }
    let mut counts = HashMap::new();
    for definition in &definitions {
        *counts.entry(definition.name.clone()).or_insert(0_usize) += 1;
    }
    let mut uses = HashMap::<&str, Vec<&crate::rule::references::Reference>>::new();
    for reference in &references {
        uses.entry(&reference.name).or_default().push(reference);
    }
    Ok(definitions
        .into_iter()
        .filter_map(|definition| {
            if counts.get(&definition.name) != Some(&1) {
                return None;
            }
            let external = uses
                .get(definition.name.as_str())
                .into_iter()
                .flatten()
                .filter(|reference| {
                    reference
                        .context
                        .as_ref()
                        .map(|context| context.name.as_str())
                        != Some(definition.name.as_str())
                })
                .collect::<Vec<_>>();
            (external.len() == 1).then(|| definition.finding(ID, external[0]))
        })
        .collect())
}

struct Definition {
    name: String,
    location: crate::Location,
}
impl Definition {
    fn finding(self, rule: &'static str, usage: &crate::rule::references::Reference) -> Finding {
        let mut finding = Finding::warning(rule, &self.name, self.location);
        finding.message = format!("private free function `{}` has one use", self.name);
        finding.help = "inline it at the use site, or document a deliberate section boundary using an exact reasoned design-lint allow comment after review".into();
        finding.related.push(Related {
            label: usage.context.as_ref().map_or_else(
                || "sole use".into(),
                |context| format!("sole use in `{}`\n{}", context.name, context.source),
            ),
            location: usage.location.clone(),
        });
        finding
    }
}

struct Definitions<'a> {
    source: &'a SourceFile,
    test_scope: bool,
    values: Vec<Definition>,
}
impl<'ast> Visit<'ast> for Definitions<'_> {
    fn visit_item_fn(&mut self, function: &'ast ItemFn) {
        if crate::rule::production::test_only(&function.attrs) {
            return;
        }
        if !self.test_scope
            && !crate::rule::production::test_only(&function.attrs)
            && candidate(function)
        {
            let span = function
                .attrs
                .first()
                .and_then(|attribute| attribute.span().join(function.span()))
                .unwrap_or_else(|| function.span());
            self.values.push(Definition {
                name: function.sig.ident.to_string(),
                location: self.source.location(span),
            });
        }
        syn::visit::visit_item_fn(self, function)
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        let previous = self.test_scope;
        self.test_scope |= crate::rule::production::test_only(&module.attrs);
        syn::visit::visit_item_mod(self, module);
        self.test_scope = previous;
    }
}
fn candidate(function: &ItemFn) -> bool {
    function.sig.ident != "main"
        && function.sig.abi.is_none()
        && matches!(function.vis, syn::Visibility::Inherited)
        && !function
            .attrs
            .iter()
            .any(|a| a.path().is_ident("test") || a.path().is_ident("bench"))
}

#[cfg(test)]
mod tests;
