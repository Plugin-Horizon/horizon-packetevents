//! Ported from packetevents (https://github.com/retrooper/packetevents), Copyright (C) 2026
//! retrooper and contributors, GPL-3.0-or-later. See NOTICE. The structure (packet types,
//! events, priorities, wrappers) follows packetevents; the wire layouts are Minecraft 26.3's.
//!
//! The listener registry and dispatch (packetevents' `PacketEventsAPI` and `EventManager`):
//! listeners run lowest priority first, equal priorities in the order they were added, and a
//! packet's outcome says whether it goes out unchanged, with changed bytes, or not at all.

use crate::pe::codec::Registry;
use crate::pe::event::{PacketEvent, PacketListener, Priority, User};
use crate::pe::packet_type::PacketType;
use std::sync::{Arc, RwLock};

/// Identifies a listener for `remove_listener`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

/// What happens to a packet after its listeners ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Sent or handled as it was.
    Unchanged,
    /// Sent or handled with this body instead.
    Changed(Vec<u8>),
    /// Not sent or not handled.
    Cancelled,
}

struct Entry {
    id: ListenerId,
    priority: Priority,
    listener: Arc<dyn PacketListener>,
}

/// The packet event API a mod uses: listeners, the registry the wrappers measure with, and the
/// dispatch the glue calls for each packet.
pub struct PacketEvents {
    registry: Box<dyn Registry>,
    listeners: RwLock<Vec<Entry>>,
    next_id: RwLock<u64>,
}

impl PacketEvents {
    pub fn new(registry: Box<dyn Registry>) -> PacketEvents {
        PacketEvents { registry, listeners: RwLock::new(Vec::new()), next_id: RwLock::new(0) }
    }

    /// The registry of the game's data (items, particles, entity values).
    pub fn registry(&self) -> &dyn Registry {
        self.registry.as_ref()
    }

    /// Adds a listener; it runs at its priority from the next packet on.
    pub fn add_listener(&self, listener: Arc<dyn PacketListener>) -> ListenerId {
        let id = {
            let mut next = self.next_id.write().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            ListenerId(*next)
        };
        let mut list = self.listeners.write().unwrap_or_else(|e| e.into_inner());
        let priority = listener.priority();
        // Insert after the last entry of the same or a lower priority: stable order.
        let at = list.iter().position(|e| e.priority > priority).unwrap_or(list.len());
        list.insert(at, Entry { id, priority, listener });
        id
    }

    /// Removes a listener; false when it was not there.
    pub fn remove_listener(&self, id: ListenerId) -> bool {
        let mut list = self.listeners.write().unwrap_or_else(|e| e.into_inner());
        let before = list.len();
        list.retain(|e| e.id != id);
        list.len() != before
    }

    /// The number of listeners.
    pub fn listener_count(&self) -> usize {
        self.listeners.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Runs the listeners on a packet the server is about to send to `user`.
    pub fn dispatch_send(&self, user: &User, packet: PacketType, body: &[u8]) -> Outcome {
        self.dispatch(user, packet, body, true)
    }

    /// Runs the listeners on a packet `user` sent to the server.
    pub fn dispatch_receive(&self, user: &User, packet: PacketType, body: &[u8]) -> Outcome {
        self.dispatch(user, packet, body, false)
    }

    fn dispatch(&self, user: &User, packet: PacketType, body: &[u8], send: bool) -> Outcome {
        // Clone the list so listeners may add or remove listeners while the packet runs.
        let listeners: Vec<Arc<dyn PacketListener>> = self
            .listeners
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|e| Arc::clone(&e.listener))
            .collect();
        let mut event = PacketEvent::new(packet, user.clone(), body.to_vec());
        for listener in &listeners {
            if send {
                listener.on_packet_send(&mut event);
            } else {
                listener.on_packet_receive(&mut event);
            }
        }
        if event.is_cancelled() {
            Outcome::Cancelled
        } else if event.needs_reencode() {
            Outcome::Changed(event.body().to_vec())
        } else {
            Outcome::Unchanged
        }
    }
}
