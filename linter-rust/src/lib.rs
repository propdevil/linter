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
        .register::<StructNoun>()?
        .register::<DuplicateEntity>()?
        .register::<ModelDuplication>()?
        .register::<BooleanState>()?
        .register::<StringState>()
}

mod declaration;
pub use rule::boolean::{BooleanState, Config as BooleanStateConfig};
pub use rule::duplicate::{Config as DuplicateEntityConfig, DuplicateEntity};

pub use rule::model::{Config as ModelDuplicationConfig, ModelDuplication};

pub use rule::state::{Config as StringStateConfig, StringState};
pub use rule::trait_count::{Config as TraitMethodCountConfig, TraitMethodCount};

pub use rule::struct_words::{Config as StructWordsConfig, StructWords};
