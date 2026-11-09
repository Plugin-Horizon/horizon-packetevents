# Contributing to horizon-packetevents

Pull requests and improvements are welcome. Bug reports, ideas and documentation fixes count just as much as code.

## Building and testing the core

The packet model is plain Rust with no dependencies, and it builds without the Horizon engine:

```sh
(cd core && cargo test)
```

CI runs exactly this on every push and pull request. The glue in `api/` depends on the engine and is not built by CI;
if you change it, say in the PR how you tested the change.

## Rules for changes

1. A wrapper reads and writes exactly the bytes the wire has, and round-trips them (`api/src/pe/tests.rs`).
2. Layouts follow Minecraft 26.3. Cite the source of a layout in the PR (the protocol docs or Horizon's packet definition).
3. Engine hooks the glue needs but Horizon does not have go into GAPS.md, not into a workaround in the core.
4. Ported code keeps the packetevents copyright header (see NOTICE).

## Licence of contributions

By submitting a contribution you agree that it is licensed under GPL-3.0-or-later (see `LICENSE`). Only submit code you
wrote yourself or that comes from a source with a compatible licence, and name the source in the code.

## Be nice

Be kind, be specific, and assume good faith. See CODE_OF_CONDUCT.md.
