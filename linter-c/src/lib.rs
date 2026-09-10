mod analysis;
mod normalize;
mod recovery;
mod syntax;

pub use analysis::{Analysis, Source};

mod rule;
pub use rule::function_length::FunctionLength;

mod lines;
