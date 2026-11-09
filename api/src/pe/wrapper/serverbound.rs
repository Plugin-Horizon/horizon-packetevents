//! Ported from packetevents (https://github.com/retrooper/packetevents), Copyright (C) 2026
//! retrooper and contributors, GPL-3.0-or-later. See NOTICE. The structure (packet types,
//! events, priorities, wrappers) follows packetevents; the wire layouts are Minecraft 26.3's.
//!
//! Serverbound play packets (client to server), as packetevents' `WrapperPlayClient*`.
//! Layouts are those of Minecraft 26.3.

use super::Wrapper;
use crate::pe::codec::{Reader, Registry, Result, Writer};
use crate::pe::packet_type::{PacketType, PlayClient};

/// A plugin channel message from the client (`CustomPayload`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl Wrapper for PluginMessage {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLUGIN_MESSAGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PluginMessage { channel: r.identifier()?, data: r.rest().to_vec() })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.identifier(&self.channel);
        w.raw(&self.data);
    }
}

/// The client's answer to a keep alive (`KeepAlive`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeepAlive {
    pub id: i64,
}

impl Wrapper for KeepAlive {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::KEEP_ALIVE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(KeepAlive { id: r.i64()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.i64(self.id);
    }
}

/// Right click on an entity (`Interact`): the entity, the hand, the point hit and sneaking.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractEntity {
    pub entity_id: i32,
    /// 0 main hand, 1 off hand.
    pub hand: i32,
    /// Where on the entity the click landed (relative to it), low-precision.
    pub location: [f64; 3],
    pub sneaking: bool,
}

impl Wrapper for InteractEntity {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::INTERACT_ENTITY);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(InteractEntity {
            entity_id: r.varint()?,
            hand: r.varint()?,
            location: r.lp_vec3()?,
            sneaking: r.bool()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.entity_id);
        w.varint(self.hand);
        w.lp_vec3(self.location);
        w.bool(self.sneaking);
    }
}

/// A melee hit on an entity (`Attack`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackEntity {
    pub entity_id: i32,
}

impl Wrapper for AttackEntity {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::ATTACK);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(AttackEntity { entity_id: r.varint()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.entity_id);
    }
}

/// Position of the player (`MovePlayerPos`); flags: bit 0 on the ground, bit 1 against a wall.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerPosition {
    pub position: [f64; 3],
    pub flags: u8,
}

impl Wrapper for PlayerPosition {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLAYER_POSITION);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PlayerPosition { position: [r.f64()?, r.f64()?, r.f64()?], flags: r.u8()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        for c in self.position {
            w.f64(c);
        }
        w.u8(self.flags);
    }
}

/// Position and rotation of the player (`MovePlayerPosRot`); angles in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerPositionAndRotation {
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
}

impl Wrapper for PlayerPositionAndRotation {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLAYER_POSITION_AND_ROTATION);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PlayerPositionAndRotation {
            position: [r.f64()?, r.f64()?, r.f64()?],
            yaw: r.f32()?,
            pitch: r.f32()?,
            flags: r.u8()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        for c in self.position {
            w.f64(c);
        }
        w.f32(self.yaw);
        w.f32(self.pitch);
        w.u8(self.flags);
    }
}

/// Rotation of the player (`MovePlayerRot`); angles in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerRotation {
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
}

impl Wrapper for PlayerRotation {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLAYER_ROTATION);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PlayerRotation { yaw: r.f32()?, pitch: r.f32()?, flags: r.u8()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.f32(self.yaw);
        w.f32(self.pitch);
        w.u8(self.flags);
    }
}

/// Only the on-ground flags changed (`MovePlayerStatus`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerFlying {
    pub flags: u8,
}

impl Wrapper for PlayerFlying {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLAYER_FLYING);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PlayerFlying { flags: r.u8()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.u8(self.flags);
    }
}

/// A block action by the player (`PlayerAction`): 0 start breaking, 1 change the direction,
/// 2 abort, 3 finish breaking, 4 drop the stack, 5 drop one, 6 release the item in use, 7 swap
/// hands, 8 stab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDigging {
    pub action: i32,
    pub position: [i32; 3],
    /// The block face (0 to 5) as the client has it.
    pub direction: u8,
    pub sequence: i32,
}

impl Wrapper for PlayerDigging {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLAYER_DIGGING);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PlayerDigging {
            action: r.varint()?,
            position: r.block_pos()?,
            direction: r.u8()?,
            sequence: r.varint()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.action);
        w.position(self.position);
        w.u8(self.direction);
        w.varint(self.sequence);
    }
}

/// Use an item on a block (`UseItemOn`): the block clicked, its face and the point in it.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPlace {
    pub hand: i32,
    pub position: [i32; 3],
    /// The face clicked (`Direction` id: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
    pub face: i32,
    pub cursor: [f32; 3],
    pub inside: bool,
    pub world_border_hit: bool,
    pub sequence: i32,
}

impl Wrapper for BlockPlace {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PLAYER_BLOCK_PLACEMENT);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(BlockPlace {
            hand: r.varint()?,
            position: r.block_pos()?,
            face: r.varint()?,
            cursor: [r.f32()?, r.f32()?, r.f32()?],
            inside: r.bool()?,
            world_border_hit: r.bool()?,
            sequence: r.varint()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.hand);
        w.position(self.position);
        w.varint(self.face);
        for c in self.cursor {
            w.f32(c);
        }
        w.bool(self.inside);
        w.bool(self.world_border_hit);
        w.varint(self.sequence);
    }
}

/// The hotbar slot selected (`SetCarriedItem`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldItemChange {
    pub slot: i16,
}

impl Wrapper for HeldItemChange {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::HELD_ITEM_CHANGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(HeldItemChange { slot: r.i16()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.i16(self.slot);
    }
}

/// The player swung the main hand (`Punch`, which has no fields in 26.3).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Animation;

impl Wrapper for Animation {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::PUNCH);

    fn read_fields(_r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(Animation)
    }

    fn write_fields(&self, _w: &mut Writer) {}
}

/// The client's options (`ClientInformation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientSettings {
    pub locale: String,
    pub view_distance: i8,
    /// 0 full, 1 system messages only, 2 hidden.
    pub chat_mode: i32,
    pub chat_colors: bool,
    /// Skin layer bits.
    pub displayed_skin_parts: u8,
    /// 0 left, 1 right.
    pub main_hand: i32,
    pub text_filtering: bool,
    pub allow_server_listings: bool,
    /// 0 all, 1 decreased, 2 minimal.
    pub particle_status: i32,
}

impl Wrapper for ClientSettings {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::CLIENT_SETTINGS);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ClientSettings {
            locale: r.string(16)?,
            view_distance: r.i8()?,
            chat_mode: r.varint()?,
            chat_colors: r.bool()?,
            displayed_skin_parts: r.u8()?,
            main_hand: r.varint()?,
            text_filtering: r.bool()?,
            allow_server_listings: r.bool()?,
            particle_status: r.varint()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.string(&self.locale);
        w.i8(self.view_distance);
        w.varint(self.chat_mode);
        w.bool(self.chat_colors);
        w.u8(self.displayed_skin_parts);
        w.varint(self.main_hand);
        w.bool(self.text_filtering);
        w.bool(self.allow_server_listings);
        w.varint(self.particle_status);
    }
}

/// Command completion request (`CommandSuggestion`): the request id and the text typed so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabComplete {
    pub id: i32,
    pub text: String,
}

impl Wrapper for TabComplete {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::TAB_COMPLETE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(TabComplete { id: r.varint()?, text: r.string(32500)? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.id);
        w.string(&self.text);
    }
}

/// The client's answer to a resource pack (`ResourcePackResponse`): the pack and the action
/// (0 loaded, 1 declined, 2 failed download, 3 accepted, 4 downloaded, 5 invalid URL,
/// 6 failed reload, 7 discarded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePackStatus {
    pub uuid: u128,
    pub action: i32,
}

impl Wrapper for ResourcePackStatus {
    const TYPE: PacketType = PacketType::PlayClient(PlayClient::RESOURCE_PACK_STATUS);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ResourcePackStatus { uuid: r.uuid()?, action: r.varint()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.uuid(self.uuid);
        w.varint(self.action);
    }
}
