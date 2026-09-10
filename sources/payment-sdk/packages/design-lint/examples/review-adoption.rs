//! Review every implemented rule, including rules not yet enabled in lint.toml.
use clap::Parser;
use design_lint::{Cases, Linter, Markdown, Policy, Registry, Reporter, Result};
use std::path::PathBuf;

#[derive(Parser)]
struct Arguments {
    #[arg(long, default_value = "lint.toml")]
    policy: PathBuf,
    #[arg(long)]
    cases: Option<PathBuf>,
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
}
fn main() -> Result<()> {
    let arguments = Arguments::parse();
    let policy = Policy::load(arguments.policy)?;
    let mut reporter: Box<dyn Reporter> = match arguments.cases {
        Some(root) => Box::new(Cases::new(root)),
        None => Box::new(Markdown::default()),
    };
    Linter::new(policy, Registry::all()?).run(arguments.paths, reporter.as_mut())?;
    Ok(())
}
