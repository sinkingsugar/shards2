//! Shards 2.0 runtime core: prototype.
//!
//! The design is in `docs/shards-2-compose-split.md` and the shard contract in
//! `docs/prototype-shard-contract.md`. Compose output ([`Shard::Compiled`]) is
//! shared and immutable; each instance owns only its [`Shard::State`] and its
//! local frame. Compose is generic over a scheduler [`compose::Backend`]:
//! [`Mesh`] is the stackless scheduler (the default), [`StackfulMesh`] the
//! stackful one (desktop native targets only). Compose types name their
//! backend explicitly.

pub mod args;
pub mod bench;
pub mod catalog;
pub mod compose;
pub mod describe;
pub mod diagnostic;
pub mod error;
pub mod flow;
#[doc(hidden)]
pub mod inline;
pub mod instance;
mod lifecycle;
pub mod log;
pub mod reload;
pub mod runtime;
pub mod shard;
pub mod shards;
pub mod stackless;
pub mod types;
pub mod var;

pub use args::{Arg, Args};
pub use catalog::Catalog;
pub use compose::{CacheStats, CompiledWire, ComposeCache, ComposeCtx, WireDef};
pub use describe::ShardDesc;
pub use diagnostic::Diagnostic;
pub use error::{Error, Result};
pub use instance::{InstanceId, InstanceMemory, Outcome, WakeMode};
#[cfg(stackful)]
pub use runtime::Mesh as StackfulMesh;
pub use shard::{Composed, Flow, ParamValue, Shard, ShardDef, ShardType, Stackful, shard_type};
pub use stackless::Mesh;
pub use stackless::Stackless;
pub use types::{Type, TypeDesc};
pub use var::Var;
