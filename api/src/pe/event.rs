//! Ported from packetevents (https://github.com/retrooper/packetevents), Copyright (C) 2026
//! retrooper and contributors, GPL-3.0-or-later. See NOTICE. The structure (packet types,
//! events, priorities, wrappers) follows packetevents; the wire layouts are Minecraft 26.3's.
//!
//! Packet events, as packetevents' `PacketEvent` family: a packet in one direction, the player it
//! belongs to, the body, and the flags a listener sets (cancel, re-encode).

use crate::pe::codec::{Registry, Result};
use crate::pe::packet_type::{ConnectionState, PacketType};
use crate::pe::wrapper::Wrapper;

/// The order listeners run in, lowest first (packetevents' `PacketListenerPriority`). `Monitor`
/// listeners look at the result and must not change it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Lowest,
    Low,
    Normal,
    High,
    Highest,
    Monitor,
}

impl Priority {
    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Priority> {
        [
            Priority::Lowest,
            Priority::Low,
            Priority::Normal,
            Priority::High,
            Priority::Highest,
            Priority::Monitor,
        ]
        .get(id as usize)
        .copied()
    }
}

/// The player a packet belongs to. Before login completes the uuid is zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub uuid: u128,
    pub name: String,
    pub state: ConnectionState,
}

/// A packet being sent to a player (`Server`, clientbound) or received from one (`Client`).
#[derive(Clone, Debug, PartialEq)]
pub struct PacketEvent {
    packet_type: PacketType,
    user: User,
    body: Vec<u8>,
    cancelled: bool,
    reencode: bool,
}

impl PacketEvent {
    /// A new event for a packet body (the bytes after the id).
    pub fn new(packet_type: PacketType, user: User, body: Vec<u8>) -> PacketEvent {
        PacketEvent {
            packet_type,
            user,
            body,
            cancelled: false,
            reencode: false,
        }
    }

    pub fn packet_type(&self) -> PacketType {
        self.packet_type
    }

    pub fn packet_id(&self) -> i32 {
        self.packet_type.id()
    }

    pub fn packet_name(&self) -> &'static str {
        self.packet_type.name()
    }

    /// True for packets the server sends to the player.
    pub fn is_server_side(&self) -> bool {
        self.packet_type.is_server_side()
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    /// The body as it is now (after the changes of earlier listeners).
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// A cancelled packet is not sent (sent to the server: not handled).
    pub fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    pub fn needs_reencode(&self) -> bool {
        self.reencode
    }

    /// Changes are only written when this is set (packetevents' `markForReEncode`). Setting a
    /// body or a wrapper marks it too.
    pub fn mark_for_reencode(&mut self, reencode: bool) {
        self.reencode = reencode;
    }

    /// Reads the body as the wrapper `W`; fails when `W` is for another packet.
    pub fn read<W: Wrapper>(&self, reg: &dyn Registry) -> Result<W> {
        if W::TYPE != self.packet_type {
            return Err(crate::pe::codec::Error::new(format!(
                "event is {} and not {}",
                self.packet_type.name(),
                W::TYPE.name()
            )));
        }
        W::from_body(&self.body, reg)
    }

    /// Replaces the body with `wrapper`'s and marks the event for re-encoding.
    pub fn write<W: Wrapper>(&mut self, wrapper: &W) -> Result<()> {
        if W::TYPE != self.packet_type {
            return Err(crate::pe::codec::Error::new(format!(
                "event is {} and not {}",
                self.packet_type.name(),
                W::TYPE.name()
            )));
        }
        self.body = wrapper.to_body();
        self.reencode = true;
        Ok(())
    }

    /// Replaces the body with raw bytes and marks the event for re-encoding.
    pub fn set_body(&mut self, body: Vec<u8>) {
        self.body = body;
        self.reencode = true;
    }
}

/// A listener on packets. Both methods have an empty default, so a listener implements the side
/// it needs. A listener is called for every packet, cancelled or not, and checks
/// `is_cancelled` itself, as packetevents' listeners do.
pub trait PacketListener: Send + Sync {
    /// Where in the order the listener runs.
    fn priority(&self) -> Priority {
        Priority::Normal
    }

    /// A packet the server is about to send.
    fn on_packet_send(&self, _event: &mut PacketEvent) {}

    /// A packet the client sent to the server.
    fn on_packet_receive(&self, _event: &mut PacketEvent) {}
}
