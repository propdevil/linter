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
        .register::<Allocation>()?
        .register::<ResultUse>()?
        .register::<Safety>()?
        .register::<Interface>()?
        .register::<ForbiddenCall>()?
        .register::<TestOnlyState>()?
        .register::<Format>()?
        .register::<Tidy>()?
        .register::<Cppcheck>()
}

pub use rule::allocation::Allocation;

pub use rule::result::ResultUse;

pub use rule::safety::Safety;

pub use rule::interface::Interface;

pub use rule::calls::ForbiddenCall;

pub use rule::hook::TestOnlyState;

mod process;
pub use rule::format::Format;

mod compilation;
pub use rule::tidy::Tidy;

pub use rule::cppcheck::Cppcheck;
