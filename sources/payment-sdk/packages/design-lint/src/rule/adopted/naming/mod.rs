use english_pos_tagger::Tagger;
use proc_macro2::Span;
use syn::{Attribute, ItemMod, ItemStruct, spanned::Spanned, visit::Visit};
use wordnet_lemmatizer::{Lemmatizer, Pos};

use crate::{
    Policy, Result,
    model::{Finding, Review},
    source::{SourceFile, Workspace, snake_case},
};

pub(crate) const ID: &str = "struct-noun-naming";

/// Checks that struct names contain a meaningful noun.
pub struct StructNaming;

impl crate::Rule for StructNaming {
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
    let mut findings = Vec::new();
    let language = English::new();
    for source in workspace.production() {
        let mut names = Names {
            rule: ID,
            source,
            test_scope: false,
            findings: &mut findings,
            language: &language,
        };
        names.visit_file(&source.syntax);
    }
    Ok(findings)
}

struct Names<'a> {
    rule: &'static str,
    source: &'a SourceFile,
    test_scope: bool,
    findings: &'a mut Vec<Finding>,
    language: &'a dyn Language,
}

impl Names<'_> {
    fn push(&mut self, name: &syn::Ident, span: Span, kind: &str, reason: &str) {
        let mut finding = Finding::error(self.rule, name.to_string(), self.source.location(span));
        finding.message = format!("`{name}` violates the noun/struct rule: {reason}");
        finding.help = "choose a precise noun for the struct, or document a reviewed exception with an exact reasoned design-lint allow comment".into();
        let mut review = Review::default();
        review.metadata.push(("Kind".into(), kind.into()));
        review
            .questions
            .push("What domain noun states this value's role?".into());
        finding.review = Some(review);
        self.findings.push(finding);
    }

    fn type_name(&mut self, name: &syn::Ident, attributes: &[Attribute], span: Span) {
        if !self.test_scope
            && !crate::rule::production::test_only(attributes)
            && !has_noun(self.language, &name.to_string())
        {
            self.push(
                name,
                span,
                "struct",
                "type name is an adjective or past participle; name the value it represents",
            );
        }
    }
}

impl<'ast> Visit<'ast> for Names<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.test_scope;
        self.test_scope |= crate::rule::production::test_only(&item.attrs);
        syn::visit::visit_item_fn(self, item);
        self.test_scope = previous;
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self.test_scope;
        self.test_scope |= crate::rule::production::test_only(&item.attrs);
        syn::visit::visit_item_impl(self, item);
        self.test_scope = previous;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous = self.test_scope;
        self.test_scope |= crate::rule::production::test_only(&item.attrs);
        syn::visit::visit_impl_item_fn(self, item);
        self.test_scope = previous;
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let previous = self.test_scope;
        self.test_scope |= crate::rule::production::test_only(&item.attrs);
        syn::visit::visit_item_mod(self, item);
        self.test_scope = previous;
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        self.type_name(&item.ident, &item.attrs, item.span());
        syn::visit::visit_item_struct(self, item);
    }
}

trait Language {
    fn noun(&self, word: &str) -> bool;
}

struct English {
    tagger: Tagger,
    wordnet: Lemmatizer,
}

impl English {
    fn new() -> Self {
        Self {
            tagger: Tagger::new(),
            wordnet: Lemmatizer::embedded(),
        }
    }
}

impl Language for English {
    fn noun(&self, word: &str) -> bool {
        self.wordnet.morphy(word, Pos::Noun).is_some()
            || self
                .tagger
                .tag_raw_tokens(&[word])
                .first()
                .and_then(|token| token.pos.as_deref())
                .is_some_and(|part| part.starts_with("NN"))
    }
}

fn has_noun(language: &dyn Language, name: &str) -> bool {
    identifier_words(name)
        .iter()
        .any(|word| language.noun(word))
}

fn identifier_words(name: &str) -> Vec<String> {
    snake_case(name)
        .split('_')
        .map(|word| {
            word.chars()
                .filter(|character| character.is_ascii_alphabetic())
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
