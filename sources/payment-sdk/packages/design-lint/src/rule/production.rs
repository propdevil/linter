use std::ops::Range;

use proc_macro2::LineColumn;
use syn::{
    Attribute, ForeignItem, ImplItem, Item, Meta, TraitItem, spanned::Spanned, visit::Visit,
};

use crate::source::SourceFile;

pub(super) fn text(source: &SourceFile) -> String {
    let mut visitor = RangeVisitor::default();
    visitor.visit_file(&source.syntax);
    let offsets = LineOffsets::new(&source.text);
    let mut bytes = source.text.as_bytes().to_vec();
    for range in visitor
        .ranges
        .into_iter()
        .filter_map(|range| offsets.byte_range(range))
    {
        for byte in &mut bytes[range] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    // Replacing whole AST spans cannot split a UTF-8 character.
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(super) fn line_count(source: &SourceFile) -> usize {
    let mut visitor = RangeVisitor::default();
    visitor.visit_file(&source.syntax);
    let offsets = LineOffsets::new(&source.text);
    let mut test_ranges = visitor
        .ranges
        .into_iter()
        .filter_map(|range| offsets.byte_range(range))
        .collect::<Vec<_>>();
    merge(&mut test_ranges);
    physical_lines(&source.text)
        .into_iter()
        .filter(|line| !test_only_line(&source.text, line, &test_ranges))
        .count()
}

pub(crate) fn test_only(attributes: &[Attribute]) -> bool {
    if attributes
        .iter()
        .any(|attribute| attribute.path().is_ident("test"))
    {
        return true;
    }
    let predicates = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .map(cfg_predicate)
        .collect::<Vec<_>>();
    if predicates.is_empty() {
        return false;
    }
    let production = predicates
        .iter()
        .fold(Truth::True, |value, predicate| value.and(predicate(false)));
    let testing = predicates
        .iter()
        .fold(Truth::True, |value, predicate| value.and(predicate(true)));
    production == Truth::False && testing != Truth::False
}

fn cfg_predicate(attribute: &Attribute) -> impl Fn(bool) -> Truth + '_ {
    move |testing| {
        let Meta::List(cfg) = &attribute.meta else {
            return Truth::Unknown;
        };
        cfg.parse_args::<Meta>()
            .map_or(Truth::Unknown, |predicate| evaluate(&predicate, testing))
    }
}

fn evaluate(predicate: &Meta, testing: bool) -> Truth {
    match predicate {
        Meta::Path(path) if path.is_ident("test") => Truth::from(testing),
        Meta::Path(_) | Meta::NameValue(_) => Truth::Unknown,
        Meta::List(list) => {
            let Ok(values) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return Truth::Unknown;
            };
            if list.path.is_ident("all") {
                values.iter().fold(Truth::True, |value, item| {
                    value.and(evaluate(item, testing))
                })
            } else if list.path.is_ident("any") {
                values.iter().fold(Truth::False, |value, item| {
                    value.or(evaluate(item, testing))
                })
            } else if list.path.is_ident("not") && values.len() == 1 {
                evaluate(&values[0], testing).not()
            } else {
                Truth::Unknown
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }

    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::False, Self::False) => Self::False,
            _ => Self::Unknown,
        }
    }

    fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}

impl From<bool> for Truth {
    fn from(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }
}

#[derive(Clone, Copy)]
struct SpanRange {
    start: LineColumn,
    end: LineColumn,
}

#[derive(Default)]
struct RangeVisitor {
    ranges: Vec<SpanRange>,
}

impl RangeVisitor {
    fn record<T: Spanned>(&mut self, attributes: &[Attribute], item: &T) {
        let span = item.span();
        let start = attributes
            .iter()
            .map(|attribute| attribute.span().start())
            .chain(std::iter::once(span.start()))
            .min_by_key(|location| (location.line, location.column))
            .unwrap_or_else(|| span.start());
        self.ranges.push(SpanRange {
            start,
            end: span.end(),
        });
    }
}

impl<'ast> Visit<'ast> for RangeVisitor {
    fn visit_item(&mut self, item: &'ast Item) {
        if let Some(attributes) = item_attributes(item)
            && test_only(attributes)
        {
            self.record(attributes, item);
            return;
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        if let Some(attributes) = impl_item_attributes(item)
            && test_only(attributes)
        {
            self.record(attributes, item);
            return;
        }
        syn::visit::visit_impl_item(self, item);
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        if let Some(attributes) = trait_item_attributes(item)
            && test_only(attributes)
        {
            self.record(attributes, item);
            return;
        }
        syn::visit::visit_trait_item(self, item);
    }

    fn visit_foreign_item(&mut self, item: &'ast ForeignItem) {
        if let Some(attributes) = foreign_item_attributes(item)
            && test_only(attributes)
        {
            self.record(attributes, item);
            return;
        }
        syn::visit::visit_foreign_item(self, item);
    }
}

fn item_attributes(item: &Item) -> Option<&[Attribute]> {
    match item {
        Item::Const(item) => Some(&item.attrs),
        Item::Enum(item) => Some(&item.attrs),
        Item::ExternCrate(item) => Some(&item.attrs),
        Item::Fn(item) => Some(&item.attrs),
        Item::ForeignMod(item) => Some(&item.attrs),
        Item::Impl(item) => Some(&item.attrs),
        Item::Macro(item) => Some(&item.attrs),
        Item::Mod(item) => Some(&item.attrs),
        Item::Static(item) => Some(&item.attrs),
        Item::Struct(item) => Some(&item.attrs),
        Item::Trait(item) => Some(&item.attrs),
        Item::TraitAlias(item) => Some(&item.attrs),
        Item::Type(item) => Some(&item.attrs),
        Item::Union(item) => Some(&item.attrs),
        Item::Use(item) => Some(&item.attrs),
        Item::Verbatim(_) => None,
        _ => None,
    }
}

fn impl_item_attributes(item: &ImplItem) -> Option<&[Attribute]> {
    match item {
        ImplItem::Const(item) => Some(&item.attrs),
        ImplItem::Fn(item) => Some(&item.attrs),
        ImplItem::Type(item) => Some(&item.attrs),
        ImplItem::Macro(item) => Some(&item.attrs),
        ImplItem::Verbatim(_) => None,
        _ => None,
    }
}

fn trait_item_attributes(item: &TraitItem) -> Option<&[Attribute]> {
    match item {
        TraitItem::Const(item) => Some(&item.attrs),
        TraitItem::Fn(item) => Some(&item.attrs),
        TraitItem::Type(item) => Some(&item.attrs),
        TraitItem::Macro(item) => Some(&item.attrs),
        TraitItem::Verbatim(_) => None,
        _ => None,
    }
}

fn foreign_item_attributes(item: &ForeignItem) -> Option<&[Attribute]> {
    match item {
        ForeignItem::Fn(item) => Some(&item.attrs),
        ForeignItem::Static(item) => Some(&item.attrs),
        ForeignItem::Type(item) => Some(&item.attrs),
        ForeignItem::Macro(item) => Some(&item.attrs),
        ForeignItem::Verbatim(_) => None,
        _ => None,
    }
}

struct LineOffsets<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineOffsets<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.match_indices('\n')
                .map(|(index, _)| index + 1)
                .filter(|start| *start < text.len()),
        );
        Self { starts, text }
    }

    fn byte_range(&self, range: SpanRange) -> Option<Range<usize>> {
        let start = self.byte_offset(range.start)?;
        let end = self.byte_offset(range.end)?;
        (start <= end).then_some(start..end)
    }

    fn byte_offset(&self, location: LineColumn) -> Option<usize> {
        let start = *self.starts.get(location.line.checked_sub(1)?)?;
        let line = self.text[start..].split('\n').next()?;
        line.char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(line.len()))
            .nth(location.column)
            .map(|offset| start + offset)
    }
}

fn merge(ranges: &mut Vec<Range<usize>>) {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    *ranges = merged;
}

fn physical_lines(text: &str) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, _) in text.match_indices('\n') {
        lines.push(start..index + 1);
        start = index + 1;
    }
    if start < text.len() {
        lines.push(start..text.len());
    }
    lines
}

fn test_only_line(text: &str, line: &Range<usize>, ranges: &[Range<usize>]) -> bool {
    let touches_test = ranges
        .iter()
        .any(|range| range.start < line.end && line.start < range.end);
    touches_test
        && text[line.clone()]
            .char_indices()
            .all(|(offset, character)| {
                character.is_whitespace()
                    || ranges.iter().any(|range| {
                        let start = line.start + offset;
                        range.start <= start && start + character.len_utf8() <= range.end
                    })
            })
}
