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
        .register::<StringState>()?
        .register::<TraitMethodCount>()?
        .register::<StructWords>()?
        .register::<EmptyStruct>()?
        .register::<ModuleName>()?
        .register::<PathModules>()?
        .register::<ReceiverName>()?
        .register::<BroadTrait>()?
        .register::<SelfConstructor>()?
        .register::<ModulePrefix>()?
        .register::<GodObject>()?
        .register::<DetachedConstructor>()?
        .register::<RedundantAccessor>()?
        .register::<UnsafeBoundary>()?
        .register::<FreeFunction>()
}

mod declaration;
pub use rule::boolean::{BooleanState, Config as BooleanStateConfig};
pub use rule::contract::{BroadTrait, Config as BroadTraitConfig};
pub use rule::duplicate::{Config as DuplicateEntityConfig, DuplicateEntity};

pub use rule::model::{Config as ModelDuplicationConfig, ModelDuplication};

pub use rule::state::{Config as StringStateConfig, StringState};
pub use rule::trait_count::{Config as TraitMethodCountConfig, TraitMethodCount};

pub use rule::empty::{Config as EmptyStructConfig, EmptyStruct};
pub use rule::struct_words::{Config as StructWordsConfig, StructWords};

pub use rule::module_name::{Config as ModuleNameConfig, ModuleName};

pub use rule::receiver::{Config as ReceiverNameConfig, ReceiverName};

pub use rule::path_modules::{Config as PathModulesConfig, PathModules};

pub use rule::constructor::{Config as SelfConstructorConfig, SelfConstructor};
pub use rule::module_prefix::{Config as ModulePrefixConfig, ModulePrefix};
mod type_path;
pub use rule::function::{Config as FreeFunctionConfig, FreeFunction};
pub use rule::object::{Config as GodObjectConfig, GodObject};

pub use rule::detached::{Config as DetachedConstructorConfig, DetachedConstructor};

pub use rule::accessor::{Config as RedundantAccessorConfig, RedundantAccessor};
pub use rule::safety::{Config as UnsafeBoundaryConfig, UnsafeBoundary};

pub use rule::namespace::{Config as RedundantNamespaceConfig, RedundantNamespace};
