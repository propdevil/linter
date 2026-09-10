use proc_macro2::Span;
use syn::{
    FnArg, GenericArgument, ItemFn, ItemImpl, ItemStruct, ItemTrait, PathArguments, ReturnType,
    TraitItem, Type, visit::Visit,
};

use super::{finding, production::test_only};
use crate::{
    Finding, Policy, Result,
    source::{SourceFile, Workspace},
};

pub(super) fn traits(workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
    let mut findings = check(workspace, policy, ApiRule::Traits)?;
    for evidence in super::adopted::contract::check(workspace, policy)? {
        if let Some(finding) = findings.iter_mut().find(|finding| {
            finding.subject == evidence.subject
                && finding.location.path == evidence.location.path
                && finding.location.line == evidence.location.line
        }) {
            finding.related = evidence.related;
            finding.review = evidence.review;
        }
    }
    Ok(findings)
}
pub(super) fn empty_structs(workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
    check(workspace, policy, ApiRule::EmptyStructs)
}
pub(super) fn struct_names(workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
    check(workspace, policy, ApiRule::StructNames)
}
pub(super) fn constructors(workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
    check(workspace, policy, ApiRule::Constructors)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApiRule {
    Traits,
    EmptyStructs,
    StructNames,
    Constructors,
}

fn check(workspace: &Workspace, _policy: &Policy, rule: ApiRule) -> Result<Vec<Finding>> {
    let mut output = Vec::new();
    for source in workspace.production() {
        ApiVisitor {
            source,
            output: &mut output,
            inherent: false,
            rule,
        }
        .visit_file(&source.syntax);
    }
    Ok(output)
}

struct ApiVisitor<'a> {
    source: &'a SourceFile,
    output: &'a mut Vec<Finding>,
    inherent: bool,
    rule: ApiRule,
}
impl ApiVisitor<'_> {
    fn push(
        &mut self,
        rule: &'static str,
        subject: String,
        span: Span,
        message: String,
        help: &'static str,
    ) {
        let location = self.source.location(span);
        if !self.source.suppressed(rule, location.line) {
            self.output
                .push(finding(rule, subject, location, message, help));
        }
    }
}

impl<'ast> Visit<'ast> for ApiVisitor<'_> {
    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        if test_only(&item.attrs) {
            return;
        }
        let count = item
            .items
            .iter()
            .filter(|item| matches!(item, TraitItem::Fn(method) if !test_only(&method.attrs)))
            .count();
        if self.rule == ApiRule::Traits && count > 3 {
            self.push(
                "trait-method-count",
                item.ident.to_string(),
                item.ident.span(),
                format!(
                    "trait `{}` declares {count} functions; maximum is 3",
                    item.ident
                ),
                "keep one to three cohesive, reusable operations in each trait",
            );
        }
        syn::visit::visit_item_trait(self, item);
    }
    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        if test_only(&item.attrs) {
            return;
        }
        if self.rule == ApiRule::EmptyStructs && item.fields.is_empty() {
            self.push(
                "empty-struct",
                item.ident.to_string(),
                item.ident.span(),
                format!("struct `{}` carries no state", item.ident),
                "use a module, function, trait, enum, or meaningful state",
            );
        }
        let count = name_words(&item.ident.to_string()).len();
        if self.rule == ApiRule::StructNames && count > 2 {
            self.push(
                "struct-word-count",
                item.ident.to_string(),
                item.ident.span(),
                format!(
                    "struct `{}` has {count} semantic words; maximum is 2",
                    item.ident
                ),
                "remove package/module context and use a short noun",
            );
        }
        syn::visit::visit_item_struct(self, item);
    }
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        if test_only(&item.attrs) {
            return;
        }
        let previous = self.inherent;
        self.inherent = item.trait_.is_none();
        syn::visit::visit_item_impl(self, item);
        self.inherent = previous;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if test_only(&item.attrs) {
            return;
        }
        let constructor = ["new", "parse", "from", "try_from"].iter().any(|prefix| {
            item.sig.ident == *prefix
                || item
                    .sig
                    .ident
                    .to_string()
                    .starts_with(&format!("{prefix}_"))
        });
        let receiver = item
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)));
        if self.rule == ApiRule::Constructors
            && self.inherent
            && constructor
            && receiver
            && returns_self(&item.sig.output)
        {
            self.push("self-constructor-static", item.sig.ident.to_string(), item.sig.ident.span(), format!("constructor `{}` returns Self but takes a receiver", item.sig.ident), "make it an associated function without self; suppress only when mutation/conversion semantics require a receiver");
        }
        syn::visit::visit_impl_item_fn(self, item);
    }
    fn visit_item_fn(&mut self, _item: &'ast ItemFn) {}
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }
}

fn returns_self(output: &ReturnType) -> bool {
    let ReturnType::Type(_, value) = output else {
        return false;
    };
    match value.as_ref() {
        Type::Path(path) if path.path.is_ident("Self") => true,
        Type::Path(path) => path.path.segments.last().is_some_and(|segment| matches!(&segment.arguments, PathArguments::AngleBracketed(arguments) if arguments.args.iter().any(|argument| matches!(argument, GenericArgument::Type(Type::Path(path)) if path.path.is_ident("Self"))))),
        _ => false,
    }
}

pub(super) fn name_words(name: &str) -> Vec<String> {
    let digit_start = name
        .trim_end_matches(|value: char| value.is_ascii_digit())
        .len();
    let name = if digit_start < name.len()
        && name
            .as_bytes()
            .get(digit_start.saturating_sub(1))
            .is_some_and(|value| *value == b'V' || *value == b'v')
    {
        &name[..digit_start - 1]
    } else {
        name
    };
    let chars = name.chars().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut start = 0;
    for index in 1..chars.len() {
        let boundary = (chars[index].is_ascii_uppercase()
            && (chars[index - 1].is_ascii_lowercase()
                || chars
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_lowercase())))
            || (!chars[index - 1].is_ascii_digit() && chars[index].is_ascii_digit());
        if boundary {
            output.push(
                chars[start..index]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
            );
            start = index;
        }
    }
    output.push(
        chars[start..]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase(),
    );
    output.into_iter().filter(|word| !word.is_empty()).collect()
}

#[cfg(test)]
pub(super) fn check_kind(
    workspace: &Workspace,
    policy: &Policy,
    name: &str,
) -> Result<Vec<Finding>> {
    match name {
        "traits" => traits(workspace, policy),
        "empty" => empty_structs(workspace, policy),
        "names" => struct_names(workspace, policy),
        "constructors" => constructors(workspace, policy),
        _ => unreachable!(),
    }
}
