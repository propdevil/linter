mod accessor;
mod blocking;
mod boolean;
mod catchall;
mod ceremony;
mod command;
pub(super) mod contract;
mod duplicate;
mod environment;
mod function;
mod model;
mod naming;
mod nesting;
mod object;
mod receiver;
mod result;
mod single;
mod state;

pub use accessor::AccessorBloat;
pub use blocking::AsyncBlocking;
pub use boolean::BooleanState;
pub use catchall::CatchAllModule;
pub use ceremony::CeremonialStructure;
pub use command::PlatformCommand;
pub use duplicate::DuplicateEntity;
pub use environment::EnvironmentAccess;
pub use function::FreeFunction;
pub use model::ModelDuplication;
pub use naming::StructNaming;
pub use nesting::DeepControlFlow;
pub use object::GodObject;
pub use receiver::ReceiverRepetition;
pub use result::IgnoredResult;
pub use single::SingleUse;
pub use state::FiniteStateString;

pub(crate) const IDS: [&str; 17] = [
    receiver::ID,
    catchall::ID,
    naming::ID,
    function::ID,
    single::ID,
    nesting::ID,
    environment::ID,
    command::ID,
    result::ID,
    blocking::ID,
    boolean::ID,
    state::ID,
    object::ID,
    accessor::ID,
    duplicate::ID,
    model::ID,
    ceremony::ID,
];

pub(crate) fn contains(id: &str) -> bool {
    IDS.contains(&id)
}
