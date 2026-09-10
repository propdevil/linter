mod analysis;
mod normalize;
mod recovery;
mod syntax;

pub use analysis::{Analysis, Source};

mod rule;
pub use rule::function_length::FunctionLength;

mod lines;

pub use rule::file_length::FileLength;

mod directive;
pub use rule::nesting::NestingRule;

/// Registers this package's built-in rules; callers can append their own.
pub fn register(registry: linter::Registry) -> Result<linter::Registry, linter::Error> {
    registry
        .register::<FileLength>()?
        .register::<FunctionLength>()?
        .register::<NestingRule>()?
        .register::<Allocation>()
}

pub use rule::allocation::Allocation;
