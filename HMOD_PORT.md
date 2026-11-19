# hmod port: packetevents
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
