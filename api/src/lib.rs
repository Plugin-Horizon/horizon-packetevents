//! horizon-packetevents: a packet API for native mods, as a trait mod (docs/native-mods.md,
//! "Trait mods"). The packet model (types, events, listeners with priorities, typed wrappers)
//! is in `pe`, engine-free and tested through `core/`. This file connects it to Horizon:
//!
//! - outgoing play packets: `Hooks::server_packet` hands each packet to the listeners as its
//!   body (Horizon's own `encode()` gives id and body; `decode(id, body)` gives the packet back);
//! - incoming plugin messages: `Hooks::custom_payload_from_client` (listen-only, see GAPS.md);
//! - sending a typed packet to one player: not possible yet (GAPS.md, `send_to_player`).
//!
//! Pinned to Horizon 1453e0e61 and built against it. Lines marked `UNVERIFIED`
//! guess an engine name or signature; they are the first to check when it compiles (STATUS.md).

// The packet model; `core/` builds the same files as a crate of its own.
#[allow(dead_code)]
pub mod pe;

use horizon_client::mod_api::{
    ClientboundPlayPacket, CustomPayloadFromClientEvent, Flow, Hooks, Host, ServerPacketEvent, TickEvent,
};
use pe::manager::{Outcome, PacketEvents};
use pe::packet_type::{ConnectionState, PacketType, PlayClient};
use pe::wrapper::Wrapper;
use pe::wrapper::serverbound::PluginMessage;
use pe::{Registry, User};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Server ticks between two refreshes of the player names.
const NAME_EVERY_TICKS: u64 = 20;

/// The mod's packet API, for other mods that depend on this crate: `packet_events().add_listener`.
pub fn packet_events() -> &'static PacketEvents {
    static API: OnceLock<PacketEvents> = OnceLock::new();
    API.get_or_init(|| PacketEvents::new(Box::new(HorizonRegistry)))
}

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(PacketMod {
        watching: AtomicBool::new(false),
        ticks: AtomicU64::new(0),
        names: Mutex::new(HashMap::new()),
    })
}

struct PacketMod {
    /// Whether `watch_server_packets` was called (once, from the first tick).
    watching: AtomicBool,
    ticks: AtomicU64,
    /// Player names by uuid, refreshed every few ticks: `server_packet` has no host to ask.
    names: Mutex<HashMap<u128, String>>,
}

impl PacketMod {
    fn user(&self, uuid: u128) -> User {
        let name = self
            .names
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&uuid)
            .cloned()
            .unwrap_or_default();
        User {
            uuid,
            name,
            state: ConnectionState::Play,
        }
    }
}

impl Hooks for PacketMod {
    fn tick(&self, host: &mut dyn Host, _event: &mut TickEvent) {
        // Built against 1453e0e61; not yet run. `watch_server_packets` is the helper of the `server_packet` fragment
        // (`native_watch_server_packets`); until it is called the server sends packets undecoded.
        if !self.watching.swap(true, Ordering::Relaxed) {
            host.watch_server_packets();
        }
        let tick = self.ticks.fetch_add(1, Ordering::Relaxed);
        if tick % NAME_EVERY_TICKS == 0 {
            let mut names = self.names.lock().unwrap_or_else(|e| e.into_inner());
            for p in host.server_players() {
                names.insert(p.uuid, p.name.clone());
            }
        }
    }

    /// Every play packet the server is about to send a guest (`native_server_packet`).
    fn server_packet(&self, event: &mut ServerPacketEvent<'_>) -> Flow {
        let frame = event.packet.encode();
        let Some((id, body)) = split_frame(&frame) else {
            return Flow::Continue;
        };
        let Some(packet_type) = PacketType::lookup(ConnectionState::Play, true, id) else {
            return Flow::Continue;
        };
        let user = self.user(event.player);
        match packet_events().dispatch_send(&user, packet_type, body) {
            Outcome::Unchanged => Flow::Continue,
            Outcome::Cancelled => Flow::Cancel,
            // A body that no longer decodes leaves the packet as it was; the listener is at fault.
            Outcome::Changed(new_body) => {
                if let Ok(changed) = ClientboundPlayPacket::decode(id, &new_body) {
                    *event.packet = changed;
                }
                Flow::Continue
            }
        }
    }

    /// A plugin channel message from a player (`native_custom_payload_from_client`). Listen-only:
    /// the message reaches the server whatever the listeners say (GAPS.md).
    fn custom_payload_from_client(&self, _host: &mut dyn Host, event: &mut CustomPayloadFromClientEvent<'_>) {
        let body = PluginMessage {
            channel: event.channel.to_owned(),
            data: event.data.to_vec(),
        }
        .to_body();
        let user = self.user(event.player);
        let _ = packet_events().dispatch_receive(
            &user,
            PacketType::PlayClient(PlayClient::PLUGIN_MESSAGE),
            &body,
        );
    }
}

/// Splits a frame (VarInt id, then the body) into the id and the body.
fn split_frame(frame: &[u8]) -> Option<(i32, &[u8])> {
    let mut id: u32 = 0;
    for (i, byte) in frame.iter().enumerate().take(5) {
        id |= u32::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((id as i32, &frame[i + 1..]));
        }
    }
    None
}

/// Horizon's own item and particle codecs measure what the core cannot (`pe::Registry`).
struct HorizonRegistry;

impl Registry for HorizonRegistry {
    // UNVERIFIED: `Slot` (horizon_protocol::item) and `Particle` (entity_data) are the codecs
    // the play packets use; their `read` takes a `horizon_protocol::buf::Reader`.
    fn item_len(&self, data: &[u8]) -> pe::Result<usize> {
        use horizon_protocol::buf::{Codec, Reader};
        let mut r = Reader::new(data);
        let _stack: Option<horizon_protocol::item::Stack> =
            <horizon_protocol::item::Slot as Codec<Option<horizon_protocol::item::Stack>>>::read(
                &mut r,
            )
            .map_err(|e| pe::Error::new(format!("item: {e:?}")))?;
        Ok(data.len() - r.remaining())
    }

    fn particle_len(&self, data: &[u8]) -> pe::Result<usize> {
        use horizon_protocol::buf::Reader;
        let mut r = Reader::new(data);
        horizon_protocol::entity_data::Particle::read(&mut r)
            .map_err(|e| pe::Error::new(format!("particle: {e:?}")))?;
        Ok(data.len() - r.remaining())
    }

    // Entity values keep the core's fixed-size table: Horizon's single-value reader is private
    // (`entity_data::read_value`), see GAPS.md.
}

/// The sending side of a typed packet to one player. The engine has no helper for a clientbound
/// play packet yet (`native_send` is internal to the server): it reports false until one exists.
#[allow(dead_code)]
fn send_to_player(_host: &mut dyn Host, _uuid: u128, _packet: &ClientboundPlayPacket) -> bool {
    false
}
