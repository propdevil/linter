use std::collections::{BTreeSet, HashMap};

use syn::{
    Expr, ExprCall, ExprMacro, FnArg, ItemFn, ItemMod, PathArguments, Type, spanned::Spanned,
    visit::Visit,
};

use crate::{
    Policy, Result,
    model::{Finding, Related, Review},
    rule::references::References,
    source::Workspace,
};

pub(crate) const ID: &str = "unclassified-free-function";

/// Reviews free functions for a cohesive owner.
pub struct FreeFunction;

impl crate::Rule for FreeFunction {
    fn id(&self) -> &'static str {
        ID
    }

    fn severity(&self) -> crate::Severity {
        crate::Severity::Error
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
    let mut candidates = Vec::new();
    let mut definitions = HashMap::<String, usize>::new();
    let mut references = HashMap::<String, Vec<crate::rule::references::Reference>>::new();
    for source in workspace.production() {
        let mut functions = Functions {
            test_scope: false,
            values: Vec::new(),
        };
        functions.visit_file(&source.syntax);
        for value in functions.values {
            *definitions.entry(value.name.clone()).or_default() += 1;
            candidates.push((source, value));
        }
        let mut uses = References::new(source);
        uses.visit_file(&source.syntax);
        for reference in uses.values {
            references
                .entry(reference.name.clone())
                .or_default()
                .push(reference);
        }
    }
    Ok(candidates
        .into_iter()
        .map(|(source, candidate)| {
            let ambiguous = definitions.get(&candidate.name).copied().unwrap_or(0) != 1;
            let usages = references
                .get(&candidate.name)
                .into_iter()
                .flatten()
                .filter(|usage| !ambiguous || usage.location.path == source.path);
            candidate.finding(ID, source, usages, ambiguous)
        })
        .collect())
}

struct Candidate {
    name: String,
    arguments: usize,
    span: proc_macro2::Span,
    dependencies: Vec<String>,
    framework_signature: bool,
}

impl Candidate {
    fn finding<'a>(
        self,
        rule: &'static str,
        source: &crate::source::SourceFile,
        usages: impl Iterator<Item = &'a crate::rule::references::Reference>,
        ambiguous: bool,
    ) -> Finding {
        let mut finding = Finding::error(rule, &self.name, source.location(self.span));
        finding.message = format!(
            "unclassified free function `{}` has {} argument{}",
            self.name,
            self.arguments,
            if self.arguments == 1 { "" } else { "s" }
        );
        finding.help =
            "move behavior to its meaningful owner, or document an exact reasoned design-lint allow exception after reviewing ownership".into();
        finding.related = usages
            .map(|usage| Related {
                label: usage.context.as_ref().map_or_else(
                    || "usage at module scope".into(),
                    |context| format!("usage in `{}`\n{}", context.name, context.source),
                ),
                location: usage.location.clone(),
            })
            .collect();
        let review = Review {
            metadata: vec![
                ("Arguments".into(), self.arguments.to_string()),
                ("Framework-shaped signature".into(), self.framework_signature.to_string()),
                (
                    "Usage resolution".into(),
                    if ambiguous {
                        "ambiguous name; same-file references only"
                    } else {
                        "unique name in scanned tree"
                    }
                    .into(),
                ),
            ],
            dependencies: self.dependencies,
            questions: vec![
                "Does one argument already have a meaningful receiver type?".into(),
                "Do related functions share this value and its invariants?".into(),
                "Would a wrapper collect cohesive behavior, or only hide one helper?".into(),
                "Is this a complete low-level algorithm that should remain free?".into(),
                "If this has a framework-shaped signature, is it a thin handler in the application boundary with a precise reviewed exception?".into(),
            ],
        };
        finding.review = Some(review);
        finding
    }
}

struct Functions {
    test_scope: bool,
    values: Vec<Candidate>,
}

impl<'ast> Visit<'ast> for Functions {
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if !crate::rule::production::test_only(&item.attrs) {
            syn::visit::visit_item_impl(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !crate::rule::production::test_only(&item.attrs) {
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_item_fn(&mut self, function: &'ast ItemFn) {
        if crate::rule::production::test_only(&function.attrs) {
            return;
        }
        if !self.test_scope
            && !crate::rule::production::test_only(&function.attrs)
            && candidate(function)
        {
            let mut dependencies = Dependencies::default();
            dependencies.visit_item_fn(function);
            let span = function
                .attrs
                .first()
                .and_then(|attribute| attribute.span().join(function.span()))
                .unwrap_or_else(|| function.span());
            self.values.push(Candidate {
                name: function.sig.ident.to_string(),
                arguments: function.sig.inputs.len(),
                span,
                dependencies: dependencies.names.into_iter().collect(),
                framework_signature: framework_signature(function),
            });
        }
        syn::visit::visit_item_fn(self, function);
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        let previous = self.test_scope;
        self.test_scope |= crate::rule::production::test_only(&module.attrs);
        syn::visit::visit_item_mod(self, module);
        self.test_scope = previous;
    }
}

fn candidate(function: &ItemFn) -> bool {
    function.sig.abi.is_none()
        && matches!(function.sig.inputs.len(), 1 | 2)
        && !function.attrs.iter().any(|attribute| {
            let path = attribute.path();
            path.is_ident("proc_macro")
                || path.is_ident("proc_macro_attribute")
                || path.is_ident("proc_macro_derive")
        })
}

fn framework_signature(function: &ItemFn) -> bool {
    function.sig.inputs.iter().any(|argument| {
        let FnArg::Typed(argument) = argument else {
            return false;
        };
        let Type::Path(ty) = argument.ty.as_ref() else {
            return false;
        };
        ty.path.segments.last().is_some_and(|segment| {
            let name = segment.ident.to_string();
            let generic = matches!(segment.arguments, PathArguments::AngleBracketed(_));
            (generic
                && matches!(
                    name.as_str(),
                    "State" | "Path" | "Query" | "Json" | "Form" | "Extension" | "ConnectInfo"
                ))
                || matches!(
                    name.as_str(),
                    "OriginalUri" | "RawQuery" | "WebSocketUpgrade" | "Multipart"
                )
        })
    })
}

#[derive(Default)]
struct Dependencies {
    names: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Dependencies {
    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(function) = call.func.as_ref() {
            self.names.insert(path_name(&function.path));
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        self.names.insert(format!(".{}", call.method));
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_macro(&mut self, expression: &'ast ExprMacro) {
        self.names
            .insert(format!("{}!", path_name(&expression.mac.path)));
        syn::visit::visit_expr_macro(self, expression);
    }
}

fn path_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests;
