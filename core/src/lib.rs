//! The engine-free core of horizon-packetevents: the packet API (`pe`), unchanged from the
//! horizon-rust port's `api/src/pe/`, compiled as a crate of its own so its tests run without
//! the engine (`cd core && cargo test`).
pub mod pe;
