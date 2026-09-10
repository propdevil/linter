use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Policy, source::Workspace};

pub struct Fixture {
    root: PathBuf,
}

impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "design-lint-adoption-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        if !files.iter().any(|(path, _)| *path == "Cargo.toml") {
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname='fixture'\nversion='0.0.0'\n",
            )
            .unwrap();
        }
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn workspace(&self) -> Workspace {
        Workspace::load(vec![self.root.clone()], &Policy::default().source).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
