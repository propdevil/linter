use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Finding, LintError, Result, Summary, report::Reporter};

const MARKER: &str = "<!-- design-lint: generated case -->\n";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Stages Markdown cases and replaces only previously marked generated files.
pub struct Cases<Output = io::Stderr> {
    root: PathBuf,
    source_root: Option<PathBuf>,
    display_roots: Vec<PathBuf>,
    pending: BTreeMap<String, Vec<u8>>,
    output: Output,
}

impl Cases<io::Stderr> {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self::with_output(root, io::stderr())
    }
}

impl<Output> Cases<Output> {
    pub fn with_output(root: PathBuf, output: Output) -> Self {
        Self {
            root,
            source_root: None,
            display_roots: Vec::new(),
            pending: BTreeMap::new(),
            output,
        }
    }

    pub fn into_inner(self) -> Output {
        self.output
    }

    fn name(&self, finding: &Finding) -> String {
        let path = self
            .source_root
            .as_ref()
            .and_then(|root| finding.location.path.strip_prefix(root).ok())
            .unwrap_or(&finding.location.path);
        let identity = format!(
            "{}\0{}\0{}\0{}\0{}",
            finding.rule,
            finding.subject,
            path.to_string_lossy().replace('\\', "/"),
            finding.location.line,
            finding.location.column
        );
        // Fixed FNV-1a gives stable names across executions and finding order.
        let hash = identity
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
            });
        format!(
            "{}_{}_{hash:016x}.md",
            safe(finding.rule),
            safe(&finding.subject)
        )
    }
}

impl<Output: Write> Reporter for Cases<Output> {
    fn begin(&mut self, workspace: &crate::source::Workspace) -> Result<()> {
        self.pending.clear();
        self.display_roots.clone_from(&workspace.policy_roots);
        self.source_root = workspace.policy_roots.first().cloned();
        if let Some(root) = self.source_root.as_mut() {
            while !workspace
                .policy_roots
                .iter()
                .all(|path| path.starts_with(&*root))
            {
                if !root.pop() {
                    break;
                }
            }
        }
        Ok(())
    }

    fn finding(&mut self, finding: &Finding) -> Result<()> {
        let mut text = MARKER.as_bytes().to_vec();
        super::markdown::render(&mut text, finding, &self.display_roots)
            .map_err(|error| LintError::report("case rendering", error))?;
        let name = self.name(finding);
        if let Some(previous) = self.pending.get(&name) {
            if previous != &text {
                return Err(LintError::configuration(format!(
                    "case identity collision for {name}"
                )));
            }
        } else {
            self.pending.insert(name, text);
        }
        Ok(())
    }

    fn finish(&mut self, _summaries: &[Summary]) -> Result<()> {
        directory(&self.root)?;
        for queue in ["errors", "check"] {
            directory(&self.root.join(queue))?;
        }
        let previous = owned_files(&self.root)?;
        for name in self.pending.keys() {
            let target = self.root.join("errors").join(name);
            match fs::symlink_metadata(&target) {
                Ok(_) if !owned(&target)? => {
                    return Err(LintError::configuration(format!(
                        "refusing to replace unowned case path {}",
                        target.display()
                    )));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(LintError::io("inspect case", &target, error)),
            }
        }
        let mut stage = Staging::new(&self.root)?;
        for (name, text) in &self.pending {
            let path = stage.path.join("new").join(name);
            fs::write(&path, text).map_err(|error| LintError::io("stage case", &path, error))?;
        }
        // Fail an unavailable output before replacing any persistent reports.
        writeln!(
            self.output,
            "prepared {} case(s) for {}",
            self.pending.len(),
            self.root.display()
        )
        .and_then(|()| self.output.flush())
        .map_err(|error| LintError::report("case summary", error))?;
        stage.publish(&self.root, &previous, self.pending.keys())
    }
}

struct Staging {
    path: PathBuf,
    retain: bool,
}

impl Staging {
    fn new(root: &Path) -> Result<Self> {
        let path = loop {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                ".design-lint-stage-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(LintError::io("create case staging", &path, error)),
            }
        };
        let stage = Self {
            path,
            retain: false,
        };
        for directory in ["new", "backup/errors", "backup/check"] {
            let path = stage.path.join(directory);
            fs::create_dir_all(&path)
                .map_err(|error| LintError::io("create staging directory", &path, error))?;
        }
        Ok(stage)
    }

    fn publish<'a>(
        &mut self,
        root: &Path,
        previous: &[PathBuf],
        names: impl Iterator<Item = &'a String>,
    ) -> Result<()> {
        let mut moved = Vec::new();
        let mut installed = Vec::new();
        let result = (|| {
            for relative in previous {
                let original = root.join(relative);
                let backup = self.path.join("backup").join(relative);
                fs::rename(&original, &backup)
                    .map_err(|error| LintError::io("back up case", &original, error))?;
                moved.push(relative.clone());
            }
            for name in names {
                let target = root.join("errors").join(name);
                fs::rename(self.path.join("new").join(name), &target)
                    .map_err(|error| LintError::io("publish case", &target, error))?;
                installed.push(target);
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(rollback) = self.restore(root, &moved, &installed) {
                self.retain = true;
                return Err(LintError::configuration(format!(
                    "{error}; restoring previous cases failed: {rollback}; backups remain at {}",
                    self.path.display()
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    fn restore(&self, root: &Path, moved: &[PathBuf], installed: &[PathBuf]) -> io::Result<()> {
        for path in installed.iter().rev() {
            fs::remove_file(path)?;
        }
        for relative in moved.iter().rev() {
            fs::rename(self.path.join("backup").join(relative), root.join(relative))?;
        }
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .map_err(|error| LintError::io("create case directory", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| LintError::io("inspect case directory", path, error))?;
    if !metadata.file_type().is_dir() {
        return Err(LintError::configuration(format!(
            "case directory must be an ordinary directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn owned_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for queue in ["errors", "check"] {
        let path = root.join(queue);
        for entry in
            fs::read_dir(&path).map_err(|error| LintError::io("read cases", &path, error))?
        {
            let entry = entry.map_err(|error| LintError::io("read case entry", &path, error))?;
            if owned(&entry.path())? {
                paths.push(Path::new(queue).join(entry.file_name()));
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn owned(path: &Path) -> Result<bool> {
    if path.file_name().is_some_and(|name| name == ".gitkeep") {
        return Ok(false);
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| LintError::io("inspect case", path, error))?;
    if !metadata.is_file() {
        return Ok(false);
    }
    let mut prefix = Vec::new();
    fs::File::open(path)
        .map_err(|error| LintError::io("read case owner", path, error))?
        .take(MARKER.len() as u64)
        .read_to_end(&mut prefix)
        .map_err(|error| LintError::io("read case owner", path, error))?;
    Ok(prefix == MARKER.as_bytes())
}

fn safe(value: &str) -> String {
    value
        .chars()
        .take(60)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

#[cfg(test)]
mod tests;
