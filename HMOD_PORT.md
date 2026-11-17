# hmod port: packetevents
Round 1
- Fixed: src/lib.rs import: `ClientboundPlayPacket` removed from `mod_api` imports (1 error; the type does not exist on main).
- Fixed: src/lib.rs `send_to_player` (dead code): `host.player_send_packet(...)` removed (1 error). The function now ends in a HMOD-GAP comment and returns `false`; its parsing and player lookup are unchanged.
- Permission nodes: none
- Gaps: HMOD-GAP(player-send-packet): Horizon main has no server-side send of a vanilla play packet to one player (only declared payload channels via `PlayerRef::send_payload`). `send_to_player` sends nothing; the mod loses packet sends to players (it was never called).
- Unsure: `let _ = (player, name, body);` keeps the parameters used so the function has no unused-variable warnings; the outgoing and incoming hooks stay as the old TODO markers describe.
