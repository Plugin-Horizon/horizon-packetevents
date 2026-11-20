# hmod port: packetevents
Parity: ~16% now / ~21% with our PRs (~66/~420 items; estimate, see audit/PARITY.md)
Round 1
- Fixed: src/lib.rs import: `ClientboundPlayPacket` removed from `mod_api` imports (1 error; the type does not exist on main).
- Fixed: src/lib.rs `send_to_player` (dead code): `host.player_send_packet(...)` removed (1 error). The function now ends in a HMOD-GAP comment and returns `false`; its parsing and player lookup are unchanged.
- Permission nodes: none
- Gaps: HMOD-GAP(player-send-packet): Horizon main has no server-side send of a vanilla play packet to one player (only declared payload channels via `PlayerRef::send_payload`). `send_to_player` sends nothing; the mod loses packet sends to players (it was never called).
- Unsure: `let _ = (player, name, body);` keeps the parameters used so the function has no unused-variable warnings; the outgoing and incoming hooks stay as the old TODO markers describe.

Round 2 (parity audit)
- Fixed: the configuration and login enums keep vanilla's names; packetevents' names for the same packets are now associated constants (`LoginServer::LOGIN_SUCCESS`, `ConfigurationClient::PLUGIN_MESSAGE`, ... 13 in all, tested for id and lookup). The player name map is rebuilt every tick (a joiner has a name at once, a leaver is gone). `PACKET_SOURCE_WIRED` (false) tells a dependent mod that no listener is called. README: wrapper naming, the gap list, the removed packet names.
- Not fixable here: no server-side send hook, no cancellable receive hook, no packet send to one player, no registry protocol ids (see README Known gaps).
- Unsure: associated constants named like a packetevents constant never collide with a variant of the same enum.

Round 3 (100 % push, blind; no cargo run)
- Added: `HandshakingClient` (HANDSHAKE 0, LEGACY_SERVER_LIST_PING 0xFE), `HandshakingServer` (LEGACY_SERVER_LIST_RESPONSE 0xFE), `StatusClient` (REQUEST 0, PING 1), `StatusServer` (RESPONSE 0, PONG 1), all wired into `PacketType` (state, side, id, name, lookup). Test: `handshaking_and_status_packets_have_their_ids`.
- Added: `PlayClient::ANIMATION` (packetevents' name, the client arm swing = `PUNCH` 46). Test in `packetevents_names`.
- Fixed: the uuid-to-name map is also set at `player_logged_in` and removed at `player_logged_out` (not only at the next tick).
- Added behind cargo feature `horizon_serverbound_packet` (off by default; horizon-rust/horizon#18803): `serverbound_packet` hook feeds custom payloads to the receive listeners, a cancel returns `Flow::Cancel`. `RECEIVE_SOURCE_WIRED` tells a dependent mod. Other serverbound packets are not heard (the hook carries no body).
- Added behind cargo feature `horizon_player_send_packet` (off by default; horizon-rust/horizon#18021): `send_to_player` calls `host.player_send_packet`. Without the feature it still sends nothing.
- README: Known gaps rewritten (feature flags, what is still missing), handshake/status names corrected, login/logout name rule.
- Not done: the ~280 missing wrappers (large, no API needed; not started here), the play names renamed by Horizon (`PLAYER_INFO`, `TITLE`, `STEER_VEHICLE` ...: each needs a 1:1 mapping checked against the original, and some are one-to-many), user connect/login/disconnect events on the listener trait, `User` fields beyond uuid/name/state, `PacketEvents` settings, a load-time "no packet source" log (no log call in the mod API reference).
- API-blocked: server-side send hook, rewrite/cancel of all serverbound packets, registry protocol ids for items and particles (GAPS_UPSTREAM line added).
- Unsure: `PlayClient::ANIMATION` assumes the port's `Animation` wrapper (empty body, `PUNCH`) matches packetevents' ANIMATION body in 26.3.
- Unsure: the feature-gated code is written blind against `ServerboundPacketEvent { player, packet: ClientPacket { id, custom_payload } }` and `ClientPayload { channel, data }` as in the branch; the generated API file does not have them yet.

Checklist round 100 (packetevents is a library: no commands, permissions or config keys in the original; the items are its packet API surface)
- [x] Handshaking packet types (3 types)
- [x] Status packet types (4 types)
- [x] Login and configuration packetevents names (round 2)
- [x] Play client ANIMATION name
- [x] Listener priorities, cancel, changed outcome (`PacketEvents::dispatch`)
- [x] Name map for users: at login, logout and each tick
- [x] Receive side wiring (behind horizon_serverbound_packet)
- [x] Send to one player (behind horizon_player_send_packet)
- [ ] Renamed play packet names (PLAYER_INFO, TITLE, STEER_VEHICLE, PICK_ITEM, ...): not done, needs a checked mapping
- [ ] Wrappers: ~280 of ~330 missing (not API-blocked; the largest open item)
- [ ] UserConnect / UserLogin / UserDisconnect events on the listener trait (the hooks exist: player_logged_in / player_logged_out)
- [ ] User fields: entity id, address, client version, profile textures, connection state details
- [ ] PacketEvents settings (checkForUpdates, bStats, debug, reEncodeByDefault): a library has no settings screen; decide with Leo
- [ ] Load-time notice that no packet source is wired (no log call in the mod API reference)
- [~] Send-side and cancellable serverbound hooks: API-blocked on main (GAPS_UPSTREAM)
- [~] Registry protocol ids for items and particles: API-blocked (GAPS_UPSTREAM)
- [~] ClientVersion / ServerVersion: no API; ViaVersion-translated clients are invisible
