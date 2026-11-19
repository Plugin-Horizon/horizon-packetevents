# horizon-packetevents

> **Unofficial Rust port of packetevents for the Horizon server (https://github.com/arismoko/horizon). Not affiliated with or endorsed by the packetevents authors.**

A packet API for [Horizon](https://github.com/arismoko/horizon) native mods, after
[packetevents](https://github.com/retrooper/packetevents): packet types, listeners with priorities,
cancellation, typed wrappers and sending a packet to a player, for Minecraft 26.3. Other native mods
depend on it instead of hooking raw bytes.

Derived from packetevents (GPL-3.0-or-later). The licence of this mod is therefore GPL-3.0-or-later (see LICENSE);
the copyright holders are in NOTICE.

Upstream: packetevents (https://github.com/retrooper/packetevents), commit `3e4e42ea9b8f2a6145a35c7fb80b618343266807`,
GPL-3.0. Horizon pin: `1453e0e614af4b2d04dd0d228a94162b5d459910`.

> **Built for and working only with [arismoko/horizon](https://github.com/arismoko/horizon)** (the Horizon fork that carries the mod API). [horizon-rust/horizon](https://github.com/horizon-rust/horizon) is experimental for mods and not supported: the earlier version for its native hooks / horizon-mixins is frozen on the `horizon-plain` branch (work stopped on 2026-11-12).

## Which Horizon

This is a native trait mod: `horizon-mod.toml`, a `Cargo.toml` that depends on `horizon-mod-api`, `src/lib.rs` with the hooks, and the plugin logic in the `core/` crate (plain Rust, no engine needed for its tests).

- Pin: `[native] horizon = "54090ee36c4e2c7b43b66db864220c577b94b7e0"` in `horizon-mod.toml` (game `26.3`). That is a development build of arismoko/horizon `main` plus mod API additions that are not all merged upstream yet, so a stock `main` may lack some host methods this mod calls until they land.
- It compiled and loaded on that build and was smoke-tested there; it has not seen a long run with real players. Treat it as experimental.
- Feature descriptions below were written for the earlier horizon-rust version; where they name engine calls (`native_*`) or old file paths, read them as descriptions only. The Known gaps list is current.

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
  them (`CHAT_MESSAGE`, `PLAYER_BLOCK_PLACEMENT` ...), with the 26.3 ids. The configuration and login enums
  use vanilla's names for some packets; packetevents' names for them (`LOGIN_SUCCESS`,
  `CONFIGURATION_END_ACK` ...) are associated constants of the same packet. Packets that 26.3 removed
  (`SPAWN_LIVING_ENTITY`, `TITLE`, `WINDOW_CONFIRMATION` ...) and the handshake and status packets do not exist. `PacketType::lookup` finds one by
  state, side and id.
- **Listeners** (`pe::event`): implement `PacketListener` with `priority()` (`Lowest` ... `Monitor`) and
  `on_packet_send` / `on_packet_receive`. Listeners run lowest priority first, and `Monitor` last.
- **Events** (`pe::event::PacketEvent`): `is_cancelled` / `set_cancelled`, `mark_for_reencode`, the user,
  the packet type, `read::<Wrapper>()` and `write(&wrapper)`. A changed event is re-encoded only when it is
  marked, as packetevents does.
- **Wrappers** (`pe::wrapper`): 43 typed wrappers, named as packetevents' without the
  `WrapperPlayServer` / `WrapperPlayClient` prefix (`SetTitleText` for `WrapperPlayServerSetTitleText`).
  Each round-trips its bytes exactly; every other packet is raw bytes.
- **Registry** (`pe::Registry`): item slots, particles and entity data values need the game's registries to
  be measured. The glue asks Horizon's own codecs for them; the core alone knows the simple cases.

## Build

Build it as a native trait mod with arismoko's launcher (see the "native mods" how-to in the arismoko/horizon docs):

```sh
horizon-launcher native-build <path to this folder> --server-output <dir> --version <tag> --trust-native-code
```

`--trust-native-code` is your consent to compile and run this code inside the server. The logic crate tests run without the engine: `(cd core && cargo test)`.

## Known gaps

**This mod does not intercept packets yet.** A listener added with `add_listener` is never called
(`PACKET_SOURCE_WIRED` is `false`). What Horizon's mod API lacks, exactly:

- a server-side hook for the play packets the server sends a player (the API has only the client-side `client_packet` and `client_packet_send`);
- a cancellable server-side hook for the packets a player sends;
- sending a vanilla play packet to one player (only declared payload channels, `PlayerRef::send_payload`);
- the item and particle registry protocol ids, so item slots and particles in packets are measured without them.

Until then the mod is a packet codec (types, wrappers, listener dispatch) for other mods. Also missing compared with packetevents:
`PacketSendEvent` / `PacketReceiveEvent` / user connect, login and disconnect events (one `PacketEvent` serves both
directions), `ClientVersion` and `ServerVersion`, the `User` details beyond uuid, name and state, `PacketEvents.load/init`
settings, and wrappers for the packets that have none (raw bytes only).

## Licence and credits

GPL-3.0-or-later. See LICENSE and NOTICE. Modifications for Horizon are by LionSpacer.
