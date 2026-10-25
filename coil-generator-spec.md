# PCB Axial-Flux Stator Coil Generator — Build Spec

## Goal

Given a motor's pole/coil count, mean radius, and winding parameters, generate the
complete stator winding on a live KiCad 10 board via the IPC API, implemented in
Rust using the `kicad-ipc-rs` crate: coil traces per layer, interlayer via
stitching, phase grouping with correct winding sense, and routed connections out
to a shared connector footprint. The board hosts two motor islands (feed motor,
peel motor) with independent parameters, so the tool must operate on a *region*
of a larger board, not assume the whole board is one motor.

## Environment constraints (KiCad 10 IPC API, Rust) — read this before writing any code

- The IPC API only talks to a **running GUI instance** of KiCad. There is no
  headless/batch mode in KiCad 10 — that lands in KiCad 11 via `kicad-cli`. Plan
  for the generator to be run as a standalone Rust binary while KiCad is open
  with the target board loaded and the API server enabled (Preferences → Plugins
  → enable IPC API server). The socket path is auto-detected by the client
  library; override via `KICAD_API_SOCKET` if needed.
- Use the **`kicad-ipc-rs`** crate (third-party, MIT licensed, actively
  maintained — not the official KiCad-team Rust bindings, which are an
  unmaintained experimental effort per the KiCad devs themselves and should be
  avoided for now). `kicad-ipc-rs` is async-first (Tokio) with an optional
  `blocking` feature for a sync wrapper — use the blocking wrapper unless there's
  a reason to want async, since this tool is a linear generate-and-exit script,
  not a long-running service.
- `kicad-ipc-rs` claims full command coverage for KiCad v10.0.1, backed by CI
  tests, as of its current release — but it's a single-maintainer crate with a
  much smaller support surface than the official Python bindings. Check its
  GitHub issues/recent commits before leaning on any specific call, and have a
  fallback plan (the crate exposes a raw `prost_types::Any` payload escape hatch
  for calling API commands not yet wrapped in the typed layer).
- **No plotting/export (gerbers, etc.) via the API in KiCad 10.** Gerber
  generation stays a manual step (KiCad's own plot dialog, or a separate
  `kicad-cli` call) after the generator finishes — don't build that into this
  tool.
- API coverage is still expanding release to release upstream in KiCad itself.
  Don't assume parity with the old SWIG `pcbnew` API for anything not explicitly
  confirmed present in `kicad-ipc-rs`'s coverage matrix.
- This does **not** need to be a KiCad-loaded plugin — `kicad-ipc-rs` connects
  to a running KiCad instance from an external process, so this can be a normal
  standalone Rust binary (`cargo run --bin generate-stator`) invoked from the
  command line while KiCad happens to be open, rather than something installed
  into KiCad's plugin menu. Simpler to build and iterate on given the rest of
  Kevin's toolchain is already CLI-first.

## Inputs (per motor instance — the board will have two)

- Placement: center (x, y) on the board, rotation
- Poles, coils
- Mean radius, active radial length (L)
- Trace width, trace spacing, copper weight (can vary by layer; default uniform)
- Layer count + which copper layers this motor's winding occupies (the two motors
  share a board, so layer/region assignment must be explicit, not assumed)
- Winding direction convention (CW/CCW) per layer
- Phase assignment rule: coil index mod 3, with per-coil winding-sense (+1/-1)
  chosen so same-phase coils reinforce rather than cancel — same logic already
  validated in the pole/coil animation tool (electrical angle = pole_pairs ×
  mechanical angle; reference angle per phase group; flip sign if a coil's base
  angle is >90° from that reference)
- Via drill + annular ring size for interlayer stitching
- Target connector pads (where each phase's start/end should land)

## Algorithm

1. Compute coil center angles evenly spaced around the mean radius.
2. For each coil, generate a spiral/racetrack trace path per layer: start near the
   outer radius, spiral inward with pitch = trace width + spacing, reversing
   spiral direction on alternating layers so turns stack in series through vias
   rather than canceling.
3. Place a via at each coil's layer-to-layer transition point, connected so the
   next layer's turn continues additively (same current direction) rather than
   fighting the previous layer.
4. Group coils into three phases (index mod 3) and apply the sign/reversal logic
   to determine series connection order and which coils need reversed winding.
5. Route each phase's coils in series (coil 1 out → coil 4 in → coil 7 out → …)
   ending at that phase's connector pad.
6. Repeat independently for both motor islands, respecting each one's assigned
   layer set and keeping the two regions' copper isolated except at the shared
   connector.
7. Run a basic clearance/DRC sanity pass against the target fab's min trace/space
   before treating a generated layout as final — flag violations rather than
   silently producing an unroutable board.

## Suggested code structure (Rust crate layout)

- `src/coil_geometry.rs` — pure math: pole/coil/radius/turn params → per-coil,
  per-layer polyline point lists. No `kicad-ipc-rs` dependency; fully unit
  testable with `cargo test`, no KiCad involved.
- `src/winding_map.rs` — phase assignment + sign/reversal logic, also
  KiCad-independent and unit-testable the same way.
- `src/kicad_writer.rs` — takes geometry + winding output, uses `kicad-ipc-rs`'s
  blocking client to create tracks, vias, and footprints on the live board.
- `src/bin/generate_stator.rs` — binary entry point; reads a per-motor config
  (TOML fits Rust conventions well, or JSON if preferred) and ties the above
  together. Run as `cargo run --bin generate-stator -- --config feed_motor.toml`
  style, against an already-open KiCad instance.
- Consider a `config.rs` module with `serde`-derived structs for the per-motor
  parameter set, so both motor islands (feed, peel) are just two config files
  read by the same binary rather than hardcoded branches.

## Testing approach

- `coil_geometry` and `winding_map` should be fully unit tested via `cargo test`
  without KiCad running at all, since they're pure functions — this is most of
  the actual design logic and should be nailed down before touching the IPC
  layer at all.
- `kicad_writer` needs a real running KiCad instance with the API enabled.
  Start with a minimal smoke-test binary that just connects and places one via,
  using `KiCadClientBlocking::connect()`, before attempting the full generator —
  `kicad-ipc-rs` is beta and worth confirming basic connectivity/auth works in
  Kevin's environment before building on top of it.
- Given `kicad-ipc-rs` is a smaller, single-maintainer crate, it's worth having
  Claude Code check the crate's own example code and test suite (linked from its
  README/docs.rs page) for the current idiomatic way to create tracks and vias,
  rather than guessing at the API shape from first principles.

## Known rough edges to flag up front

- `kicad-ipc-rs` is beta and unofficial — confirm current track/via/footprint
  creation coverage against its docs/coverage matrix before assuming any
  specific call exists, and have a fallback plan via its raw protobuf escape
  hatch (`*_raw` APIs) for anything not yet wrapped.
- No headless mode in KiCad 10 — a human needs KiCad open with the target board
  and API server enabled whenever this runs. This is a hard requirement of the
  IPC API itself, not something `kicad-ipc-rs` can work around.
- At small coil pitches (e.g. the ~5mm pitch on the peel motor), the generator
  should warn if requested trace/space doesn't leave room for the requested
  turns per layer, rather than silently generating something unfabricable.
- Since this is a beta third-party crate maintained by one person, budget some
  time for hitting a genuine coverage gap partway through — worth scoping the
  first milestone as "prove out track + via creation end-to-end for one coil"
  before committing to the full generator architecture.
