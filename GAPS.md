# What Horizon is missing for this mod

Checked against Horizon bc08885276def824e719e18921d8fb27c92c6904 (upstream/main, 2026-11-09). Each entry: what the mod
would need, which feature it serves, and what the mod does today. Proposed signatures follow the conventions of
docs/native-mods.md; they are suggestions for the engine side, not a request for a particular name.

## Hooks

- [ ] **A cancellable hook for every serverbound play packet** (the receive side). Proposed:
  `native_client_packet_received(player, name, packet: &mut ServerboundPlayPacket, cancel: &mut bool)`, heard in the
  server's packet handler before the packet is applied, with a watch switch like `native_watch_server_packets`
  (decode only while a mod listens). Feature: `PacketEvent.isCancelled` and changed serverbound packets for
  `PLAYER_DIGGING`, `PLAYER_BLOCK_PLACEMENT`, `INTERACT_ENTITY`, `PLAYER_POSITION` and the rest of the client's
  packets. Today: `native_server_packets(player, packets, outbound = false)` counts received packets once a tick, after
  they were applied; the mod's receive listeners run only for plugin messages (below), and cannot cancel.

- [ ] **`custom_payload_from_client` cancellable** (`native_custom_payload_from_client`, currently listen-only).
  Proposed: a `cancel: &mut bool` out-parameter, so a plugin channel message a listener refuses is not applied.
  Feature: `PacketEvent.setCancelled` on `PLUGIN_MESSAGE` from the client. Today: the message is applied whatever the
  listeners say (the glue's `custom_payload_from_client` is listen-only).

- [ ] **Configuration and login packets.** `native_client_packet` (decoded, listen-only) skips the configuration phase,
  and nothing hears the login packets. Proposed: `native_server_packet_config(player, packet, cancel)` for the
  configuration clientbound packets, and the same for the serverbound ones. Feature: `ConnectionState.CONFIGURATION` and
  `LOGIN` listeners (`RegistryData`, `FinishConfiguration`, `KeepAlive` in configuration). Today: the mod sees Play only;
  the core has the tables for the other states (`PacketType` covers them).

- [ ] **The `server_packet` hook carries the player's name.** Proposed: pass the `NativeServerPlayer` (uuid and name)
  or `name: &str` next to `player`. Feature: `User.name` for listeners. Today: the glue keeps a name cache, refreshed
  from `host.server_players()` every 20 ticks, so a player who joined within the last second shows an empty name.

## Helpers

- [ ] **Send a clientbound play packet to one player.** Proposed:
  `native_send_play_packet(player: u128, packet: ClientboundPlayPacket) -> bool`, the public form of the internal
  `Game::native_send` (native_server.rs, used by `native_action_bar`), usable from any server hook and from a mod's
  own packets. Feature: `User.sendPacket` and every typed send of the wrappers (title, boss bar, teams, scoreboard,
  entity data), and the mod's own packet shapes. Today: `send_to_player` in `api/src/lib.rs` returns false. Only the
  fixed-purpose helpers exist (action bar, title, sound, particles, custom payloads): a plugin channel message
  (`native_send_custom_payload_to_player`) reaches a vanilla client but can carry only data the mod encodes itself.

- [ ] **`watch_server_packets` on the mod's `Host`.** The fragment `server_packet` names the helper
  `native_watch_server_packets` (native_packets.rs). The glue calls `host.watch_server_packets()` (UNVERIFIED: the
  `Host` method name). Feature: `server_packet` heard at all. Today: nothing is heard until the glue's first tick
  calls it. If `Host` has no such method, a `Hooks` method would be the way to switch it on.

- [ ] **Per-packet switching for `server_packet`.** Once the watch is on, every play packet to every guest is
  decoded and encoded again, even when a listener wants one id. Proposed: `native_watch_server_packets(ids: &[i32])`
  (or a set per mod) so the server decodes only the listened packets. Feature: the cost of a busy server. Today:
  the whole stream is decoded once the watch is on (the STATUS.md performance check measures it).

- [ ] **A single-value entity data reader.** `entity_data::read_value(kind, r)` is private, so the core cannot measure
  an entity value of any serializer beyond the fixed table (item stacks 7, particles 16 and 17, profiles 41, wolf and
  painting variants 25 and 34, optional living entity 13, and the others). Proposed: `pub fn entity_data::value_len(kind,
  r) -> Result<usize>` or `pub fn read_value`. Feature: `EntityMetadata` for every entity type. Today:
  `HorizonRegistry` keeps the core's fixed table. A listener's `read` of such a packet fails, so the listener cannot change
  it and the packet goes out as it was.

- [ ] **Serverbound `COMMAND_SUGGESTION` (id 15) is not decoded.** Horizon has the id and no packet type for it, so it
  arrives as `Unknown`. Proposed: a typed `CommandSuggestion { id: i32, text: String }` in `packet_enum!`. Feature:
  `TAB_COMPLETE` listeners. Today: the wrapper `TabComplete` reads the bytes; no hook carries them until the receive
  hook above exists.

- [ ] **`Slot` and `Particle` readers as public codecs.** The glue calls `horizon_protocol::item::Slot` and
  `horizon_protocol::entity_data::Particle::read` to measure items and particles (UNVERIFIED that both are public
  paths of the crate). Proposed: the same names exported from `horizon_protocol`. Feature: exact lengths of item stacks
  with components. Today: the glue calls them as written; a path that is not public stops the build, not the server.

## Protocol data

- [ ] **Chat signature acknowledgement and the client's chat packet.** The serverbound chat (`CHAT`, id 9) layout is not in
  the core: its signature, last-seen and acknowledgement parts need the chat signing state. Feature: listeners on what a
  player types. Today: no wrapper; the packet goes through unchanged.

- [ ] **Click window (`CONTAINER_CLICK`, id 18) with hashed slots.** Needs the hashed item codec (`HashedSlot`). Feature:
  inventory listeners. Today: no wrapper.

- [ ] **Registry-dependent parts measured by the core.** `basic_item_len` (component-free stacks only), particle
  payloads (state and colour types only: ids 1, 2, 36, 121, 125, 28, 46, 52) and entity serializers 0-15, 18, 39, 40.
  The other particle and serializer ids need the engine's tables; the glue asks Horizon's codecs (see the `Slot` and
  `Particle` items above). Feature: exact wire lengths. Today: an unmeasurable part makes the packet fail to decode and
  it goes out unchanged.

## Packet events

- [ ] **Connection data on `User`**: remote address, client version, the protocol the client speaks (packetevents'
  `User.getAddress`, `getClientVersion`). Feature: listeners that depend on the client (Bedrock, old clients). Today:
  `User` has the uuid, the name (cached) and the state. Not checked whether the engine hands the address to a hook.

- [ ] **Packets from bundles.** Whether `server_packet` is heard once per packet of a `BUNDLE_DELIMITER` bundle (as the
  client receives it) is not checked against the source. Feature: consistent listeners for bundled packets (spawn
  batches). Today: unverified.

- [ ] **Packetevents' post tasks** (`PacketSendEvent.getTasksAfterSend`). Feature: run code after a packet was sent.
  Proposed: a completion callback of the engine's send, if it exists. Today: not ported.

## Not gaps (by design)

- Version translation: 26.3 only, as the brief says. Horizon translates for older clients natively.
- packetevents' `NAMED_SOUND_EFFECT` packet does not exist in 26.3; the sound packet names an inline sound, which
  `SoundEffect` covers (`SoundRef::Inline`).
