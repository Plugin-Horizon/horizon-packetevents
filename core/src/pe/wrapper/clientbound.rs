//! Ported from packetevents (https://github.com/retrooper/packetevents), Copyright (C) 2026
//! retrooper and contributors, GPL-3.0-or-later. See NOTICE. The structure (packet types,
//! events, priorities, wrappers) follows packetevents; the wire layouts are Minecraft 26.3's.
//!
//! Clientbound play packets (server to client), as packetevents' `WrapperPlayServer*`.
//! Layouts are those of Minecraft 26.3.

use super::{
    EntityDataEntry, RawItem, RawParticle, Wrapper, read_entity_data, read_item, read_list,
    read_opt, write_entity_data, write_opt,
};
use crate::pe::codec::{Reader, Registry, Result, Writer, pack_section_pos, unpack_section_pos};
use crate::pe::nbt::Tag;
use crate::pe::packet_type::{PacketType, PlayServer};

/// A sound event: a registry id, or an inline name with an optional range.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundRef {
    /// The `minecraft:sound_event` registry id.
    Registry(i32),
    /// An inline sound location (`minecraft:ambient.cave`), with its range when given.
    Inline { name: String, range: Option<f32> },
}

impl SoundRef {
    fn read(r: &mut Reader) -> Result<SoundRef> {
        let id = r.varint()?;
        if id != 0 {
            return Ok(SoundRef::Registry(id - 1));
        }
        let name = r.identifier()?;
        let range = read_opt(r, |r| r.f32())?;
        Ok(SoundRef::Inline { name, range })
    }

    fn write(&self, w: &mut Writer) {
        match self {
            SoundRef::Registry(id) => w.varint(id + 1),
            SoundRef::Inline { name, range } => {
                w.varint(0);
                w.identifier(name);
                write_opt(w, range, |w, r| w.f32(*r));
            }
        }
    }
}

/// A player-list entry of `PlayerInfoUpdate`; each field is present when its action is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerInfoEntry {
    pub uuid: u128,
    /// ADD_PLAYER: name and profile properties.
    pub name: Option<String>,
    pub properties: Vec<Property>,
    /// INITIALIZE_CHAT: session id, expiry, public key and key signature.
    pub chat_session: Option<(u128, i64, Vec<u8>, Vec<u8>)>,
    /// UPDATE_GAME_MODE.
    pub game_mode: Option<i32>,
    /// UPDATE_LISTED.
    pub listed: Option<bool>,
    /// UPDATE_LATENCY, in milliseconds.
    pub latency: Option<i32>,
    /// UPDATE_DISPLAY_NAME: `Some(None)` clears the display name.
    pub display_name: Option<Option<Tag>>,
    /// UPDATE_LIST_PRIORITY.
    pub list_order: Option<i32>,
    /// UPDATE_HAT.
    pub show_hat: Option<bool>,
}

/// A profile property (`com.mojang.authlib.properties.Property`), e.g. `textures`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

/// The number format of a score or an objective: blank, a style compound or a component.
#[derive(Clone, Debug, PartialEq)]
pub enum NumberFormat {
    Blank,
    Styled(Tag),
    Fixed(Tag),
}

impl NumberFormat {
    fn read(r: &mut Reader) -> Result<NumberFormat> {
        Ok(match r.varint()? {
            0 => NumberFormat::Blank,
            1 => NumberFormat::Styled(r.nbt()?),
            2 => NumberFormat::Fixed(r.nbt()?),
            other => {
                return Err(crate::pe::codec::Error::new(format!(
                    "number format {other}"
                )));
            }
        })
    }

    fn write(&self, w: &mut Writer) {
        match self {
            NumberFormat::Blank => w.varint(0),
            NumberFormat::Styled(t) => {
                w.varint(1);
                w.nbt(t);
            }
            NumberFormat::Fixed(t) => {
                w.varint(2);
                w.nbt(t);
            }
        }
    }
}

/// The parameters of a team (`Team` on the wire, methods 0 and 2).
#[derive(Clone, Debug, PartialEq)]
pub struct TeamParameters {
    pub display_name: Tag,
    pub prefix: Tag,
    pub suffix: Tag,
    /// `Team.Visibility` id: 0 always, 1 hide for other teams, 2 hide for own team, 3 never.
    pub name_tag_visibility: i32,
    /// `Team.CollisionRule` id.
    pub collision_rule: i32,
    /// The team colour (`ChatFormatting` id), none for no colour.
    pub color: Option<i32>,
    /// 1 friendly fire, 2 see invisible teammates.
    pub options: i8,
}

/// `minecraft:chat` on the wire: the player chat's signature cache entries are `(index, signature)`
/// pairs, as Horizon keeps them: an index into the client's cache, or a full 256-byte signature.
pub type PackedSignature = (i32, Option<Vec<u8>>);

/// Player chat (`PlayerChat`): a signed message from a player.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub global_index: i32,
    pub sender: u128,
    pub index: i32,
    /// The 256-byte signature, when signed.
    pub signature: Option<Vec<u8>>,
    pub content: String,
    pub timestamp: i64,
    pub salt: i64,
    pub last_seen: Vec<PackedSignature>,
    /// The text before filtering, when it differs.
    pub unsigned_content: Option<Tag>,
    /// Filter mask: type (0 pass, 1 fully filtered, 2 partially with the bits) and its longs.
    pub filter: (i32, Vec<i64>),
    /// The chat type holder as written (registry id plus one), with the name and target.
    pub chat_type: i32,
    pub chat_type_name: Tag,
    pub chat_type_target: Option<Tag>,
}

impl Wrapper for ChatMessage {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::CHAT_MESSAGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ChatMessage {
            global_index: r.varint()?,
            sender: r.uuid()?,
            index: r.varint()?,
            signature: read_opt(r, |r| read_bytes(r, 256))?,
            content: r.string(256)?,
            timestamp: r.i64()?,
            salt: r.i64()?,
            last_seen: read_list(r, 4096, |r| {
                let id = r.varint()? - 1;
                if id == -1 {
                    Ok((-1, Some(read_bytes(r, 256)?)))
                } else {
                    Ok((id, None))
                }
            })?,
            unsigned_content: read_opt(r, |r| r.nbt())?,
            filter: {
                let kind = r.varint()?;
                let longs = if kind == 2 {
                    read_list(r, 4096, |r| r.i64())?
                } else {
                    Vec::new()
                };
                (kind, longs)
            },
            chat_type: r.varint()?,
            chat_type_name: r.nbt()?,
            chat_type_target: read_opt(r, |r| r.nbt())?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.global_index);
        w.uuid(self.sender);
        w.varint(self.index);
        write_opt(w, &self.signature, |w, s| w.raw(s));
        w.string(&self.content);
        w.i64(self.timestamp);
        w.i64(self.salt);
        w.varint(self.last_seen.len() as i32);
        for (id, sig) in &self.last_seen {
            w.varint(id + 1);
            if let Some(s) = sig {
                w.raw(s);
            }
        }
        write_opt(w, &self.unsigned_content, |w, t| w.nbt(t));
        w.varint(self.filter.0);
        if self.filter.0 == 2 {
            w.varint(self.filter.1.len() as i32);
            for l in &self.filter.1 {
                w.i64(*l);
            }
        }
        w.varint(self.chat_type);
        w.nbt(&self.chat_type_name);
        write_opt(w, &self.chat_type_target, |w, t| w.nbt(t));
    }
}

fn read_bytes(r: &mut Reader, n: usize) -> Result<Vec<u8>> {
    Ok(r.bytes(n)?.to_vec())
}

/// A system message (`SystemChat`), in the chat or, with `overlay`, the action bar.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemChatMessage {
    pub content: Tag,
    pub overlay: bool,
}

impl Wrapper for SystemChatMessage {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SYSTEM_CHAT_MESSAGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(SystemChatMessage {
            content: r.nbt()?,
            overlay: r.bool()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.nbt(&self.content);
        w.bool(self.overlay);
    }
}

/// The title line (`SetTitleText`).
#[derive(Clone, Debug, PartialEq)]
pub struct SetTitleText {
    pub text: Tag,
}

impl Wrapper for SetTitleText {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SET_TITLE_TEXT);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(SetTitleText { text: r.nbt()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.nbt(&self.text);
    }
}

/// The subtitle line (`SetTitleSubtitle`).
#[derive(Clone, Debug, PartialEq)]
pub struct SetTitleSubtitle {
    pub text: Tag,
}

impl Wrapper for SetTitleSubtitle {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SET_TITLE_SUBTITLE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(SetTitleSubtitle { text: r.nbt()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.nbt(&self.text);
    }
}

/// Fade in, stay and fade out, in ticks (`SetTitlesAnimation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetTitleTimes {
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

impl Wrapper for SetTitleTimes {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SET_TITLE_TIMES);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(SetTitleTimes {
            fade_in: r.i32()?,
            stay: r.i32()?,
            fade_out: r.i32()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.i32(self.fade_in);
        w.i32(self.stay);
        w.i32(self.fade_out);
    }
}

/// Clears the title; `reset` also resets the times (`ClearTitles`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClearTitles {
    pub reset: bool,
}

impl Wrapper for ClearTitles {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::CLEAR_TITLES);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ClearTitles { reset: r.bool()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.bool(self.reset);
    }
}

/// The action bar line (`SetActionBarText`).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionBar {
    pub text: Tag,
}

impl Wrapper for ActionBar {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::ACTION_BAR);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ActionBar { text: r.nbt()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.nbt(&self.text);
    }
}

/// A boss bar action (`BossEvent`).
#[derive(Clone, Debug, PartialEq)]
pub enum BossBarAction {
    Add {
        name: Tag,
        progress: f32,
        /// The `BossEvent.BossBarColor` id.
        color: i32,
        /// The `BossEvent.BossBarOverlay` id.
        overlay: i32,
        darken_sky: bool,
        play_music: bool,
        create_fog: bool,
    },
    Remove,
    UpdateProgress(f32),
    UpdateName(Tag),
    UpdateStyle {
        color: i32,
        overlay: i32,
    },
    UpdateFlags {
        darken_sky: bool,
        play_music: bool,
        create_fog: bool,
    },
}

/// A boss bar (`BossEvent`): its id and what changes.
#[derive(Clone, Debug, PartialEq)]
pub struct BossBar {
    pub uuid: u128,
    pub action: BossBarAction,
}

impl Wrapper for BossBar {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::BOSS_BAR);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        let uuid = r.uuid()?;
        let flags = |b: u8| (b & 1 != 0, b & 2 != 0, b & 4 != 0);
        let action = match r.varint()? {
            0 => {
                let name = r.nbt()?;
                let progress = r.f32()?;
                let color = r.varint()?;
                let overlay = r.varint()?;
                let (darken_sky, play_music, create_fog) = flags(r.u8()?);
                BossBarAction::Add {
                    name,
                    progress,
                    color,
                    overlay,
                    darken_sky,
                    play_music,
                    create_fog,
                }
            }
            1 => BossBarAction::Remove,
            2 => BossBarAction::UpdateProgress(r.f32()?),
            3 => BossBarAction::UpdateName(r.nbt()?),
            4 => BossBarAction::UpdateStyle {
                color: r.varint()?,
                overlay: r.varint()?,
            },
            5 => {
                let (darken_sky, play_music, create_fog) = flags(r.u8()?);
                BossBarAction::UpdateFlags {
                    darken_sky,
                    play_music,
                    create_fog,
                }
            }
            other => {
                return Err(crate::pe::codec::Error::new(format!(
                    "boss bar action {other}"
                )));
            }
        };
        Ok(BossBar { uuid, action })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.uuid(self.uuid);
        let bits =
            |d: bool, m: bool, f: bool| u8::from(d) | (u8::from(m) << 1) | (u8::from(f) << 2);
        match &self.action {
            BossBarAction::Add {
                name,
                progress,
                color,
                overlay,
                darken_sky,
                play_music,
                create_fog,
            } => {
                w.varint(0);
                w.nbt(name);
                w.f32(*progress);
                w.varint(*color);
                w.varint(*overlay);
                w.u8(bits(*darken_sky, *play_music, *create_fog));
            }
            BossBarAction::Remove => w.varint(1),
            BossBarAction::UpdateProgress(p) => {
                w.varint(2);
                w.f32(*p);
            }
            BossBarAction::UpdateName(n) => {
                w.varint(3);
                w.nbt(n);
            }
            BossBarAction::UpdateStyle { color, overlay } => {
                w.varint(4);
                w.varint(*color);
                w.varint(*overlay);
            }
            BossBarAction::UpdateFlags {
                darken_sky,
                play_music,
                create_fog,
            } => {
                w.varint(5);
                w.u8(bits(*darken_sky, *play_music, *create_fog));
            }
        }
    }
}

/// Synched entity data (`SetEntityData`): the entity's values, changed or all of them.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityMetadata {
    pub entity_id: i32,
    pub values: Vec<EntityDataEntry>,
}

impl Wrapper for EntityMetadata {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::ENTITY_METADATA);

    fn read_fields(r: &mut Reader, reg: &dyn Registry) -> Result<Self> {
        Ok(EntityMetadata {
            entity_id: r.varint()?,
            values: read_entity_data(r, reg)?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.entity_id);
        write_entity_data(w, &self.values);
    }
}

/// An entity appearing (`AddEntity`).
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnEntity {
    pub entity_id: i32,
    pub uuid: u128,
    /// The `minecraft:entity_type` registry id.
    pub kind: i32,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    /// Angles in 1/256 of a turn.
    pub pitch: i8,
    pub yaw: i8,
    pub head_yaw: i8,
    /// The entity's type-specific data (`minecraft:arrow`'s owner, a falling block's state ...).
    pub data: i32,
}

impl Wrapper for SpawnEntity {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SPAWN_ENTITY);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(SpawnEntity {
            entity_id: r.varint()?,
            uuid: r.uuid()?,
            kind: r.varint()?,
            position: [r.f64()?, r.f64()?, r.f64()?],
            velocity: r.lp_vec3()?,
            pitch: r.i8()?,
            yaw: r.i8()?,
            head_yaw: r.i8()?,
            data: r.varint()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.entity_id);
        w.uuid(self.uuid);
        w.varint(self.kind);
        for c in self.position {
            w.f64(c);
        }
        w.lp_vec3(self.velocity);
        w.i8(self.pitch);
        w.i8(self.yaw);
        w.i8(self.head_yaw);
        w.varint(self.data);
    }
}

/// An entity moved to an absolute place (`TeleportEntity`).
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTeleport {
    pub entity_id: i32,
    pub position: [f64; 3],
    /// The velocity (delta movement).
    pub delta: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `RelativeMovement` bits: 1 x, 2 y, 4 z, 8 y rotation, 16 x rotation, 32/64/128 delta, 256 rotate delta.
    pub relatives: i32,
    pub on_ground: bool,
}

impl Wrapper for EntityTeleport {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::ENTITY_TELEPORT);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(EntityTeleport {
            entity_id: r.varint()?,
            position: [r.f64()?, r.f64()?, r.f64()?],
            delta: [r.f64()?, r.f64()?, r.f64()?],
            yaw: r.f32()?,
            pitch: r.f32()?,
            relatives: r.i32()?,
            on_ground: r.bool()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.entity_id);
        for c in self.position.iter().chain(&self.delta) {
            w.f64(*c);
        }
        w.f32(self.yaw);
        w.f32(self.pitch);
        w.i32(self.relatives);
        w.bool(self.on_ground);
    }
}

/// One block changes (`BlockUpdate`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockChange {
    pub position: [i32; 3],
    /// The block state id.
    pub state: i32,
}

impl Wrapper for BlockChange {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::BLOCK_CHANGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(BlockChange {
            position: r.block_pos()?,
            state: r.varint()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.position(self.position);
        w.varint(self.state);
    }
}

/// Several blocks of one 16 by 16 by 16 section (`SectionBlocksUpdate`). Each entry packs the
/// block state id (above 12 bits) and the block's place in the section (4 bits each for x, z, y).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiBlockChange {
    pub section: [i32; 3],
    pub changes: Vec<i64>,
}

impl MultiBlockChange {
    /// The block state id and the block's position in the world, for one entry.
    pub fn decode_entry(&self, entry: i64) -> (i32, [i32; 3]) {
        let state = (entry >> 12) as i32;
        let local = (entry & 0xFFF) as i32;
        let base = [
            self.section[0] * 16,
            self.section[1] * 16,
            self.section[2] * 16,
        ];
        (
            state,
            [
                base[0] + (local >> 8 & 0xF),
                base[1] + (local & 0xF),
                base[2] + (local >> 4 & 0xF),
            ],
        )
    }
}

impl Wrapper for MultiBlockChange {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::MULTI_BLOCK_CHANGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        let section = unpack_section_pos(r.i64()?);
        let changes = read_list(r, 65536, |r| r.varlong())?;
        Ok(MultiBlockChange { section, changes })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.i64(pack_section_pos(self.section));
        w.varint(self.changes.len() as i32);
        for c in &self.changes {
            w.varlong(*c);
        }
    }
}

/// Entries added or changed in the player list (`PlayerInfoUpdate`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfoUpdate {
    /// The actions bit set: 1 add, 2 chat session, 4 game mode, 8 listed, 16 latency,
    /// 32 display name, 64 list order, 128 hat.
    pub actions: u8,
    pub entries: Vec<PlayerInfoEntry>,
}

impl Wrapper for PlayerInfoUpdate {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::PLAYER_INFO_UPDATE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        let actions = r.u8()?;
        let entries = read_list(r, 4096, |r| {
            let mut e = PlayerInfoEntry {
                uuid: r.uuid()?,
                ..PlayerInfoEntry::default()
            };
            if actions & 1 != 0 {
                e.name = Some(r.string(16)?);
                e.properties = read_list(r, 16, |r| {
                    Ok(Property {
                        name: r.string(64)?,
                        value: r.string(32767)?,
                        signature: read_opt(r, |r| r.string(1024))?,
                    })
                })?;
            }
            if actions & 2 != 0 {
                e.chat_session = read_opt(r, |r| {
                    Ok((r.uuid()?, r.i64()?, r.byte_array(512)?, r.byte_array(4096)?))
                })?;
            }
            if actions & 4 != 0 {
                e.game_mode = Some(r.varint()?);
            }
            if actions & 8 != 0 {
                e.listed = Some(r.bool()?);
            }
            if actions & 16 != 0 {
                e.latency = Some(r.varint()?);
            }
            if actions & 32 != 0 {
                e.display_name = Some(read_opt(r, |r| r.nbt())?);
            }
            if actions & 64 != 0 {
                e.list_order = Some(r.varint()?);
            }
            if actions & 128 != 0 {
                e.show_hat = Some(r.bool()?);
            }
            Ok(e)
        })?;
        Ok(PlayerInfoUpdate { actions, entries })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.u8(self.actions);
        w.varint(self.entries.len() as i32);
        for e in &self.entries {
            w.uuid(e.uuid);
            if self.actions & 1 != 0 {
                w.string(e.name.as_deref().unwrap_or(""));
                w.varint(e.properties.len() as i32);
                for p in &e.properties {
                    w.string(&p.name);
                    w.string(&p.value);
                    write_opt(w, &p.signature, |w, s| w.string(s));
                }
            }
            if self.actions & 2 != 0 {
                write_opt(w, &e.chat_session, |w, (id, expires, key, sig)| {
                    w.uuid(*id);
                    w.i64(*expires);
                    w.byte_array(key);
                    w.byte_array(sig);
                });
            }
            if self.actions & 4 != 0 {
                w.varint(e.game_mode.unwrap_or(0));
            }
            if self.actions & 8 != 0 {
                w.bool(e.listed.unwrap_or(false));
            }
            if self.actions & 16 != 0 {
                w.varint(e.latency.unwrap_or(0));
            }
            if self.actions & 32 != 0 {
                write_opt(w, &e.display_name.clone().unwrap_or(None), |w, t| w.nbt(t));
            }
            if self.actions & 64 != 0 {
                w.varint(e.list_order.unwrap_or(0));
            }
            if self.actions & 128 != 0 {
                w.bool(e.show_hat.unwrap_or(false));
            }
        }
    }
}

/// Players leaving the player list (`PlayerInfoRemove`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerInfoRemove {
    pub uuids: Vec<u128>,
}

impl Wrapper for PlayerInfoRemove {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::PLAYER_INFO_REMOVE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PlayerInfoRemove {
            uuids: read_list(r, 4096, |r| r.uuid())?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.uuids.len() as i32);
        for u in &self.uuids {
            w.uuid(*u);
        }
    }
}

/// Creates (method 0), removes (1) or changes (2) an objective (`SetObjective`).
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreboardObjective {
    pub name: String,
    /// 0 create, 1 remove, 2 update.
    pub method: i8,
    /// Display name, render type (0 integer, 1 hearts) and number format: for methods 0 and 2.
    pub display: Option<(Tag, i32, Option<NumberFormat>)>,
}

impl Wrapper for ScoreboardObjective {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SCOREBOARD_OBJECTIVE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        let name = r.string(32767)?;
        let method = r.i8()?;
        let display = if method == 0 || method == 2 {
            Some((r.nbt()?, r.varint()?, read_opt(r, NumberFormat::read)?))
        } else {
            None
        };
        Ok(ScoreboardObjective {
            name,
            method,
            display,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.string(&self.name);
        w.i8(self.method);
        if self.method == 0 || self.method == 2 {
            let (name, render, format) = self.display.clone().unwrap_or((Tag::End, 0, None));
            w.nbt(&name);
            w.varint(render);
            write_opt(w, &format, |w, f| f.write(w));
        }
    }
}

/// One score of one objective (`SetScore`).
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateScore {
    pub owner: String,
    pub objective: String,
    pub score: i32,
    pub display: Option<Tag>,
    pub number_format: Option<NumberFormat>,
}

impl Wrapper for UpdateScore {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::UPDATE_SCORE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(UpdateScore {
            owner: r.string(32767)?,
            objective: r.string(32767)?,
            score: r.varint()?,
            display: read_opt(r, |r| r.nbt())?,
            number_format: read_opt(r, NumberFormat::read)?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.string(&self.owner);
        w.string(&self.objective);
        w.varint(self.score);
        write_opt(w, &self.display, |w, t| w.nbt(t));
        write_opt(w, &self.number_format, |w, f| f.write(w));
    }
}

/// Team changes (`SetPlayerTeam`): 0 adds with its parameters and players, 1 removes, 2 changes
/// the parameters, 3 adds players, 4 removes players.
#[derive(Clone, Debug, PartialEq)]
pub struct Teams {
    pub name: String,
    pub method: i8,
    pub parameters: Option<TeamParameters>,
    pub players: Vec<String>,
}

impl Wrapper for Teams {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::TEAMS);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        let name = r.string(32767)?;
        let method = r.i8()?;
        let parameters = if method == 0 || method == 2 {
            Some(TeamParameters {
                display_name: r.nbt()?,
                prefix: r.nbt()?,
                suffix: r.nbt()?,
                name_tag_visibility: r.varint()?,
                collision_rule: r.varint()?,
                color: read_opt(r, |r| r.varint())?,
                options: r.i8()?,
            })
        } else {
            None
        };
        let players = if matches!(method, 0 | 3 | 4) {
            read_list(r, 32767, |r| r.string(40))?
        } else {
            Vec::new()
        };
        Ok(Teams {
            name,
            method,
            parameters,
            players,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.string(&self.name);
        w.i8(self.method);
        if self.method == 0 || self.method == 2 {
            let p = self.parameters.clone().unwrap_or(TeamParameters {
                display_name: Tag::End,
                prefix: Tag::End,
                suffix: Tag::End,
                name_tag_visibility: 0,
                collision_rule: 0,
                color: None,
                options: 0,
            });
            w.nbt(&p.display_name);
            w.nbt(&p.prefix);
            w.nbt(&p.suffix);
            w.varint(p.name_tag_visibility);
            w.varint(p.collision_rule);
            write_opt(w, &p.color, |w, c| w.varint(*c));
            w.i8(p.options);
        }
        if matches!(self.method, 0 | 3 | 4) {
            w.varint(self.players.len() as i32);
            for p in &self.players {
                w.string(p);
            }
        }
    }
}

/// One slot of an open or player inventory (`ContainerSetSlot`).
#[derive(Clone, Debug, PartialEq)]
pub struct SetSlot {
    /// 0 for the player's inventory, otherwise the open container's id.
    pub window_id: i32,
    pub state_id: i32,
    pub slot: i16,
    pub item: RawItem,
}

impl Wrapper for SetSlot {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SET_SLOT);

    fn read_fields(r: &mut Reader, reg: &dyn Registry) -> Result<Self> {
        Ok(SetSlot {
            window_id: r.varint()?,
            state_id: r.varint()?,
            slot: r.i16()?,
            item: read_item(r, reg)?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.window_id);
        w.varint(self.state_id);
        w.i16(self.slot);
        w.raw(&self.item.0);
    }
}

/// A whole container's contents (`ContainerSetContent`), and the item held by the cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowItems {
    pub window_id: i32,
    pub state_id: i32,
    pub items: Vec<RawItem>,
    pub carried: RawItem,
}

impl Wrapper for WindowItems {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::WINDOW_ITEMS);

    fn read_fields(r: &mut Reader, reg: &dyn Registry) -> Result<Self> {
        let window_id = r.varint()?;
        let state_id = r.varint()?;
        let n = r.count(4096)?;
        let mut items = Vec::with_capacity(n.min(256));
        for _ in 0..n {
            items.push(read_item(r, reg)?);
        }
        Ok(WindowItems {
            window_id,
            state_id,
            items,
            carried: read_item(r, reg)?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.window_id);
        w.varint(self.state_id);
        w.varint(self.items.len() as i32);
        for i in &self.items {
            w.raw(&i.0);
        }
        w.raw(&self.carried.0);
    }
}

/// A container opened for the player (`OpenScreen`): its id, its menu type and title.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenWindow {
    pub window_id: i32,
    /// The `minecraft:menu` registry id.
    pub menu: i32,
    pub title: Tag,
}

impl Wrapper for OpenWindow {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::OPEN_WINDOW);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(OpenWindow {
            window_id: r.varint()?,
            menu: r.varint()?,
            title: r.nbt()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.varint(self.window_id);
        w.varint(self.menu);
        w.nbt(&self.title);
    }
}

/// Particles around a place (`LevelParticles`). The particle is kept as its bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    pub particle: RawParticle,
    pub override_limiter: bool,
    pub always_show: bool,
    pub position: [f64; 3],
    pub distance: [f32; 3],
    pub max_speed: [f32; 3],
    pub count: i32,
    pub randomization: i32,
}

impl Wrapper for Particle {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::PARTICLE);

    fn read_fields(r: &mut Reader, reg: &dyn Registry) -> Result<Self> {
        let n = reg.particle_len(r.peek())?;
        Ok(Particle {
            particle: RawParticle(r.bytes(n)?.to_vec()),
            override_limiter: r.bool()?,
            always_show: r.bool()?,
            position: [r.f64()?, r.f64()?, r.f64()?],
            distance: [r.f32()?, r.f32()?, r.f32()?],
            max_speed: [r.f32()?, r.f32()?, r.f32()?],
            count: r.varint()?,
            randomization: r.varint()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.raw(&self.particle.0);
        w.bool(self.override_limiter);
        w.bool(self.always_show);
        for c in self.position {
            w.f64(c);
        }
        for c in self.distance.iter().chain(&self.max_speed) {
            w.f32(*c);
        }
        w.varint(self.count);
        w.varint(self.randomization);
    }
}

/// A sound at a place (`Sound`); an inline name is the packetevents "named sound effect".
#[derive(Clone, Debug, PartialEq)]
pub struct SoundEffect {
    pub sound: SoundRef,
    /// The `SoundSource` id (master, music, ... player).
    pub source: i32,
    /// Position in 1/8 of a block.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

impl Wrapper for SoundEffect {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::SOUND_EFFECT);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(SoundEffect {
            sound: SoundRef::read(r)?,
            source: r.varint()?,
            x: r.i32()?,
            y: r.i32()?,
            z: r.i32()?,
            volume: r.f32()?,
            pitch: r.f32()?,
            seed: r.i64()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        self.sound.write(w);
        w.varint(self.source);
        w.i32(self.x);
        w.i32(self.y);
        w.i32(self.z);
        w.f32(self.volume);
        w.f32(self.pitch);
        w.i64(self.seed);
    }
}

/// A sound that follows an entity (`SoundEntity`).
#[derive(Clone, Debug, PartialEq)]
pub struct EntitySoundEffect {
    pub sound: SoundRef,
    pub source: i32,
    pub entity_id: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

impl Wrapper for EntitySoundEffect {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::ENTITY_SOUND_EFFECT);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(EntitySoundEffect {
            sound: SoundRef::read(r)?,
            source: r.varint()?,
            entity_id: r.varint()?,
            volume: r.f32()?,
            pitch: r.f32()?,
            seed: r.i64()?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        self.sound.write(w);
        w.varint(self.source);
        w.varint(self.entity_id);
        w.f32(self.volume);
        w.f32(self.pitch);
        w.i64(self.seed);
    }
}

/// The server's keep alive (`KeepAlive`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeepAlive {
    pub id: i64,
}

impl Wrapper for KeepAlive {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::KEEP_ALIVE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(KeepAlive { id: r.i64()? })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.i64(self.id);
    }
}

/// A plugin channel message from the server (`CustomPayload`): a channel and its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl Wrapper for PluginMessage {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::PLUGIN_MESSAGE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(PluginMessage {
            channel: r.identifier()?,
            data: r.rest().to_vec(),
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.identifier(&self.channel);
        w.raw(&self.data);
    }
}

/// A resource pack the client should load (`ResourcePackPush`).
#[derive(Clone, Debug, PartialEq)]
pub struct ResourcePackSend {
    pub uuid: u128,
    pub url: String,
    /// The SHA-1 of the pack, 40 hex characters (or empty).
    pub hash: String,
    pub required: bool,
    pub prompt: Option<Tag>,
}

impl Wrapper for ResourcePackSend {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::RESOURCE_PACK_SEND);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ResourcePackSend {
            uuid: r.uuid()?,
            url: r.string(32767)?,
            hash: r.string(40)?,
            required: r.bool()?,
            prompt: read_opt(r, |r| r.nbt())?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        w.uuid(self.uuid);
        w.string(&self.url);
        w.string(&self.hash);
        w.bool(self.required);
        write_opt(w, &self.prompt, |w, t| w.nbt(t));
    }
}

/// Removes one resource pack, or all of them when `uuid` is none (`ResourcePackPop`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePackRemove {
    pub uuid: Option<u128>,
}

impl Wrapper for ResourcePackRemove {
    const TYPE: PacketType = PacketType::PlayServer(PlayServer::RESOURCE_PACK_REMOVE);

    fn read_fields(r: &mut Reader, _reg: &dyn Registry) -> Result<Self> {
        Ok(ResourcePackRemove {
            uuid: read_opt(r, |r| r.uuid())?,
        })
    }

    fn write_fields(&self, w: &mut Writer) {
        write_opt(w, &self.uuid, |w, u| w.uuid(*u));
    }
}
