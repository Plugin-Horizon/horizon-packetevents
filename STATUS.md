# Status (2026-11-09, written blind: no engine build yet)

Legend: **written** = code and rule done, unit-tested where the rule is engine-free; **glue, unbuilt** = written against the
engine's names but not compiled with Horizon; **not started** = absent (GAPS.md says why).

Base: Horizon `bc08885276def824e719e18921d8fb27c92c6904` (upstream/main on 2026-11-09). The packet enums, the id tables, `encode`/`decode`
and the packet hook fragments were read at that commit (play, configuration and login protocol files unchanged since 6bab737cae).

## Tests

`cd core && cargo test`: **81 tests**, all green, no dependencies (the core is `api/src/pe/`, included as a crate of its own).

| Area | Tests |
|---|---|
| Codec: VarInt/VarLong edges, numbers, UUID, strings, block and section positions, LpVec3, network NBT (every tag, modified UTF-8, nesting limit) | 21 |
| Registry: basic item slots, particle measuring, fixed entity serializers | 3 |
| Packet tables: ids against the 26.3 values, round trip of every entry, dense ids, lookup by state and side | 5 |
| Wrappers: a round trip for each of the 43 wrappers (several shapes for the multi-action ones), known bytes, whole-body checks | 40 |
| Events and dispatch: priority order, cancel reaches every listener, changed bodies re-encoded only when marked, receive side, removal, a listener adding a listener | 12 |

## Features

| Feature (packetevents) | State | Notes |
|---|---|---|
| Packet types `PlayServer`/`PlayClient` (144 and 69 ids), configuration clientbound (21), configuration serverbound and login (Horizon's names) | written | `api/src/pe/packet_type.rs`; ids are the 26.3 positions, checked against Horizon's id tables (same count, same id for every wrapped packet) |
| `ConnectionState`, `PacketType::lookup(state, side, id)` | written | |
| `Priority` (LOWEST, LOW, NORMAL, HIGH, HIGHEST, MONITOR) | written | |
| `User` (uuid, name, state) | written | no address, client version or connection events (GAPS.md) |
| `PacketEvent`: type, id, name, user, body, cancel, `mark_for_reencode`, `read::<W>`, `write(&w)`, `set_body` | written | packetevents' `getLastUsedWrapper`, post tasks, `clone` and timestamps are not ported |
| `PacketListener` (`priority`, `on_packet_send`, `on_packet_receive`) | written | |
| `PacketEvents`: add and remove listeners, `dispatch_send` / `dispatch_receive` with `Outcome` | written | a cancelled packet still reaches every listener (as packetevents) |
| Wrappers, 43 of them (28 clientbound, 15 serverbound) | written | tables below |
| Typed encode/decode with Horizon's own types | glue, unbuilt | the glue hands each packet body to the core and gets it back through Horizon's `encode()`/`decode(id, body)`; the core has its own wire codec |
| Item slots, particles | written (basic) / glue, unbuilt | the core measures only empty and component-free items and the state/colour particles; the glue uses Horizon's `Slot` and `Particle` codecs |
| Entity data values | written (fixed serializers) | serializers 0-15, 18, 39, 40 measured by the core; the rest need the engine (GAPS.md) |
| Hook `server_packet` (every play packet to a guest, changeable, cancellable) | glue, unbuilt | needs `watch_server_packets` (called from the first tick) |
| Hook `custom_payload_from_client` (plugin messages from a player) | glue, unbuilt | listen-only: cannot cancel (GAPS.md) |
| Receive side for play packets other than plugin messages | not started | no engine hook (GAPS.md) |
| Sending a typed packet to one player (`User.sendPacket`) | not started | the glue's `send_to_player` returns false; no engine helper (GAPS.md) |
| Packet events of login and configuration | not started | the engine does not call a hook for them (GAPS.md) |
| Version translation | out of scope | 26.3 only, by the brief |

## Wrappers

Clientbound (server to client), `api/src/pe/wrapper/clientbound.rs`:

| Wrapper | Packet (26.3 id) | Covers |
|---|---|---|
| `ChatMessage` | CHAT_MESSAGE (66) | PlayerChat: signed chat, signature cache, filter mask, chat type holder |
| `SystemChatMessage` | SYSTEM_CHAT_MESSAGE (124) | system chat, or the action bar with `overlay` |
| `SetTitleText` | SET_TITLE_TEXT (116) | title line |
| `SetTitleSubtitle` | SET_TITLE_SUBTITLE (114) | subtitle line |
| `SetTitleTimes` | SET_TITLE_TIMES (117) | fade in, stay, fade out |
| `ClearTitles` | CLEAR_TITLES (14) | clear, with reset |
| `ActionBar` | ACTION_BAR (89) | action bar line |
| `BossBar` | BOSS_BAR (9) | add, remove, progress, name, style, flags |
| `EntityMetadata` | ENTITY_METADATA (101) | synched entity data; values kept as bytes, serializers 0-15, 18, 39, 40 measured |
| `SpawnEntity` | SPAWN_ENTITY (1) | AddEntity, velocity as LpVec3 |
| `EntityTeleport` | ENTITY_TELEPORT (128) | TeleportEntity with relative flags |
| `BlockChange` | BLOCK_CHANGE (8) | BlockUpdate |
| `MultiBlockChange` | MULTI_BLOCK_CHANGE (86) | SectionBlocksUpdate, `decode_entry` gives state and world position |
| `PlayerInfoUpdate` | PLAYER_INFO_UPDATE (71) | player list: add, chat session, game mode, listed, latency, display name, order, hat |
| `PlayerInfoRemove` | PLAYER_INFO_REMOVE (70) | player list removals |
| `ScoreboardObjective` | SCOREBOARD_OBJECTIVE (108) | create, remove, update, number formats |
| `UpdateScore` | UPDATE_SCORE (112) | SetScore |
| `Teams` | TEAMS (111) | the seven methods, parameters and players |
| `SetSlot` | SET_SLOT (20) | ContainerSetSlot; item bytes kept (`RawItem`) |
| `WindowItems` | WINDOW_ITEMS (18) | ContainerSetContent; items and cursor item kept as bytes |
| `OpenWindow` | OPEN_WINDOW (60) | OpenScreen: id, menu type, title |
| `Particle` | PARTICLE (48) | LevelParticles; particle bytes kept, length from the registry |
| `SoundEffect` | SOUND_EFFECT (119) | Sound: registry id or inline name (the "named sound" form) |
| `EntitySoundEffect` | ENTITY_SOUND_EFFECT (118) | SoundEntity |
| `KeepAlive` | KEEP_ALIVE (45) | keep alive, server side |
| `PluginMessage` | PLUGIN_MESSAGE (24) | CustomPayload: channel and bytes to the end |
| `ResourcePackSend` | RESOURCE_PACK_SEND (82) | ResourcePackPush: url, hash, required, prompt |
| `ResourcePackRemove` | RESOURCE_PACK_REMOVE (81) | ResourcePackPop: one pack or all |

Serverbound (client to server), `api/src/pe/wrapper/serverbound.rs`:

| Wrapper | Packet (26.3 id) | Covers |
|---|---|---|
| `ChatMessage (serverbound)` | not wrapped | see GAPS.md (signature and acknowledgement layout) |
| `PluginMessage` | PLUGIN_MESSAGE (22) | CustomPayload from the client |
| `KeepAlive` | KEEP_ALIVE (28) | keep alive answer |
| `InteractEntity` | INTERACT_ENTITY (26) | Interact: entity, hand, LpVec3 location, sneaking (use entity) |
| `AttackEntity` | ATTACK (1) | Attack: entity (hit) |
| `PlayerPosition` | PLAYER_POSITION (30) | MovePlayerPos |
| `PlayerPositionAndRotation` | PLAYER_POSITION_AND_ROTATION (31) | MovePlayerPosRot |
| `PlayerRotation` | PLAYER_ROTATION (32) | MovePlayerRot |
| `PlayerFlying` | PLAYER_FLYING (33) | MovePlayerStatus (on ground flags only) |
| `PlayerDigging` | PLAYER_DIGGING (41) | PlayerAction (block dig) |
| `BlockPlace` | PLAYER_BLOCK_PLACEMENT (66) | UseItemOn (block place) |
| `HeldItemChange` | HELD_ITEM_CHANGE (54) | SetCarriedItem |
| `Animation` | PUNCH (46) | arm swing; empty in 26.3 |
| `ClientSettings` | CLIENT_SETTINGS (14) | ClientInformation |
| `TabComplete` | TAB_COMPLETE (15) | command suggestion request; Horizon has the id but no decoded packet |
| `ResourcePackStatus` | RESOURCE_PACK_STATUS (50) | ResourcePackResponse |

Not wrapped yet: the client's chat (signatures and acknowledgements), click window (hashed slots), creative inventory
(item bytes; a small addition), the recipe book, and the packets of login and configuration. A wrapper is one struct and
one `Wrapper` impl: the pattern is in place.

## Layout decisions

- **Raw bytes where the length is not the core's to know.** Item slots (`RawItem`), particles (`RawParticle`)
  and entity values (`EntityDataEntry`) keep their bytes; the registry measures them. This keeps
  round trips exact without a copy of every component codec.
- **Horizon does the encoding.** The glue calls `encode()`/`decode(id, body)` of Horizon's packet enums.
  Unmodelled packets come through as Horizon's `Unknown(id, payload)`, which the glue passes on unchanged.
- **Quantised vectors.** `LpVec3` values are quantised by the wire; the tests compare round trips of quantised values.

## Test first, once a build is possible

1. `horizon-mixins apply` with this bundle at the commit above, then `cargo build` of the mod. Expected trouble spots:
   the `Hooks` method signatures of `server_packet` and `custom_payload_from_client` (UNVERIFIED in `api/src/lib.rs`),
   `host.watch_server_packets()`, the `horizon_protocol` dependency name and the `Slot`/`Particle` paths in `HorizonRegistry`.
2. A round trip per packet on a live server: a probe client connects, the server sends each wrapped clientbound packet
   (a title, a boss bar, a team), the listener reads and writes it, the client shows the same. Compare with Horizon's
   own decode of the changed body.
3. A listener that cancels a `KEEP_ALIVE` (the client must disconnect, the server log must say so) and one that rewrites a
   `SET_SLOT` item count.
4. Priority order on a live server: two listeners with different priorities mutate the same packet; the result must follow
   the order.
5. Performance: with the watch on, every packet is decoded: measure the tick time with a busy world (chunks, entities).
