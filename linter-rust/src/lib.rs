mod analysis;
mod rule;

pub use analysis::{Analysis, Source};
pub use rule::function_length::{Config as FunctionLengthConfig, FunctionLength};
pub use rule::layers::{Config as LayersConfig, Layers};
pub use rule::length::{Config as LengthConfig, FileLength};
mod scope;
pub use rule::method_length::{Config as MethodLengthConfig, MethodLength};

mod directive;
pub use rule::nesting::{Config as NestingConfig, NestingRule};
pub use rule::struct_noun::{Config as StructNounConfig, StructNoun};

/// Registers this package's built-in rules; callers can append their own.
pub fn register(registry: linter::Registry) -> Result<linter::Registry, linter::Error> {
    registry
        .register::<Layers>()?
        .register::<FileLength>()?
        .register::<FunctionLength>()?
        .register::<MethodLength>()?
        .register::<NestingRule>()?
        .register::<StructNoun>()
}

mod declaration;
pub use rule::duplicate::{Config as DuplicateEntityConfig, DuplicateEntity};
