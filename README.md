# horizon-packetevents

> **Unofficial Rust port of packetevents for the Horizon server (https://github.com/horizon-rust/horizon). Not affiliated with or endorsed by the packetevents authors.**

A packet API for [Horizon](https://github.com/horizon-rust/horizon) native mods, after
[packetevents](https://github.com/retrooper/packetevents): packet types, listeners with priorities,
cancellation, typed wrappers and sending a packet to a player, for Minecraft 26.3. Other native mods
depend on it instead of hooking raw bytes.

Derived from packetevents (GPL-3.0-or-later). The licence of this mod is therefore GPL-3.0-or-later (see LICENSE);
the copyright holders are in NOTICE.

**State: written without building the engine (see STATUS.md and GAPS.md).** The packet model is unit-tested
(`core/`, 81 tests); the glue to Horizon's hooks has not run yet.

## What it gives a mod

```rust
use mod_packetevents::pe::{PacketEvent, PacketListener, Priority, Tag};
use mod_packetevents::pe::wrapper::clientbound::SetTitleText;

struct Titles;

impl PacketListener for Titles {
    fn priority(&self) -> Priority { Priority::High }

    fn on_packet_send(&self, event: &mut PacketEvent) {
        if event.is_cancelled() { return; }
        let registry = mod_packetevents::packet_events().registry();
        if let Ok(mut title) = event.read::<SetTitleText>(registry) {
            title.text = Tag::plain("Welcome");
            // the event now carries the changed packet, re-encoded when it is sent
            event.write(&title).ok();
        }
    }
}

// at start-up:
mod_packetevents::packet_events().add_listener(std::sync::Arc::new(Titles));
```

- **Packet types** (`pe::packet_type`): `PlayServer` (clientbound), `PlayClient` (serverbound),
  `ConfigurationServer`, `ConfigurationClient`, `LoginServer`, `LoginClient`, named as packetevents names
  them (`CHAT_MESSAGE`, `PLAYER_BLOCK_PLACEMENT` ...), with the 26.3 ids. `PacketType::lookup` finds one by
  state, side and id.
- **Listeners** (`pe::event`): implement `PacketListener` with `priority()` (`Lowest` ... `Monitor`) and
  `on_packet_send` / `on_packet_receive`. Listeners run lowest priority first, and `Monitor` last.
- **Events** (`pe::event::PacketEvent`): `is_cancelled` / `set_cancelled`, `mark_for_reencode`, the user,
  the packet type, `read::<Wrapper>()` and `write(&wrapper)`. A changed event is re-encoded only when it is
  marked, as packetevents does.
- **Wrappers** (`pe::wrapper`): 43 typed wrappers, named `Wrapper*` after packetevents'. See the table in
  STATUS.md. Each round-trips its bytes exactly.
- **Registry** (`pe::Registry`): item slots, particles and entity data values need the game's registries to
  be measured. The glue asks Horizon's own codecs for them; the core alone knows the simple cases.

## Layout

- `api/src/pe/` — the packet model, engine-free: `codec.rs` (wire primitives, `Registry`), `nbt.rs` (network NBT,
  text components), `packet_type.rs` (generated 26.3 tables), `event.rs`, `manager.rs` (listeners, dispatch),
  `wrapper/` (`clientbound.rs`, `serverbound.rs`), `tests.rs`.
- `api/src/lib.rs` — the trait mod: `Hooks::server_packet` (every play packet the server sends a guest, encoded
  and decoded with Horizon's own `encode` and `decode`), `tick` (switches the packet watch on, refreshes names),
  `custom_payload_from_client`, and `HorizonRegistry` (Horizon's item and particle codecs).
- `core/` — builds `api/src/pe/` as a crate of its own: `cd core && cargo test`.

Listed in GAPS.md: what Horizon does not have yet (a cancellable incoming packet hook, a helper to send a
clientbound play packet to one player, entity values of any serializer, configuration and login packets).

## Build

As any trait mod: put this folder into the launcher's mods and build the game (docs/native-mods.md, "Trait mods").
It needs no injections. The glue pins Horizon at the commit in `horizon-mixin.toml`.

## Licence and credits

GPL-3.0-or-later. See LICENSE and NOTICE. Modifications for Horizon are by Plugin-Horizon contributors.

## Contributing

Pull requests and improvements are welcome. See CONTRIBUTING.md. Changes to the packet model are tested with
`cd core && cargo test`; CI runs that on every pull request.
