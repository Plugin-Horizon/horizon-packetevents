//! Tests of the engine-free packet API: codec, NBT, the packet tables, every wrapper round trip,
//! and the listener dispatch.

use super::codec::{
    BasicRegistry, Error, Reader, Registry, Writer, basic_item_len, pack_block_pos,
    pack_section_pos, unpack_block_pos, unpack_section_pos,
};
use super::event::{PacketEvent, PacketListener, Priority, User};
use super::manager::{Outcome, PacketEvents};
use super::nbt::Tag;
use super::packet_type::{
    ConnectionState, LoginClient, LoginServer, PacketType, PlayClient, PlayServer,
};
use super::wrapper::clientbound::*;
use super::wrapper::serverbound as sb;
use super::wrapper::serverbound::{
    Animation, AttackEntity, BlockPlace, ClientSettings, HeldItemChange, InteractEntity,
    PlayerDigging, PlayerFlying, PlayerPosition, PlayerPositionAndRotation, PlayerRotation,
    ResourcePackStatus, TabComplete,
};
use super::wrapper::{EntityDataEntry, RawItem, RawParticle, Wrapper};
use std::sync::{Arc, Mutex};

const REG: BasicRegistry = BasicRegistry;

/// A registry that knows one particle without payload (id 7) and one entity serializer (50, a
/// two-byte value), so the registry-dependent paths run.
struct TestRegistry;

impl Registry for TestRegistry {
    fn item_len(&self, data: &[u8]) -> super::codec::Result<usize> {
        // Items with components: the count, id, added, removed, then one byte per added component
        // (a test-only rule).
        let mut r = Reader::new(data);
        let count = r.varint()?;
        if count <= 0 {
            return Ok(r.position());
        }
        r.varint()?;
        let added = r.varint()?;
        let removed = r.varint()?;
        if removed != 0 {
            return Err(Error::new(
                "test registry: removed components are not modelled",
            ));
        }
        r.skip(added as usize)?;
        Ok(r.position())
    }

    fn particle_len(&self, data: &[u8]) -> super::codec::Result<usize> {
        let mut r = Reader::new(data);
        match r.varint()? {
            7 => Ok(r.position()),
            other => Err(Error::new(format!("test registry: particle {other}"))),
        }
    }

    fn entity_value_len(&self, serializer: i32, data: &[u8]) -> super::codec::Result<usize> {
        if serializer == 50 {
            let mut r = Reader::new(data);
            r.skip(2)?;
            return Ok(r.position());
        }
        super::codec::fixed_entity_value_len(serializer, data)
    }
}

/// A vector as the wire keeps it (LpVec3 is quantised).
fn quantise(v: [f64; 3]) -> [f64; 3] {
    let mut w = Writer::new();
    w.lp_vec3(v);
    Reader::new(&w.data).lp_vec3().unwrap()
}

fn user() -> User {
    User {
        uuid: 0x1234_5678_9abc_def0_0fed_cba9_8765_4321,
        name: "player".into(),
        state: ConnectionState::Play,
    }
}

fn tag_text(s: &str) -> Tag {
    Tag::plain(s)
}

fn nbt_compound() -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String("hi".into())),
        ("color".into(), Tag::String("red".into())),
        ("bold".into(), Tag::Byte(1)),
        (
            "extra".into(),
            Tag::List(
                10,
                vec![Tag::Compound(vec![(
                    "text".into(),
                    Tag::String("x".into()),
                )])],
            ),
        ),
        ("ints".into(), Tag::IntArray(vec![1, -2, 3])),
        ("longs".into(), Tag::LongArray(vec![i64::MIN, 0, i64::MAX])),
        ("bytes".into(), Tag::ByteArray(vec![-1, 0, 127])),
        ("short".into(), Tag::Short(-300)),
        ("float".into(), Tag::Float(1.5)),
        ("double".into(), Tag::Double(-0.25)),
        ("long".into(), Tag::Long(1 << 40)),
    ])
}

// Codec ----------------------------------------------------------------------------------------

#[test]
fn varint_round_trips_the_edges() {
    for v in [
        0,
        1,
        127,
        128,
        255,
        25_565,
        2_097_151,
        2_147_483_647,
        -1,
        -2_147_483_648,
    ] {
        let mut w = Writer::new();
        w.varint(v);
        let mut r = Reader::new(&w.data);
        assert_eq!(r.varint().unwrap(), v, "varint {v}");
        r.finish().unwrap();
    }
}

#[test]
fn varint_bytes_match_the_protocol_examples() {
    let mut w = Writer::new();
    w.varint(-1);
    assert_eq!(w.data, [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    let mut w = Writer::new();
    w.varint(300);
    assert_eq!(w.data, [0xAC, 0x02]);
}

#[test]
fn varint_longer_than_five_bytes_is_an_error() {
    let mut r = Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01]);
    assert!(r.varint().is_err());
}

#[test]
fn varlong_round_trips() {
    for v in [0i64, 1, 300, 1 << 35, i64::MAX, -1, i64::MIN] {
        let mut w = Writer::new();
        w.varlong(v);
        assert_eq!(Reader::new(&w.data).varlong().unwrap(), v);
    }
}

#[test]
fn numbers_are_big_endian() {
    let mut w = Writer::new();
    w.i16(-2);
    w.i32(0x0102_0304);
    w.i64(-1);
    w.f32(1.5);
    w.f64(-2.25);
    assert_eq!(&w.data[..2], [0xFF, 0xFE]);
    assert_eq!(&w.data[2..6], [1, 2, 3, 4]);
    let mut r = Reader::new(&w.data);
    assert_eq!(r.i16().unwrap(), -2);
    assert_eq!(r.i32().unwrap(), 0x0102_0304);
    assert_eq!(r.i64().unwrap(), -1);
    assert_eq!(r.f32().unwrap(), 1.5);
    assert_eq!(r.f64().unwrap(), -2.25);
    r.finish().unwrap();
}

#[test]
fn booleans_reject_other_bytes() {
    assert!(Reader::new(&[2]).bool().is_err());
    assert!(!Reader::new(&[0]).bool().unwrap());
}

#[test]
fn uuid_round_trips_both_halves() {
    let id: u128 = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff;
    let mut w = Writer::new();
    w.uuid(id);
    assert_eq!(w.data[0], 0x00);
    assert_eq!(w.data[8], 0x88);
    assert_eq!(Reader::new(&w.data).uuid().unwrap(), id);
}

#[test]
fn strings_are_length_prefixed_and_limited() {
    let mut w = Writer::new();
    w.string("héllo");
    assert_eq!(Reader::new(&w.data).string(16).unwrap(), "héllo");
    // Five characters, but a limit of four.
    assert!(Reader::new(&w.data).string(4).is_err());
    // Bytes that are not UTF-8.
    assert!(Reader::new(&[1, 0xFF]).string(8).is_err());
}

#[test]
fn a_string_cut_short_is_an_error() {
    assert!(Reader::new(&[5, b'a']).string(8).is_err());
}

#[test]
fn block_positions_pack_as_the_game_does() {
    // BlockPos.asLong(1, 2, 3): x in the top bits, then z, then y in the low 12 bits.
    assert_eq!(pack_block_pos([1, 2, 3]), (1i64 << 38) | (3i64 << 12) | 2);
    for p in [
        [0, 0, 0],
        [-1, -64, -1],
        [30_000_000, 2047, -30_000_000],
        [-30_000_000, -2048, 5],
    ] {
        let packed = pack_block_pos(p);
        assert_eq!(unpack_block_pos(packed), p, "position {p:?}");
    }
}

#[test]
fn section_positions_pack_as_the_game_does() {
    for p in [[0, 0, 0], [-5, -4, 1000], [2_097_151, -2048, -2_097_152]] {
        assert_eq!(unpack_section_pos(pack_section_pos(p)), p, "section {p:?}");
    }
}

#[test]
fn lp_vec3_zero_is_one_byte() {
    let mut w = Writer::new();
    w.lp_vec3([0.0, 0.0, 0.0]);
    assert_eq!(w.data, [0]);
    assert_eq!(Reader::new(&w.data).lp_vec3().unwrap(), [0.0; 3]);
}

#[test]
fn lp_vec3_keeps_direction_and_magnitude_within_quantisation() {
    for v in [
        [0.5, -0.25, 0.1],
        [1.0, 0.0, 0.0],
        [12.5, -7.0, 3.25],
        [-2000.0, 1000.0, 0.5],
    ] {
        let mut w = Writer::new();
        w.lp_vec3(v);
        let back = Reader::new(&w.data).lp_vec3().unwrap();
        let scale = v.iter().fold(0.0f64, |m, c| m.max(c.abs())).ceil();
        for i in 0..3 {
            assert!(
                (back[i] - v[i]).abs() <= scale / 16_000.0 + 1e-9,
                "axis {i}: {} vs {}",
                back[i],
                v[i]
            );
        }
    }
}

#[test]
fn lp_vec3_small_values_take_one_extra_scale_byte_only_when_big() {
    let mut small = Writer::new();
    small.lp_vec3([0.5, 0.5, 0.5]);
    assert_eq!(small.data.len(), 6);
    let mut big = Writer::new();
    big.lp_vec3([100.0, 0.0, 0.0]);
    assert_eq!(big.data.len(), 7, "scale above 3 needs the VarInt tail");
}

#[test]
fn nbt_round_trips_every_tag_kind() {
    let tag = nbt_compound();
    let mut out = Vec::new();
    tag.write_network(&mut out);
    let (back, used) = Tag::read_network(&out).unwrap();
    assert_eq!(used, out.len());
    assert_eq!(back, tag);
}

#[test]
fn nbt_strings_use_modified_utf8() {
    // NUL is written as C0 80, and a character past U+FFFF as a surrogate pair.
    let tag = Tag::String("a\u{0}😀".into());
    let mut out = Vec::new();
    tag.write_network(&mut out);
    assert_eq!(&out[1..3], [0, 9], "length counts the modified encoding");
    assert_eq!(&out[3..6], [b'a', 0xC0, 0x80]);
    let (back, _) = Tag::read_network(&out).unwrap();
    assert_eq!(back, tag);
}

#[test]
fn nbt_plain_text_is_a_string_root() {
    let mut out = Vec::new();
    tag_text("Welcome").write_network(&mut out);
    assert_eq!(out[0], 8, "string tag id");
    let (tag, _) = Tag::read_network(&out).unwrap();
    assert_eq!(tag.plain_text(), Some("Welcome"));
}

#[test]
fn nbt_plain_text_reads_a_text_compound() {
    let tag = Tag::Compound(vec![("text".into(), Tag::String("x".into()))]);
    assert_eq!(tag.plain_text(), Some("x"));
    let styled = Tag::Compound(vec![
        ("text".into(), Tag::String("x".into())),
        ("bold".into(), Tag::Byte(1)),
    ]);
    assert_eq!(styled.plain_text(), None);
}

#[test]
fn nbt_rejects_cut_short_and_unknown_tags() {
    assert!(Tag::read_network(&[3, 0, 0]).is_err());
    assert!(Tag::read_network(&[99]).is_err());
    assert!(Tag::read_network(&[10]).is_err());
}

#[test]
fn nbt_nesting_is_limited() {
    let mut deep = Vec::new();
    for _ in 0..600 {
        deep.push(9); // list
        deep.push(9); // of lists
        deep.extend_from_slice(&1i32.to_be_bytes());
    }
    deep.extend_from_slice(&[0, 0, 0, 0]);
    assert!(Tag::read_network(&deep).is_err());
}

#[test]
fn reader_nbt_consumes_exactly_one_value() {
    let mut out = Vec::new();
    Tag::Int(7).write_network(&mut out);
    out.push(0xAA);
    let mut r = Reader::new(&out);
    assert_eq!(r.nbt().unwrap(), Tag::Int(7));
    assert_eq!(r.remaining(), 1);
}

// Registry -------------------------------------------------------------------------------------

#[test]
fn basic_item_slots_need_no_components() {
    assert_eq!(basic_item_len(&[0]).unwrap(), 1);
    assert_eq!(basic_item_len(&RawItem::simple(3, 17).0).unwrap(), 4);
    // A stack with one added component needs the item codec.
    let mut w = Writer::new();
    w.varint(1);
    w.varint(17);
    w.varint(1);
    w.varint(0);
    assert!(basic_item_len(&w.data).is_err());
}

#[test]
fn particles_without_payload_or_with_a_block_state_or_colour_are_measured() {
    let reg = BasicRegistry;
    // minecraft:block (1) with state 300 (two bytes).
    let mut w = Writer::new();
    w.varint(1);
    w.varint(300);
    assert_eq!(reg.particle_len(&w.data).unwrap(), w.data.len());
    // minecraft:entity_effect (28) with an ARGB int.
    let mut w = Writer::new();
    w.varint(28);
    w.i32(0x80FF_0000u32 as i32);
    assert_eq!(reg.particle_len(&w.data).unwrap(), w.data.len());
    // Unknown particles cannot be measured without the registry.
    assert!(reg.particle_len(&[99]).is_err());
}

#[test]
fn fixed_entity_serializers_are_measured() {
    let reg = BasicRegistry;
    assert_eq!(reg.entity_value_len(0, &[5]).unwrap(), 1); // byte
    assert_eq!(reg.entity_value_len(3, &[0; 4]).unwrap(), 4); // float
    assert_eq!(reg.entity_value_len(9, &[0; 12]).unwrap(), 12); // rotations
    assert_eq!(reg.entity_value_len(11, &[0]).unwrap(), 1); // absent optional block pos
    assert_eq!(
        reg.entity_value_len(11, &[1, 0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap(),
        9
    );
    assert!(reg.entity_value_len(7, &[0]).is_err()); // item stacks need the registry
}

// Packet tables --------------------------------------------------------------------------------

#[test]
fn packet_ids_match_the_26_3_protocol() {
    assert_eq!(PlayServer::CHAT_MESSAGE.id(), 66);
    assert_eq!(PlayServer::SYSTEM_CHAT_MESSAGE.id(), 124);
    assert_eq!(PlayServer::KEEP_ALIVE.id(), 45);
    assert_eq!(PlayServer::SET_SLOT.id(), 20);
    assert_eq!(PlayServer::WINDOW_ITEMS.id(), 18);
    assert_eq!(PlayServer::SPAWN_ENTITY.id(), 1);
    assert_eq!(PlayServer::ENTITY_TELEPORT.id(), 128);
    assert_eq!(PlayServer::BOSS_BAR.id(), 9);
    assert_eq!(PlayServer::PLAYER_INFO_UPDATE.id(), 71);
    assert_eq!(PlayClient::PLAYER_BLOCK_PLACEMENT.id(), 66);
    assert_eq!(PlayClient::INTERACT_ENTITY.id(), 26);
    assert_eq!(PlayClient::KEEP_ALIVE.id(), 28);
    assert_eq!(PlayClient::PUNCH.id(), 46);
}

#[test]
fn every_table_entry_round_trips_its_id_and_name() {
    for p in PlayServer::ALL {
        assert_eq!(PlayServer::from_id(p.id()), Some(*p));
        assert!(!p.name().is_empty());
    }
    for p in PlayClient::ALL {
        assert_eq!(PlayClient::from_id(p.id()), Some(*p));
    }
    assert_eq!(PlayServer::ALL.len(), 144);
    assert_eq!(PlayClient::ALL.len(), 69);
    assert_eq!(PlayServer::from_id(144), None);
    assert_eq!(PlayServer::from_id(-1), None);
}

#[test]
fn ids_are_dense_from_zero() {
    for (i, p) in PlayServer::ALL.iter().enumerate() {
        assert_eq!(p.id() as usize, i);
    }
    for (i, p) in PlayClient::ALL.iter().enumerate() {
        assert_eq!(p.id() as usize, i);
    }
}

#[test]
fn login_and_configuration_tables_match_horizon() {
    assert_eq!(LoginServer::from_id(1), Some(LoginServer::HELLO));
    assert!(LoginClient::from_id(0).is_some());
    assert!(
        LoginServer::ALL
            .iter()
            .all(|p| p.name() == p.name().to_lowercase())
    );
}

#[test]
fn packet_type_lookup_uses_state_and_side() {
    assert_eq!(
        PacketType::lookup(ConnectionState::Play, true, 66),
        Some(PacketType::PlayServer(PlayServer::CHAT_MESSAGE))
    );
    assert_eq!(
        PacketType::lookup(ConnectionState::Play, false, 66),
        Some(PacketType::PlayClient(PlayClient::PLAYER_BLOCK_PLACEMENT))
    );
    assert_eq!(
        PacketType::lookup(ConnectionState::Handshaking, true, 0),
        None
    );
    assert!(PacketType::PlayServer(PlayServer::KEEP_ALIVE).is_server_side());
    assert!(!PacketType::PlayClient(PlayClient::KEEP_ALIVE).is_server_side());
    assert_eq!(
        PacketType::PlayClient(PlayClient::KEEP_ALIVE).state(),
        ConnectionState::Play
    );
}

// Wrappers: round trips ------------------------------------------------------------------------

/// Writes a wrapper, reads it back with the test registry and checks it is equal.
fn round_trip<W: Wrapper + PartialEq + std::fmt::Debug>(value: W) {
    let body = value.to_body();
    let back =
        W::from_body(&body, &TestRegistry).unwrap_or_else(|e| panic!("{}: {e}", W::TYPE.name()));
    assert_eq!(back, value, "{}", W::TYPE.name());
    assert_eq!(
        back.to_body(),
        body,
        "{} re-encodes to the same bytes",
        W::TYPE.name()
    );
}

fn sample_chat() -> ChatMessage {
    ChatMessage {
        global_index: 4,
        sender: 77,
        index: 2,
        signature: Some(vec![9; 256]),
        content: "hello".into(),
        timestamp: 1_700_000_000_000,
        salt: -5,
        last_seen: vec![(0, None), (-1, Some(vec![3; 256])), (7, None)],
        unsigned_content: Some(tag_text("hello!")),
        filter: (2, vec![0b101, 0]),
        chat_type: 1,
        chat_type_name: Tag::String("chat.type.text".into()),
        chat_type_target: None,
    }
}

#[test]
fn chat_message_round_trips_with_signatures_and_filters() {
    round_trip(sample_chat());
    let mut unsigned = sample_chat();
    unsigned.signature = None;
    unsigned.unsigned_content = None;
    unsigned.filter = (0, Vec::new());
    unsigned.last_seen.clear();
    unsigned.chat_type_target = Some(nbt_compound());
    round_trip(unsigned);
}

#[test]
fn system_chat_round_trips_both_targets() {
    round_trip(SystemChatMessage {
        content: nbt_compound(),
        overlay: false,
    });
    round_trip(SystemChatMessage {
        content: tag_text("bar"),
        overlay: true,
    });
}

#[test]
fn title_wrappers_round_trip() {
    round_trip(SetTitleText {
        text: tag_text("Title"),
    });
    round_trip(SetTitleSubtitle {
        text: nbt_compound(),
    });
    round_trip(SetTitleTimes {
        fade_in: 10,
        stay: 70,
        fade_out: -1,
    });
    round_trip(ClearTitles { reset: true });
    round_trip(ClearTitles { reset: false });
}

#[test]
fn action_bar_round_trips() {
    round_trip(ActionBar {
        text: tag_text("Health 20"),
    });
}

#[test]
fn boss_bar_round_trips_every_action() {
    let id = 0xDEAD_BEEF_u128;
    let actions = vec![
        BossBarAction::Add {
            name: tag_text("Boss"),
            progress: 0.5,
            color: 2,
            overlay: 1,
            darken_sky: true,
            play_music: false,
            create_fog: true,
        },
        BossBarAction::Remove,
        BossBarAction::UpdateProgress(0.25),
        BossBarAction::UpdateName(nbt_compound()),
        BossBarAction::UpdateStyle {
            color: 5,
            overlay: 0,
        },
        BossBarAction::UpdateFlags {
            darken_sky: false,
            play_music: true,
            create_fog: true,
        },
    ];
    for action in actions {
        round_trip(BossBar { uuid: id, action });
    }
}

#[test]
fn entity_metadata_round_trips_known_serializers_and_a_registry_one() {
    let values = vec![
        EntityDataEntry {
            index: 0,
            serializer: 0,
            value: vec![0x40],
        },
        EntityDataEntry {
            index: 1,
            serializer: 1,
            value: vec![0x80, 0x01],
        },
        EntityDataEntry {
            index: 2,
            serializer: 3,
            value: 1.5f32.to_be_bytes().to_vec(),
        },
        EntityDataEntry {
            index: 3,
            serializer: 8,
            value: vec![1],
        },
        EntityDataEntry {
            index: 4,
            serializer: 50,
            value: vec![0xAB, 0xCD],
        },
    ];
    round_trip(EntityMetadata {
        entity_id: 12,
        values: values.clone(),
    });
    let entry = &values[1];
    assert_eq!(entry.as_varint(), Some(128));
    assert_eq!(values[0].as_byte(), Some(0x40));
    assert_eq!(values[2].as_float(), Some(1.5));
    assert_eq!(values[3].as_bool(), Some(true));
}

#[test]
fn entity_metadata_without_values_is_just_the_end_marker() {
    round_trip(EntityMetadata {
        entity_id: 3,
        values: Vec::new(),
    });
    let body = EntityMetadata {
        entity_id: 3,
        values: Vec::new(),
    }
    .to_body();
    assert_eq!(body, [3, 0xFF]);
}

#[test]
fn entity_metadata_of_an_unknown_serializer_is_an_error() {
    let body = [1u8, 9, 99, 0];
    assert!(EntityMetadata::from_body(&body, &REG).is_err());
    assert!(EntityMetadata::from_body(&body, &TestRegistry).is_err());
}

#[test]
fn spawn_entity_round_trips() {
    round_trip(SpawnEntity {
        entity_id: 9,
        uuid: 42,
        kind: 100,
        position: [1.5, 64.0, -3.25],
        velocity: quantise([0.1, 0.0, -0.2]),
        pitch: 10,
        yaw: -90,
        head_yaw: 45,
        data: 3,
    });
}

#[test]
fn entity_teleport_round_trips() {
    round_trip(EntityTeleport {
        entity_id: 5,
        position: [10.0, 70.0, 10.0],
        delta: [0.0, 0.0, 0.0],
        yaw: 90.0,
        pitch: -15.5,
        relatives: 0,
        on_ground: true,
    });
}

#[test]
fn block_change_round_trips() {
    round_trip(BlockChange {
        position: [-100, 64, 200],
        state: 1234,
    });
}

#[test]
fn multi_block_change_decodes_its_entries() {
    let section = [-1, 4, 2];
    let mc = MultiBlockChange {
        section,
        changes: vec![(7 << 12) | (5 << 8 | 3 << 4 | 2), (9 << 12) | 0xFFF],
    };
    round_trip(mc.clone());
    let (state, pos) = mc.decode_entry(mc.changes[0]);
    assert_eq!(state, 7);
    assert_eq!(pos, [-16 + 5, 64 + 2, 32 + 3]);
    let (state, pos) = mc.decode_entry(mc.changes[1]);
    assert_eq!((state, pos), (9, [-16 + 15, 64 + 15, 32 + 15]));
}

fn sample_entry(uuid: u128) -> PlayerInfoEntry {
    PlayerInfoEntry {
        uuid,
        name: Some("name".into()),
        properties: vec![Property {
            name: "textures".into(),
            value: "e30=".into(),
            signature: Some("sig".into()),
        }],
        chat_session: Some((5, 99, vec![1, 2], vec![3])),
        game_mode: Some(1),
        listed: Some(true),
        latency: Some(42),
        display_name: Some(Some(tag_text("Shown"))),
        list_order: Some(-3),
        show_hat: Some(false),
    }
}

#[test]
fn player_info_update_round_trips_every_action() {
    round_trip(PlayerInfoUpdate {
        actions: 255,
        entries: vec![sample_entry(1), sample_entry(2)],
    });
    // Only latency: the other fields stay absent.
    let latency = PlayerInfoEntry {
        uuid: 3,
        latency: Some(20),
        ..PlayerInfoEntry::default()
    };
    round_trip(PlayerInfoUpdate {
        actions: 16,
        entries: vec![latency],
    });
    // A display name cleared.
    let cleared = PlayerInfoEntry {
        uuid: 4,
        display_name: Some(None),
        ..PlayerInfoEntry::default()
    };
    round_trip(PlayerInfoUpdate {
        actions: 32,
        entries: vec![cleared],
    });
}

#[test]
fn player_info_remove_round_trips() {
    round_trip(PlayerInfoRemove {
        uuids: vec![1, 2, u128::MAX],
    });
    round_trip(PlayerInfoRemove { uuids: Vec::new() });
}

#[test]
fn scoreboard_objective_round_trips_each_method() {
    round_trip(ScoreboardObjective {
        name: "sb".into(),
        method: 1,
        display: None,
    });
    round_trip(ScoreboardObjective {
        name: "sb".into(),
        method: 0,
        display: Some((tag_text("Score"), 1, Some(NumberFormat::Blank))),
    });
    round_trip(ScoreboardObjective {
        name: "sb".into(),
        method: 2,
        display: Some((nbt_compound(), 0, Some(NumberFormat::Fixed(tag_text("x"))))),
    });
}

#[test]
fn update_score_round_trips() {
    round_trip(UpdateScore {
        owner: "Steve".into(),
        objective: "kills".into(),
        score: -4,
        display: None,
        number_format: None,
    });
    round_trip(UpdateScore {
        owner: "Steve".into(),
        objective: "kills".into(),
        score: 12,
        display: Some(tag_text("12")),
        number_format: Some(NumberFormat::Styled(nbt_compound())),
    });
}

#[test]
fn teams_round_trip_each_method() {
    let params = TeamParameters {
        display_name: tag_text("Red"),
        prefix: tag_text("[R] "),
        suffix: Tag::End,
        name_tag_visibility: 1,
        collision_rule: 0,
        color: Some(12),
        options: 3,
    };
    round_trip(Teams {
        name: "red".into(),
        method: 0,
        parameters: Some(params.clone()),
        players: vec!["a".into(), "b".into()],
    });
    round_trip(Teams {
        name: "red".into(),
        method: 1,
        parameters: None,
        players: Vec::new(),
    });
    round_trip(Teams {
        name: "red".into(),
        method: 2,
        parameters: Some(TeamParameters {
            color: None,
            ..params
        }),
        players: Vec::new(),
    });
    round_trip(Teams {
        name: "red".into(),
        method: 3,
        parameters: None,
        players: vec!["c".into()],
    });
    round_trip(Teams {
        name: "red".into(),
        method: 4,
        parameters: None,
        players: vec!["c".into()],
    });
}

#[test]
fn set_slot_round_trips_items_and_keeps_their_bytes() {
    round_trip(SetSlot {
        window_id: 0,
        state_id: 5,
        slot: 36,
        item: RawItem::empty(),
    });
    round_trip(SetSlot {
        window_id: 2,
        state_id: 1,
        slot: 0,
        item: RawItem::simple(64, 1000),
    });
    // A stack with a registry-sized component: the test registry reads one byte for it.
    let mut w = Writer::new();
    w.varint(3);
    w.varint(77);
    w.varint(1);
    w.varint(0);
    w.u8(0xEE);
    let item = RawItem(w.data.clone());
    round_trip(SetSlot {
        window_id: 1,
        state_id: 2,
        slot: 3,
        item: item.clone(),
    });
    assert_eq!(item.count(), Some(3));
    assert!(!item.is_empty());
    assert!(RawItem::empty().is_empty());
}

#[test]
fn set_slot_with_components_needs_a_registry_that_measures_them() {
    let mut w = Writer::new();
    w.varint(0);
    w.varint(1);
    w.i16(2);
    w.varint(3);
    w.varint(77);
    w.varint(1);
    w.varint(0);
    w.u8(0);
    let body = w.data;
    assert!(SetSlot::from_body(&body, &REG).is_err());
    assert!(SetSlot::from_body(&body, &TestRegistry).is_ok());
}

#[test]
fn window_items_round_trip_with_the_cursor_item() {
    round_trip(WindowItems {
        window_id: 0,
        state_id: 9,
        items: vec![
            RawItem::empty(),
            RawItem::simple(1, 1),
            RawItem::simple(16, 2),
        ],
        carried: RawItem::simple(2, 3),
    });
    round_trip(WindowItems {
        window_id: 3,
        state_id: 0,
        items: Vec::new(),
        carried: RawItem::empty(),
    });
}

#[test]
fn open_window_round_trips() {
    round_trip(OpenWindow {
        window_id: 4,
        menu: 9,
        title: nbt_compound(),
    });
}

#[test]
fn particle_round_trips_with_a_registry_payload() {
    let mut particle = Writer::new();
    particle.varint(7);
    round_trip(Particle {
        particle: RawParticle(particle.data.clone()),
        override_limiter: true,
        always_show: false,
        position: [0.5, 65.5, 0.5],
        distance: [0.2, 0.2, 0.2],
        max_speed: [0.0, 0.1, 0.0],
        count: 10,
        randomization: 1,
    });
    let mut block = Writer::new();
    block.varint(1);
    block.varint(300);
    let body = Particle {
        particle: RawParticle(block.data.clone()),
        override_limiter: false,
        always_show: true,
        position: [1.0, 2.0, 3.0],
        distance: [0.0; 3],
        max_speed: [0.0; 3],
        count: 1,
        randomization: 0,
    }
    .to_body();
    let back = Particle::from_body(&body, &REG).unwrap();
    assert_eq!(back.particle.id(), Some(1));
    assert_eq!(back.particle.0, block.data);
}

#[test]
fn particle_with_an_unknown_type_fails_without_the_registry() {
    let body = Particle {
        particle: RawParticle({
            let mut w = Writer::new();
            w.varint(7);
            w.data
        }),
        override_limiter: false,
        always_show: false,
        position: [0.0; 3],
        distance: [0.0; 3],
        max_speed: [0.0; 3],
        count: 0,
        randomization: 0,
    }
    .to_body();
    assert!(Particle::from_body(&body, &REG).is_err());
}

#[test]
fn sound_effect_round_trips_registry_and_inline_sounds() {
    round_trip(SoundEffect {
        sound: SoundRef::Registry(12),
        source: 2,
        x: 8,
        y: -16,
        z: 0,
        volume: 1.0,
        pitch: 0.8,
        seed: 99,
    });
    round_trip(SoundEffect {
        sound: SoundRef::Inline {
            name: "minecraft:ambient.cave".into(),
            range: Some(16.0),
        },
        source: 0,
        x: 0,
        y: 0,
        z: 0,
        volume: 0.5,
        pitch: 1.0,
        seed: -1,
    });
    round_trip(EntitySoundEffect {
        sound: SoundRef::Inline {
            name: "mod:thing".into(),
            range: None,
        },
        source: 1,
        entity_id: 44,
        volume: 2.0,
        pitch: 1.5,
        seed: 0,
    });
    round_trip(EntitySoundEffect {
        sound: SoundRef::Registry(0),
        source: 4,
        entity_id: 1,
        volume: 1.0,
        pitch: 1.0,
        seed: 3,
    });
}

#[test]
fn keep_alive_round_trips_and_is_eight_bytes() {
    round_trip(KeepAlive {
        id: 1_234_567_890_123,
    });
    assert_eq!(KeepAlive { id: 1 }.to_body(), [0, 0, 0, 0, 0, 0, 0, 1]);
    round_trip(sb::KeepAlive { id: -9 });
}

#[test]
fn plugin_message_keeps_the_data_to_the_end() {
    round_trip(PluginMessage {
        channel: "minecraft:brand".into(),
        data: b"vanilla".to_vec(),
    });
    round_trip(PluginMessage {
        channel: "voicechat:x".into(),
        data: Vec::new(),
    });
    round_trip(sb::PluginMessage {
        channel: "a:b".into(),
        data: vec![0xFF; 10],
    });
}

#[test]
fn resource_packs_round_trip() {
    round_trip(ResourcePackSend {
        uuid: 1,
        url: "https://example.invalid/pack.zip".into(),
        hash: "0123456789abcdef0123456789abcdef01234567".into(),
        required: true,
        prompt: Some(tag_text("Please load")),
    });
    round_trip(ResourcePackRemove { uuid: Some(1) });
    round_trip(ResourcePackRemove { uuid: None });
    round_trip(ResourcePackStatus { uuid: 1, action: 3 });
}

#[test]
fn interact_entity_round_trips_its_point() {
    round_trip(InteractEntity {
        entity_id: 9,
        hand: 0,
        location: [0.0, 0.0, 0.0],
        sneaking: false,
    });
    round_trip(InteractEntity {
        entity_id: 9,
        hand: 1,
        location: quantise([0.25, 1.0, -0.5]),
        sneaking: true,
    });
}

#[test]
fn attack_entity_round_trips() {
    round_trip(AttackEntity { entity_id: 300 });
}

#[test]
fn player_movement_round_trips() {
    round_trip(PlayerPosition {
        position: [1.0, 2.0, 3.0],
        flags: 1,
    });
    round_trip(PlayerPositionAndRotation {
        position: [-1.5, 64.0, 0.25],
        yaw: 90.0,
        pitch: -10.0,
        flags: 3,
    });
    round_trip(PlayerRotation {
        yaw: 180.0,
        pitch: 0.0,
        flags: 0,
    });
    round_trip(PlayerFlying { flags: 2 });
}

#[test]
fn player_digging_round_trips() {
    round_trip(PlayerDigging {
        action: 0,
        position: [10, 64, -10],
        direction: 1,
        sequence: 7,
    });
}

#[test]
fn block_place_round_trips() {
    round_trip(BlockPlace {
        hand: 0,
        position: [1, 2, 3],
        face: 1,
        cursor: [0.5, 1.0, 0.25],
        inside: false,
        world_border_hit: true,
        sequence: 12,
    });
}

#[test]
fn held_item_change_round_trips() {
    round_trip(HeldItemChange { slot: 8 });
    round_trip(HeldItemChange { slot: -1 });
}

#[test]
fn animation_is_empty() {
    round_trip(Animation);
    assert!(Animation.to_body().is_empty());
    assert!(Animation::from_body(&[1], &REG).is_err());
}

#[test]
fn client_settings_round_trip() {
    round_trip(ClientSettings {
        locale: "en_us".into(),
        view_distance: 12,
        chat_mode: 0,
        chat_colors: true,
        displayed_skin_parts: 0x7F,
        main_hand: 1,
        text_filtering: false,
        allow_server_listings: true,
        particle_status: 2,
    });
}

#[test]
fn tab_complete_round_trips() {
    round_trip(TabComplete {
        id: 4,
        text: "/give @p ".into(),
    });
}

// Wrappers: whole-body checks ------------------------------------------------------------------

#[test]
fn a_body_with_bytes_left_over_is_rejected() {
    let mut body = KeepAlive { id: 1 }.to_body();
    body.push(0);
    assert!(KeepAlive::from_body(&body, &REG).is_err());
}

#[test]
fn a_truncated_body_is_rejected() {
    let body = BlockChange {
        position: [1, 2, 3],
        state: 5,
    }
    .to_body();
    assert!(BlockChange::from_body(&body[..body.len() - 1], &REG).is_err());
}

#[test]
fn block_change_matches_the_known_bytes() {
    let body = BlockChange {
        position: [1, 2, 3],
        state: 1,
    }
    .to_body();
    assert_eq!(body, [0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x30, 0x02, 0x01]);
}

#[test]
fn the_wrapper_type_is_checked_against_the_event() {
    let event = PacketEvent::new(
        PacketType::PlayServer(PlayServer::KEEP_ALIVE),
        user(),
        KeepAlive { id: 5 }.to_body(),
    );
    assert_eq!(event.read::<KeepAlive>(&REG).unwrap(), KeepAlive { id: 5 });
    assert!(event.read::<ClearTitles>(&REG).is_err());
}

// Events and dispatch --------------------------------------------------------------------------

struct Recorder {
    name: &'static str,
    priority: Priority,
    log: Arc<Mutex<Vec<&'static str>>>,
    cancel: bool,
    replace_with: Option<i64>,
}

impl PacketListener for Recorder {
    fn priority(&self) -> Priority {
        self.priority
    }

    fn on_packet_send(&self, event: &mut PacketEvent) {
        self.log.lock().unwrap().push(self.name);
        if self.cancel {
            event.set_cancelled(true);
        }
        if let Some(id) = self.replace_with {
            let mut keep = event.read::<KeepAlive>(&REG).unwrap();
            keep.id = id;
            event.write(&keep).unwrap();
        }
    }

    fn on_packet_receive(&self, event: &mut PacketEvent) {
        self.log.lock().unwrap().push(self.name);
        event.set_cancelled(self.cancel);
    }
}

fn recorder(
    name: &'static str,
    priority: Priority,
    log: &Arc<Mutex<Vec<&'static str>>>,
) -> Arc<Recorder> {
    Arc::new(Recorder {
        name,
        priority,
        log: log.clone(),
        cancel: false,
        replace_with: None,
    })
}

fn keep_alive_body(id: i64) -> Vec<u8> {
    KeepAlive { id }.to_body()
}

#[test]
fn listeners_run_lowest_priority_first_and_monitor_last() {
    let api = PacketEvents::new(Box::new(BasicRegistry));
    let log = Arc::new(Mutex::new(Vec::new()));
    api.add_listener(recorder("high", Priority::High, &log));
    api.add_listener(recorder("monitor", Priority::Monitor, &log));
    api.add_listener(recorder("lowest", Priority::Lowest, &log));
    api.add_listener(recorder("normal-a", Priority::Normal, &log));
    api.add_listener(recorder("normal-b", Priority::Normal, &log));
    let outcome = api.dispatch_send(
        &user(),
        PacketType::PlayServer(PlayServer::KEEP_ALIVE),
        &keep_alive_body(1),
    );
    assert_eq!(outcome, Outcome::Unchanged);
    assert_eq!(
        *log.lock().unwrap(),
        ["lowest", "normal-a", "normal-b", "high", "monitor"]
    );
}

#[test]
fn a_cancelled_packet_still_reaches_every_listener() {
    let api = PacketEvents::new(Box::new(BasicRegistry));
    let log = Arc::new(Mutex::new(Vec::new()));
    api.add_listener(Arc::new(Recorder {
        name: "cancel",
        priority: Priority::Low,
        log: log.clone(),
        cancel: true,
        replace_with: None,
    }));
    api.add_listener(recorder("after", Priority::High, &log));
    let outcome = api.dispatch_send(
        &user(),
        PacketType::PlayServer(PlayServer::KEEP_ALIVE),
        &keep_alive_body(1),
    );
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(*log.lock().unwrap(), ["cancel", "after"]);
}

#[test]
fn a_changed_wrapper_is_sent_re_encoded() {
    let api = PacketEvents::new(Box::new(BasicRegistry));
    let log = Arc::new(Mutex::new(Vec::new()));
    api.add_listener(Arc::new(Recorder {
        name: "rewrite",
        priority: Priority::Normal,
        log: log.clone(),
        cancel: false,
        replace_with: Some(77),
    }));
    let outcome = api.dispatch_send(
        &user(),
        PacketType::PlayServer(PlayServer::KEEP_ALIVE),
        &keep_alive_body(1),
    );
    assert_eq!(outcome, Outcome::Changed(keep_alive_body(77)));
}

#[test]
fn a_body_changed_without_the_re_encode_mark_is_not_sent() {
    // A listener that changes the body without marking it: packetevents sends the original.
    struct Silent;
    impl PacketListener for Silent {
        fn on_packet_send(&self, event: &mut PacketEvent) {
            let mut changed = event.body().to_vec();
            changed[0] ^= 0xFF;
            event.set_body(changed);
            event.mark_for_reencode(false);
        }
    }
    let api = PacketEvents::new(Box::new(BasicRegistry));
    api.add_listener(Arc::new(Silent));
    assert_eq!(
        api.dispatch_send(
            &user(),
            PacketType::PlayServer(PlayServer::KEEP_ALIVE),
            &keep_alive_body(1)
        ),
        Outcome::Unchanged
    );
}

#[test]
fn set_body_marks_the_event_for_re_encoding() {
    let mut event = PacketEvent::new(
        PacketType::PlayClient(PlayClient::KEEP_ALIVE),
        user(),
        keep_alive_body(1),
    );
    assert!(!event.needs_reencode());
    event.set_body(keep_alive_body(2));
    assert!(event.needs_reencode());
    assert_eq!(event.body(), keep_alive_body(2));
}

#[test]
fn receive_listeners_see_client_packets() {
    let api = PacketEvents::new(Box::new(BasicRegistry));
    let log = Arc::new(Mutex::new(Vec::new()));
    api.add_listener(Arc::new(Recorder {
        name: "receive",
        priority: Priority::Normal,
        log: log.clone(),
        cancel: true,
        replace_with: None,
    }));
    let outcome = api.dispatch_receive(
        &user(),
        PacketType::PlayClient(PlayClient::KEEP_ALIVE),
        &keep_alive_body(1),
    );
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(*log.lock().unwrap(), ["receive"]);
}

#[test]
fn removed_listeners_stop_running() {
    let api = PacketEvents::new(Box::new(BasicRegistry));
    let log = Arc::new(Mutex::new(Vec::new()));
    let id = api.add_listener(recorder("gone", Priority::Normal, &log));
    assert_eq!(api.listener_count(), 1);
    assert!(api.remove_listener(id));
    assert!(!api.remove_listener(id));
    api.dispatch_send(
        &user(),
        PacketType::PlayServer(PlayServer::KEEP_ALIVE),
        &keep_alive_body(1),
    );
    assert!(log.lock().unwrap().is_empty());
    assert_eq!(api.listener_count(), 0);
}

#[test]
fn a_listener_added_during_dispatch_does_not_deadlock() {
    struct Adder(Arc<PacketEvents>);
    impl PacketListener for Adder {
        fn on_packet_send(&self, _event: &mut PacketEvent) {
            self.0.add_listener(Arc::new(Adder(self.0.clone())));
        }
    }
    let api = Arc::new(PacketEvents::new(Box::new(BasicRegistry)));
    api.add_listener(Arc::new(Adder(api.clone())));
    api.dispatch_send(
        &user(),
        PacketType::PlayServer(PlayServer::KEEP_ALIVE),
        &keep_alive_body(1),
    );
    assert_eq!(api.listener_count(), 2);
}

#[test]
fn the_event_exposes_its_packet_and_user() {
    let event = PacketEvent::new(
        PacketType::PlayServer(PlayServer::PLUGIN_MESSAGE),
        user(),
        vec![],
    );
    assert_eq!(event.packet_id(), 24);
    assert_eq!(event.packet_name(), "plugin_message");
    assert!(event.is_server_side());
    assert_eq!(event.user().name, "player");
}

#[test]
fn priorities_have_ids_in_order() {
    assert_eq!(Priority::Lowest.id(), 0);
    assert_eq!(Priority::Monitor.id(), 5);
    assert_eq!(Priority::from_id(3), Some(Priority::High));
    assert_eq!(Priority::from_id(6), None);
    assert!(Priority::Lowest < Priority::Monitor);
}

#[test]
fn packets_of_the_other_states_are_listed_by_their_table() {
    assert_eq!(
        LoginServer::from_id(2).map(|p| p.name()),
        Some("login_finished")
    );
    assert_eq!(LoginClient::ALL.len(), 5);
}

#[test]
fn raw_particle_and_entity_helpers_read_their_ids() {
    let p = RawParticle(vec![0x07]);
    assert_eq!(p.id(), Some(7));
    assert_eq!(RawParticle(vec![]).id(), None);
    let e = EntityDataEntry {
        index: 0,
        serializer: 1,
        value: vec![5],
    };
    assert_eq!(e.as_varint(), Some(5));
    assert_eq!(e.as_byte(), None);
}
