//! horizon-packetevents as an arismoko native mod (trait mod on `mod_api`). The packet model
//! (packet types, events, listeners with priorities, typed wrappers) is the unchanged
//! `horizon-packetevents-core` crate (`core/`); this file wires it to the hooks arismoko has.
//!
//! What arismoko does not have yet (see PORT_NOTES.md, every `TODO(arismoko-api: ...)`):
//! - no hook for the play packets the server sends a player (`server_packet`), so nothing is
//!   heard and the send-side listeners never run; the dispatch below is kept, compiled-out;
//! - no cancellable receive hook for serverbound packets on main. The `serverbound_packet` hook of
//!   horizon-rust/horizon#18803 is used behind the cargo feature `horizon_serverbound_packet` (off by
//!   default) and hears custom payloads only. `net_from_client` is a parameterless stub, not used;
//! - no registry codec for item slots and particles (`registry_protocol_id`), so the core's
//!   basic measures are used and anything else stays as it was sent.

pub mod horizon_packetevents_core;
use crate::horizon_packetevents_core::pe::{
    BasicRegistry, ConnectionState, Outcome, PacketEvents, PacketType, User,
};
#[cfg(feature = "horizon_serverbound_packet")]
use crate::horizon_packetevents_core::pe::packet_type::PlayClient;
#[cfg(feature = "horizon_serverbound_packet")]
use crate::horizon_packetevents_core::pe::wrapper::Wrapper;
#[cfg(feature = "horizon_serverbound_packet")]
use crate::horizon_packetevents_core::pe::wrapper::serverbound::PluginMessage;
use mod_api::{
    Hooks, Identifier, PlayerLoggedInEvent, PlayerLoggedOutEvent, ServerHost, ServerTickEvent,
    ServerTickPhase,
};
#[cfg(feature = "horizon_serverbound_packet")]
use mod_api::{Flow, ServerboundPacketEvent};
#[cfg(feature = "horizon_player_send_packet")]
use mod_api::ClientboundPlayPacket;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Whether a packet source is wired to the listeners. It is not: Horizon has no server-side packet
/// hook yet (see the Known gaps in the README), so `add_listener` registers a listener that is never
/// called. A mod that depends on interception can check this first.
pub const PACKET_SOURCE_WIRED: bool = false;

/// Whether the receive side is wired: a player's custom payloads reach the receive listeners and a
/// listener's cancel drops the payload. True only with the cargo feature `horizon_serverbound_packet`
/// (the `serverbound_packet` hook of horizon-rust/horizon#18803, not on main yet). Other serverbound
/// packets never reach the listeners: their bodies are not in the hook.
pub const RECEIVE_SOURCE_WIRED: bool = cfg!(feature = "horizon_serverbound_packet");

/// The mod's packet API, for other mods that depend on this crate: `packet_events().add_listener`.
pub fn packet_events() -> &'static PacketEvents {
    static API: OnceLock<PacketEvents> = OnceLock::new();
    // TODO(arismoko-api: registry_protocol_id): item slots and particles are measured by the
    // core's basic rules only; the item and particle codecs of the game are not reachable.
    API.get_or_init(|| PacketEvents::new(Box::new(BasicRegistry)))
}

#[derive(Default)]
struct Mod {
    /// Player names by uuid, rebuilt every tick: a packet hook has no host to ask.
    names: Mutex<HashMap<u128, String>>,
}

/// What a send-side dispatch tells the engine to do with the packet.
#[allow(dead_code)]
enum SendAction {
    /// Sent as it was.
    Keep,
    /// Not sent.
    Cancel,
    /// Sent with this packet id and body instead.
    Replace(i32, Vec<u8>),
}

impl Mod {
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

    /// The send-side dispatch of one play packet (id and body, without the frame), as a packet
    /// hook would hand it over. Kept compiled-out until arismoko has `server_packet`
    /// (TODO(arismoko-api: server_packet)).
    #[allow(dead_code)]
    fn dispatch_server_packet(&self, uuid: u128, id: i32, body: &[u8]) -> SendAction {
        let Some(packet_type) = PacketType::lookup(ConnectionState::Play, true, id) else {
            return SendAction::Keep;
        };
        match packet_events().dispatch_send(&self.user(uuid), packet_type, body) {
            Outcome::Unchanged => SendAction::Keep,
            Outcome::Cancelled => SendAction::Cancel,
            // A body that no longer decodes leaves the packet as it was; the listener is at fault.
            Outcome::Changed(new_body) => SendAction::Replace(id, new_body),
        }
    }

    /// A plugin channel message from a player, as the serverbound hook hands it over. Returns the
    /// outcome: `Cancelled` when a listener dropped it. A changed body cannot be applied (the hook
    /// has no rewrite), so `Changed` is treated as `Unchanged` by the caller.
    #[cfg(feature = "horizon_serverbound_packet")]
    fn dispatch_plugin_message(&self, uuid: u128, channel: &str, data: &[u8]) -> Outcome {
        let body = PluginMessage {
            channel: channel.to_owned(),
            data: data.to_vec(),
        }
        .to_body();
        packet_events().dispatch_receive(
            &self.user(uuid),
            PacketType::PlayClient(PlayClient::PLUGIN_MESSAGE),
            &body,
        )
    }
}

/// Keeps the name map in step with the join and the leave, not only at the next tick.
impl Mod {
    fn remember_name(&self, uuid: u128, name: &str) {
        self.names
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(uuid, name.to_owned());
    }

    fn forget_name(&self, uuid: u128) {
        self.names
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&uuid);
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickStart { world_running: true, .. }) {
            return;
        }
        // TODO(arismoko-api: server_packet): the outgoing packet watch (`watch_server_packets` in
        // the horizon-rust port) is switched on here once the hook exists; until then no packet
        // is heard.
        // Rebuilt, not added to: a player who left is gone and one who joined has a name at once.
        let mut names = HashMap::new();
        for p in host.players().server_players() {
            // UNVERIFIED: `PlayerRef::name(host)` is named in docs/modding (001, "Server services
            // and player handles"); its exact return type is not in the source tree.
            if let Some(name) = p.name(&*host) {
                names.insert(p.uuid(), name);
            }
        }
        *self.names.lock().unwrap_or_else(|e| e.into_inner()) = names;
    }

    fn player_logged_in(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        self.remember_name(event.player, event.name);
    }

    fn player_logged_out(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        self.forget_name(event.player);
    }

    /// Receive side, behind the cargo feature `horizon_serverbound_packet` (off by default: the hook
    /// is not on main yet). Only custom payloads carry a body, so only they reach the listeners; a
    /// listener that cancels one drops it (`Flow::Cancel`).
    #[cfg(feature = "horizon_serverbound_packet")]
    fn serverbound_packet(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut ServerboundPacketEvent<'_>,
    ) -> Flow {
        let Some(payload) = event.packet.custom_payload.as_ref() else {
            return Flow::Continue;
        };
        match self.dispatch_plugin_message(event.player, &payload.channel, &payload.data) {
            Outcome::Cancelled => Flow::Cancel,
            Outcome::Unchanged | Outcome::Changed(_) => Flow::Continue,
        }
    }
}

/// Sending a typed packet to one player (`send_to_player` in the horizon-rust port). `name` is the
/// vanilla packet type with its namespace (`minecraft:set_player_team`), not the core's PacketEvents name: the core calls
/// some packets differently (`teams`), so the core's id-to-name is not used here. `player_send_packet`
/// checks the body and sends it as it is.
/// Behind the cargo feature `horizon_player_send_packet` (horizon-rust/horizon#18021, not on main yet);
/// without it nothing is sent and this returns false.
#[cfg(feature = "horizon_player_send_packet")]
#[allow(dead_code)]
fn send_to_player(host: &mut dyn ServerHost, uuid: u128, name: &str, body: &[u8]) -> bool {
    let Ok(name) = Identifier::parse(name) else {
        return false;
    };
    let Some(player) = host.players().server_players().into_iter().find(|p| p.uuid() == uuid) else {
        return false;
    };
    host.player_send_packet(player, ClientboundPlayPacket::new(name, body.to_vec()))
        .is_ok()
}

/// Without the feature `horizon_player_send_packet` the send is not available: nothing is sent.
#[cfg(not(feature = "horizon_player_send_packet"))]
#[allow(dead_code)]
fn send_to_player(host: &mut dyn ServerHost, uuid: u128, name: &str, body: &[u8]) -> bool {
    // HMOD-GAP(player-send-packet): Horizon main has no server-side send of a vanilla play packet to one
    // player (only declared payload channels, `PlayerRef::send_payload`). Nothing is sent.
    let _ = (host, uuid, name, body);
    false
}

/// Splits a frame (VarInt id, then the body) into the id and the body.
#[allow(dead_code)]
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
