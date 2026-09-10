use linter::Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
struct Command {
    directory: PathBuf,
    file: PathBuf,
    #[serde(flatten)]
    metadata: BTreeMap<String, serde_json::Value>,
}

pub(crate) struct Database {
    directory: tempfile::TempDir,
    pub files: Vec<PathBuf>,
}

impl Database {
    pub fn select(path: &Path, selected: &BTreeSet<PathBuf>) -> Result<Self, Error> {
        let bytes = fs::read(path).map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
        let commands: Vec<Command> = serde_json::from_slice(&bytes)
            .map_err(|error| Error::Analysis(format!("{}: {error}", path.display())))?;
        let mut files = BTreeSet::new();
        let mut filtered = Vec::new();
        for command in commands {
            if let Some(command) = command.selected(path, selected)? {
                files.insert(command.file.clone());
                filtered.push(command);
            }
        }
        if files.is_empty() && !selected.is_empty() {
            return Err(Error::Analysis(format!(
                "{}: no selected C translation units",
                path.display()
            )));
        }
        let directory = tempfile::tempdir().map_err(|error| Error::Analysis(error.to_string()))?;
        let output = directory.path().join("compile_commands.json");
        let bytes =
            serde_json::to_vec(&filtered).map_err(|error| Error::Analysis(error.to_string()))?;
        fs::write(&output, bytes).map_err(|source| Error::Io {
            path: output,
            source,
        })?;
        Ok(Self {
            directory,
            files: files.into_iter().collect(),
        })
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}

impl Command {
    fn selected(
        mut self,
        database: &Path,
        selected: &BTreeSet<PathBuf>,
    ) -> Result<Option<Self>, Error> {
        if !self.directory.is_absolute() {
            self.directory = database
                .parent()
                .unwrap_or(Path::new("."))
                .join(&self.directory);
        }
        let Ok(path) = self.directory.join(&self.file).canonicalize() else {
            return Ok(None);
        };
        if !selected.contains(&path) {
            return Ok(None);
        }
        let has_arguments = self.metadata.get("arguments").is_some_and(|value| {
            value.as_array().is_some_and(|values| {
                !values.is_empty() && values.iter().all(|value| value.is_string())
            })
        });
        let has_command = self
            .metadata
            .get("command")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.trim().is_empty());
        if !has_arguments && !has_command {
            return Err(Error::Analysis(format!(
                "{}: compilation entry needs arguments or command",
                path.display()
            )));
        }
        self.file = path;
        Ok(Some(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_database_and_preserves_compile_metadata() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("one.c");
        fs::write(&source, "int x;").unwrap();
        fs::write(root.path().join("two.c"), "int y;").unwrap();
        let path = root.path().join("compile_commands.json");
        fs::write(
            &path,
            r#"[
{"directory":".","file":"one.c","arguments":["cc","-c","one.c"],"output":"one.o"},
{"directory":".","file":"two.c","command":"cc -c two.c"},
{"directory":".","file":"missing.c","command":"cc -c missing.c"}]
"#,
        )
        .unwrap();
        let database =
            Database::select(&path, &BTreeSet::from([source.canonicalize().unwrap()])).unwrap();
        assert_eq!(database.files.len(), 1);
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(database.path().join("compile_commands.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(value[0]["output"], "one.o");
        assert_eq!(value[0]["arguments"][0], "cc");
        fs::write(&path, "[]").unwrap();
        assert!(Database::select(&path, &BTreeSet::from([source])).is_err());
    }
}
