//! The engine-free core of horizon-packetevents: the packet API of `api/src/pe/`, compiled as a
//! crate of its own so its tests run without the Horizon engine (`cd core && cargo test`).
#[path = "../../api/src/pe/mod.rs"]
pub mod pe;
