//! The packet API: packetevents' model (packet types, events, listeners with priorities, typed
//! wrappers) for Minecraft 26.3, engine-free. `api/src/lib.rs` connects it to Horizon's hooks;
//! `core/` builds this directory as a crate of its own, so `cargo test` there needs no engine.

pub mod codec;
pub mod event;
pub mod manager;
pub mod nbt;
pub mod packet_type;
pub mod wrapper;

#[cfg(test)]
mod tests;

pub use codec::{BasicRegistry, Error, Reader, Registry, Result, Writer};
pub use event::{PacketEvent, PacketListener, Priority, User};
pub use manager::{ListenerId, Outcome, PacketEvents};
pub use nbt::Tag;
pub use packet_type::{ConnectionState, PacketType};
pub use wrapper::{RawItem, RawParticle, Wrapper};
