mod analysis;
mod rule;
pub use analysis::{Analysis, Source};
pub use rule::examples::{Config as ExamplesConfig, Examples};

pub fn register(registry: linter::Registry) -> Result<linter::Registry, linter::Error> {
    registry.register::<Examples>()
}
