use linter::{Evidence, Finding, Span};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
/// One source position recorded during the scan.
#[derive(Clone, Debug)]
pub(super) struct Site {
    pub(super) path: PathBuf,
    pub(super) span: Span,
    /// Function containing the site, absent at file scope.
    pub(super) function: Option<String>,
    /// The site itself sits inside test-only conditional compilation.
    pub(super) test_only: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Context<'a> {
    pub(super) test_only: bool,
    pub(super) predicate: bool,
    pub(super) function: Option<&'a str>,
}

#[derive(Default)]
pub(super) struct Corpus {
    /// Functions defined only inside test-only conditional compilation.
    pub(super) test_only_definitions: BTreeSet<String>,
    /// Functions with at least one definition a production build compiles.
    pub(super) production_definitions: BTreeSet<String>,
    /// Every definition site of a function name, used to skip undefined externals.
    pub(super) defined: BTreeSet<String>,
    /// Call sites keyed by callee.
    pub(super) calls: BTreeMap<String, Vec<Site>>,
    /// Assignments to file-scope state, keyed by the assigned name.
    pub(super) writes: BTreeMap<String, Vec<Site>>,
    /// Predicate reads of file-scope state outside test-only compilation.
    pub(super) predicate_reads: BTreeMap<String, Vec<Site>>,
    /// Names declared at file scope, the only names this rule tracks.
    pub(super) state: BTreeMap<String, Site>,
}

impl Corpus {
    /// Returns the functions no production call site can reach.
    ///
    /// A function is test-only when every one of its call sites is test-only, and a
    /// call site is test-only when the conditional compilation around it is, or when
    /// the calling function is itself unreachable from production. Functions with no
    /// call site in the corpus are entry points called across the FFI boundary and
    /// are treated as production.
    fn test_only_functions(&self) -> BTreeSet<String> {
        let mut unreachable = self
            .test_only_definitions
            .difference(&self.production_definitions)
            .cloned()
            .collect::<BTreeSet<_>>();
        loop {
            let mut grown = false;
            for name in &self.defined {
                if unreachable.contains(name) {
                    continue;
                }
                let Some(sites) = self.calls.get(name) else {
                    continue;
                };
                if sites
                    .iter()
                    .all(|site| Self::site_is_test_only(site, &unreachable))
                {
                    unreachable.insert(name.clone());
                    grown = true;
                }
            }
            if !grown {
                return unreachable;
            }
        }
    }

    fn site_is_test_only(site: &Site, unreachable: &BTreeSet<String>) -> bool {
        site.test_only
            || site
                .function
                .as_ref()
                .is_some_and(|function| unreachable.contains(function))
    }

    pub(super) fn findings(&self, setting: &str) -> Vec<Finding> {
        let unreachable = self.test_only_functions();
        let mut findings = Vec::new();
        for (name, reads) in &self.predicate_reads {
            if !self.state.contains_key(name) {
                continue;
            }
            let Some(writes) = self.writes.get(name) else {
                continue;
            };
            if !writes
                .iter()
                .all(|site| Self::site_is_test_only(site, &unreachable))
            {
                continue;
            }
            // A read is recorded from its conditional compilation alone, while a write is judged
            // by reachability. Without this the two disagree: a function no production call site
            // reaches reports a production read of state only it writes, and there is no
            // production branch left to be wrong about.
            for read in reads
                .iter()
                .filter(|read| !Self::site_is_test_only(read, &unreachable))
            {
                findings.push(finding(name, read, writes, self.state.get(name), setting));
            }
        }
        findings
    }
}

fn finding(
    name: &str,
    read: &Site,
    writes: &[Site],
    declaration: Option<&Site>,
    setting: &str,
) -> Finding {
    let label = name.rsplit("::").next().unwrap_or(name);
    let mut related = declaration
        .map(|site| Evidence {
            path: site.path.clone(),
            span: Some(site.span.clone()),
            message: format!("File-scope state '{label}' declared here."),
        })
        .into_iter()
        .collect::<Vec<_>>();
    related.extend(writes.iter().map(|write| Evidence {
        path: write.path.clone(),
        span: Some(write.span.clone()),
        message: format!(
            "Test-only write in '{}'.",
            write.function.as_deref().unwrap_or("file scope")
        ),
    }));
    Finding {
        rule: "c/test-only-state",
        path: read.path.clone(),
        span: Some(read.span.clone()),
        related,
        configuration: setting.into(),
        message: format!("production predicate reads '{label}', which only test-only code writes"),
        instruction: format!(
            "Give '{label}' a production writer or guard the predicate with the same test-only condition as its writers."
        ),
    }
}
