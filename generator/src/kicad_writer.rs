//! Writes a motor's generated winding to a live KiCad board.
//!
//! Bridges `coil_geometry` + `winding_map` output into `kicad_proto` items
//! and submits them via `kicad-ipc-rs`'s `create_items()`, batched under one
//! commit per motor. See `coil-generator-spec.md` and the implementation
//! plan for why this crate hand-encodes protobuf rather than using
//! `kicad-ipc-rs`'s own (crate-private) item types.

use crate::coil_geometry::{self, CoilLayerPath, Point2};
use crate::config::MotorConfig;
use crate::kicad_proto::{self, DrillProperties, PadStack, PadStackLayer, Track, Via};
use crate::winding_map::{self, Phase, WindingSense};
use kicad_ipc_rs::{BoardLayerInfo, CommitAction, KiCadClientBlocking, KiCadError};

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error(transparent)]
    Geometry(#[from] coil_geometry::GeometryError),
    #[error(transparent)]
    KiCad(#[from] KiCadError),
    #[error("unknown KiCad layer name `{0}` in motor config")]
    UnknownLayer(String),
}

#[derive(Debug)]
pub struct WriteReport {
    pub motor_name: String,
    pub created_item_count: usize,
}

/// Controls how much of a motor's winding a `write_motor`/`build_items`
/// call actually emits — for building the board up incrementally and
/// visually checking each stage in the KiCad GUI, rather than generating
/// everything (turns + layer vias + bus wiring, all phases at once) in one
/// shot with no way to tell which part of 30,000+ items is wrong. Each
/// call is stateless/pure — running with a narrower scope, then a wider
/// one, only adds the delta if the scopes don't overlap; it doesn't know
/// what a previous call already wrote to the board.
#[derive(Debug, Clone)]
pub struct GenerationScope {
    pub phases: Vec<Phase>,
    /// Which of `config.layers`' indices to emit turns/track segments for.
    pub layer_indices: Vec<u32>,
    /// Whether to emit inter-coil and phase-termination bus connections
    /// (arcs/spokes/vias outside the coil ring). Layer-transition vias
    /// (coil layer N to layer N+1) are placed manually in the KiCad GUI,
    /// not generated here.
    pub include_bus: bool,
}

impl GenerationScope {
    /// Everything: all phases, every layer, and bus wiring.
    pub fn full(layer_count: u32) -> Self {
        Self {
            phases: vec![Phase::A, Phase::B, Phase::C],
            layer_indices: (0..layer_count).collect(),
            include_bus: true,
        }
    }
}

fn mm_to_nm(mm: f64) -> i64 {
    (mm * 1_000_000.0).round() as i64
}

fn phase_letter(phase: Phase) -> &'static str {
    match phase {
        Phase::A => "A",
        Phase::B => "B",
        Phase::C => "C",
    }
}

fn phase_termination_point(config: &MotorConfig, phase: Phase) -> Point2 {
    let p = match phase {
        Phase::A => config.phase_terminations.a,
        Phase::B => config.phase_terminations.b,
        Phase::C => config.phase_terminations.c,
    };
    Point2 { x_mm: p.x_mm, y_mm: p.y_mm }
}

fn phase_start_termination_point(config: &MotorConfig, phase: Phase) -> Point2 {
    let p = match phase {
        Phase::A => config.phase_start_terminations.a,
        Phase::B => config.phase_start_terminations.b,
        Phase::C => config.phase_start_terminations.c,
    };
    Point2 { x_mm: p.x_mm, y_mm: p.y_mm }
}

fn point_angle_rad(p: Point2) -> f64 {
    p.y_mm.atan2(p.x_mm)
}

fn point_at_radius_angle(radius_mm: f64, angle_rad: f64) -> Point2 {
    Point2 { x_mm: radius_mm * angle_rad.cos(), y_mm: radius_mm * angle_rad.sin() }
}

/// Shortest signed angular delta from `from` to `to`, in (-PI, PI].
fn shortest_angle_delta(from: f64, to: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut delta = (to - from) % two_pi;
    if delta > std::f64::consts::PI {
        delta -= two_pi;
    } else if delta <= -std::f64::consts::PI {
        delta += two_pi;
    }
    delta
}

/// Which of a motor's configured layers a given phase's interconnect bus
/// lives on. Each phase gets its own layer specifically so that three
/// phases' bus arcs — which do overlap in 2D, by construction, since
/// same-phase coils are spaced every 3rd coil around the ring — never
/// touch each other: same radius/angle on different copper layers isn't a
/// short. See `push_bus_connection`'s doc comment for the rest of the
/// reasoning (why a bus radius outside every coil's outer edge, why vias
/// bridge onto it).
fn bus_layer_for_phase_index(layer_ids: &[i32], phase_index: usize) -> i32 {
    layer_ids[phase_index.min(layer_ids.len() - 1)]
}

/// Rotates by the motor's `rotation_deg` then translates by its placement
/// center, converting motor-local mm into board nm.
fn to_board_point(config: &MotorConfig, p: Point2) -> kicad_proto::Vector2 {
    let angle = config.placement.rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = p.x_mm * cos - p.y_mm * sin;
    let y = p.x_mm * sin + p.y_mm * cos;
    kicad_proto::Vector2 {
        x_nm: mm_to_nm(x + config.placement.center_x_mm),
        y_nm: mm_to_nm(y + config.placement.center_y_mm),
    }
}

fn is_outer_layer(layer_id: i32) -> bool {
    layer_id == kicad_proto::BOARD_LAYER_F_CU || layer_id == kicad_proto::BOARD_LAYER_B_CU
}

/// Via type + padstack type for a transition between two layers.
///
/// Via type is the straightforward part: both layers outer -> through,
/// exactly one outer -> blind, neither -> buried.
///
/// Padstack type is NOT simply "outer involved -> normal, else custom" —
/// that was the first hypothesis and it's wrong. Confirmed live against a
/// real 6-layer board (`probe_blind_via.rs`, `probe_buried_via.rs`,
/// `probe_bcu_variants.rs`): `PST_NORMAL` works for F.Cu-B.Cu (through) and
/// F.Cu-inner (blind), but KiCad rejects it ("could not unpack PCB_VIA")
/// for BOTH B.Cu-inner (blind) and inner-inner (buried) — `PST_CUSTOM` is
/// required there instead. So the real rule is specifically about F.Cu, not
/// "any outer layer": `PST_NORMAL` iff `BOARD_LAYER_F_CU` is one of the two
/// layers, `PST_CUSTOM` otherwise. Whatever KiCad 10.0.5 is validating
/// internally, it treats F.Cu and B.Cu asymmetrically here.
fn via_type_and_pad_stack_type(layer_a_id: i32, layer_b_id: i32) -> (i32, i32) {
    let via_type = match (is_outer_layer(layer_a_id), is_outer_layer(layer_b_id)) {
        (true, true) => kicad_proto::VIA_TYPE_THROUGH,
        (true, false) | (false, true) => kicad_proto::VIA_TYPE_BLIND,
        (false, false) => kicad_proto::VIA_TYPE_BURIED,
    };
    let involves_f_cu = layer_a_id == kicad_proto::BOARD_LAYER_F_CU || layer_b_id == kicad_proto::BOARD_LAYER_F_CU;
    let pad_stack_type = if involves_f_cu {
        kicad_proto::PAD_STACK_TYPE_NORMAL
    } else {
        kicad_proto::PAD_STACK_TYPE_CUSTOM
    };
    (via_type, pad_stack_type)
}

fn push_track(
    items: &mut Vec<prost_types::Any>,
    start: kicad_proto::Vector2,
    end: kicad_proto::Vector2,
    width_nm: i64,
    layer_id: i32,
    net_name: &str,
) {
    let track = Track {
        id: None,
        start: Some(start),
        end: Some(end),
        width: Some(kicad_proto::Distance { value_nm: width_nm }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        layer: layer_id,
        net: Some(kicad_proto::Net { code: None, name: net_name.to_string() }),
    };
    items.push(kicad_proto::pack_any(&track, kicad_proto::TRACK_TYPE_URL));
}

fn push_via(
    items: &mut Vec<prost_types::Any>,
    position: kicad_proto::Vector2,
    layer_a_id: i32,
    layer_b_id: i32,
    drill_nm: i64,
    diameter_nm: i64,
    net_name: &str,
) {
    let (low, high) = if layer_a_id <= layer_b_id {
        (layer_a_id, layer_b_id)
    } else {
        (layer_b_id, layer_a_id)
    };
    let (via_type, pad_stack_type) = via_type_and_pad_stack_type(low, high);

    let via = Via {
        id: None,
        position: Some(position),
        pad_stack: Some(PadStack {
            r#type: pad_stack_type,
            layers: vec![low, high],
            drill: Some(DrillProperties {
                start_layer: low,
                end_layer: high,
                diameter: Some(kicad_proto::Vector2 { x_nm: drill_nm, y_nm: drill_nm }),
                shape: kicad_proto::DRILL_SHAPE_CIRCLE,
            }),
            unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
            copper_layers: vec![PadStackLayer {
                layer: low,
                shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE,
                size: Some(kicad_proto::Vector2 { x_nm: diameter_nm, y_nm: diameter_nm }),
            }],
        }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        net: Some(kicad_proto::Net { code: None, name: net_name.to_string() }),
        r#type: via_type,
    };
    items.push(kicad_proto::pack_any(&via, kicad_proto::VIA_TYPE_URL));
}

/// A coil's series-chain `(entry, layer)` / `(exit, layer)` terminals.
///
/// `coil_shape.rs` places both of a coil's outer bus terminals
/// symmetrically about its own center angle, and layer mirroring
/// (`generate_coil_paths`) puts one of them at `paths[0].start()` (layer
/// 0's start) and the other at `paths[last].end()` (the last layer's
/// end) — always on opposite sides of center, but *which side* each one
/// lands on flips between `Forward` and `Reversed` coils, since
/// `Reversed` mirrors the whole shape before this is called (see
/// `build_items`'s per-coil `coil_params`). Picking entry/exit by fixed
/// physical role (start = entry, end = exit) — the previous behavior —
/// therefore puts a `Forward` coil's entry on the far side of its own
/// exit from the chain's direction of travel: the bus arc feeding its
/// entry and the bus arc leaving its exit both have to cross the same
/// stretch of the phase bus to get there, physically overlapping and
/// shorting the coil's entry straight to its exit, bypassing its actual
/// winding. Picking by *angular position relative to `center_rad`*
/// instead — entry is whichever physical terminal sits behind center
/// (toward the previous coil), exit whichever sits ahead (toward the
/// next) — keeps every coil's bus arcs on their own non-overlapping
/// stretch of the ring regardless of winding sense.
fn coil_terminals(paths: &[CoilLayerPath], start_layer_id: i32, end_layer_id: i32, center_rad: f64) -> ((Point2, i32), (Point2, i32)) {
    let start = (paths[0].start(), start_layer_id);
    let end = (paths[paths.len() - 1].end(), end_layer_id);
    let start_delta = shortest_angle_delta(center_rad, point_angle_rad(start.0));
    let end_delta = shortest_angle_delta(center_rad, point_angle_rad(end.0));
    if start_delta <= end_delta {
        (start, end)
    } else {
        (end, start)
    }
}

/// Connects two points via a phase's dedicated bus layer, staying entirely
/// at or outside `bus_radius_mm` (chosen past every coil's outer radius, so
/// this never crosses any coil's own turns on any layer — turns only exist
/// between the coil's inner and outer radius). This replaced a first-pass
/// version that just drew a straight track directly between the two points
/// on layer 0: straight lines between non-adjacent-in-angle same-phase
/// coils cut straight across the intervening coils (phase groups are coil
/// index mod 3, so phase A's coils are 3 apart — a direct line from coil 0
/// to coil 3 crosses right through coils 1 and 2's copper on every layer).
/// That version also had a real correctness bug: a coil's exit terminal is
/// on whichever physical layer its winding sense put it on (entry and exit
/// are NOT consistently "layer 0" and "last layer" — sense can swap them),
/// but the old code always drew the connector on layer 0 regardless,
/// silently leaving some connections electrically disconnected.
///
/// Geometry, in the motor's local frame: if `from_layer` isn't already
/// `bus_layer`, a short radial stub (`via_stub_mm`, still on `from_layer`)
/// out from `from` before the via bridging onto the bus layer — `from` is
/// one of this *same coil's own* trace endpoints, not a point already
/// clear of its copper, so a via dropped right there is a through-via
/// landing on top of (or immediately next to) this coil's own nearby
/// turns; stepping it out first gives it room to actually clear them. Then
/// a radial track out to `bus_radius_mm`, an arc at `bus_radius_mm` over to
/// `to`'s angle (shortest direction), a radial track back in, and the same
/// stub-then-via pattern on the way off the bus if `to_layer` isn't
/// `bus_layer`. `to_layer: None` means `to` isn't a coil terminal (the
/// phase's final termination point) — no via or stub there, the bus track
/// just ends at that point on `bus_layer`.
///
/// Two widths, not one: `lead_trace_width_nm` for everything that's still
/// close to a coil's own copper (the stub, and the radial run in/out to
/// `bus_radius_mm`) — that's tightly-spaced turn territory
/// (`turn_pitch_mm` apart), so it stays at the coil's own trace width no
/// matter how much room the bus has elsewhere. `ring_trace_width_nm` is
/// only for the arc at `bus_radius_mm` itself, out past every coil's edge
/// where there's genuinely space to spare — see `bus_trace_width_mm`'s
/// doc comment on `MotorConfig`.
///
/// Every via this creates spans F.Cu to B.Cu (always a through via),
/// regardless of which two layers are actually electrically relevant
/// (`from_layer`/`bus_layer`/`to_layer` only pick where the *tracks*
/// land) — a blind or buried via here connects fine electrically, but
/// Kevin wants through vias uniformly (simpler and cheaper to fab than
/// mixing via types across one board), and it sidesteps whatever KiCad
/// was doing differently with blind/buried annular rings that was making
/// them run into the bus ring (see `bus_radius_mm`'s own margin, below,
/// for the other half of that fix).
#[allow(clippy::too_many_arguments)]
fn push_bus_connection(
    items: &mut Vec<prost_types::Any>,
    config: &MotorConfig,
    from: Point2,
    from_layer: i32,
    to: Point2,
    to_layer: Option<i32>,
    bus_layer: i32,
    bus_radius_mm: f64,
    arc_segments: u32,
    lead_trace_width_nm: i64,
    ring_trace_width_nm: i64,
    via_drill_nm: i64,
    via_diameter_nm: i64,
    via_stub_mm: f64,
    net_name: &str,
) {
    let from_angle = point_angle_rad(from);
    let to_angle = point_angle_rad(to);

    let bus_entry = if from_layer != bus_layer {
        let from_r = (from.x_mm.powi(2) + from.y_mm.powi(2)).sqrt();
        let stub_point = point_at_radius_angle(from_r + via_stub_mm, from_angle);
        push_track(items, to_board_point(config, from), to_board_point(config, stub_point), lead_trace_width_nm, from_layer, net_name);
        push_via(items, to_board_point(config, stub_point), kicad_proto::BOARD_LAYER_F_CU, kicad_proto::BOARD_LAYER_B_CU, via_drill_nm, via_diameter_nm, net_name);
        stub_point
    } else {
        from
    };

    let from_bus = point_at_radius_angle(bus_radius_mm, from_angle);
    push_track(items, to_board_point(config, bus_entry), to_board_point(config, from_bus), lead_trace_width_nm, bus_layer, net_name);

    let delta = shortest_angle_delta(from_angle, to_angle);
    let segments = arc_segments.max(1);
    let mut previous = from_bus;
    for step in 1..=segments {
        let t = step as f64 / segments as f64;
        let point = point_at_radius_angle(bus_radius_mm, from_angle + delta * t);
        push_track(items, to_board_point(config, previous), to_board_point(config, point), ring_trace_width_nm, bus_layer, net_name);
        previous = point;
    }

    match to_layer {
        Some(layer) if layer != bus_layer => {
            let to_r = (to.x_mm.powi(2) + to.y_mm.powi(2)).sqrt();
            let stub_point = point_at_radius_angle(to_r + via_stub_mm, to_angle);
            push_track(items, to_board_point(config, previous), to_board_point(config, stub_point), lead_trace_width_nm, bus_layer, net_name);
            push_via(items, to_board_point(config, stub_point), kicad_proto::BOARD_LAYER_F_CU, kicad_proto::BOARD_LAYER_B_CU, via_drill_nm, via_diameter_nm, net_name);
            push_track(items, to_board_point(config, stub_point), to_board_point(config, to), lead_trace_width_nm, layer, net_name);
        }
        _ => {
            push_track(items, to_board_point(config, previous), to_board_point(config, to), lead_trace_width_nm, bus_layer, net_name);
        }
    }
}

/// Builds every track/via `prost_types::Any` payload for one motor, without
/// talking to KiCad — split out from `write_motor` so geometry/winding
/// logic here stays testable without a live connection.
fn build_items(
    config: &MotorConfig,
    arc_segments: u32,
    scope: &GenerationScope,
) -> Result<Vec<prost_types::Any>, WriteError> {
    let geometry_params = config.to_geometry_params(arc_segments);
    let assignments = winding_map::assign_coils(config.coils, config.poles);
    let chains = winding_map::build_phase_chains(&assignments);

    let layer_ids: Vec<i32> = config
        .layers
        .iter()
        .map(|name| {
            BoardLayerInfo::id_from_name(name).ok_or_else(|| WriteError::UnknownLayer(name.clone()))
        })
        .collect::<Result<_, _>>()?;

    let trace_width_nm = mm_to_nm(config.trace_width_mm);
    let bus_trace_width_nm = mm_to_nm(config.bus_trace_width_mm);
    let via_drill_nm = mm_to_nm(config.via.drill_mm);
    let via_diameter_nm = mm_to_nm(config.via.diameter_mm);
    // How far a coil-terminal via steps out (still on the coil's own
    // layer) before dropping onto the bus — see `push_bus_connection`'s
    // doc comment for why landing it right at the terminal is the
    // problem. A full via diameter's worth of clearance from a point
    // that's already right next to this same coil's own nearest turns.
    let via_stub_mm = config.via.diameter_mm;

    let outer_radius_mm = config.mean_radius_mm + config.active_radial_length_mm / 2.0;
    // Past every coil's outer edge, with enough margin to clear the via
    // that bridges a coil terminal onto the bus (`via_stub_mm` out from
    // the coil's own edge, plus the via's own footprint) — and with real
    // slack left over, not just the bare minimum: the previous margin
    // (`via.diameter_mm * 2.0`) worked out to exactly `trace_spacing_mm`
    // of clearance between the via's edge and the bus track's edge —
    // the board's own minimum spacing, with zero room for anything
    // else — and Kevin found the bus ring actually running into the
    // vias on the live board. This adds a full extra via diameter of
    // slack on top of that minimum.
    let via_footprint_radius_mm = config.via.diameter_mm / 2.0;
    let bus_radius_mm = outer_radius_mm
        + via_stub_mm
        + via_footprint_radius_mm
        + config.via.diameter_mm
        + config.trace_spacing_mm
        + config.bus_trace_width_mm / 2.0;
    // Bus arcs can span several coil-widths (phase groups are 3 coils
    // apart); use a finer resolution than per-turn arcs so long sweeps
    // still look like arcs rather than a coarse polygon.
    let bus_arc_segments = arc_segments.max(24);

    let start_layer_id = layer_ids[0];
    let end_layer_id = layer_ids[layer_ids.len() - 1];

    let mut items = Vec::new();

    for (phase_index, chain) in chains.iter().enumerate() {
        if !scope.phases.contains(&chain.phase) {
            continue;
        }

        let net_name = format!("{}_{}", config.name.to_uppercase(), phase_letter(chain.phase));
        let bus_layer = bus_layer_for_phase_index(&layer_ids, phase_index);
        // Seeded with the phase's *start* termination (already "on" the
        // bus layer, same as the end termination is treated — an external
        // connector pad, not tied to any particular copper layer) so the
        // loop below wires it to the first coil's entry via the exact
        // same `push_bus_connection` call every other inter-coil
        // connection uses. Without this, the first coil in every phase
        // chain had its entry terminal wired to nothing at all.
        let mut previous_exit: Option<(Point2, i32)> =
            scope.include_bus.then(|| (phase_start_termination_point(config, chain.phase), bus_layer));

        for &coil_index in &chain.coil_order {
            let sense = assignments
                .iter()
                .find(|a| a.coil_index == coil_index)
                .expect("winding_map assigns every coil index it hands back in a chain")
                .sense;

            // A `Reversed`-sense coil needs its flux contribution flipped
            // relative to a `Forward` one — done here by mirroring the
            // coil's own shape (flipping the *effective* winding
            // direction just for this coil), which is electrically
            // equivalent to reversing current through a fixed shape (both
            // flip the same sign). Which physical terminal (start vs end)
            // ends up playing entry vs exit still has to be worked out per
            // coil, though — mirroring swaps which side of center each one
            // lands on — see `coil_terminals`'s doc comment.
            let coil_params = coil_geometry::CoilGeometryParams {
                layer0_direction: match sense {
                    WindingSense::Forward => geometry_params.layer0_direction,
                    WindingSense::Reversed => geometry_params.layer0_direction.flipped(),
                },
                ..geometry_params
            };
            let paths = coil_geometry::generate_coil_paths(&coil_params, coil_index)?;

            for (layer_index, path) in paths.iter().enumerate() {
                if scope.layer_indices.contains(&(layer_index as u32)) {
                    let layer_id = layer_ids[layer_index];
                    for pair in path.points.windows(2) {
                        let start = to_board_point(config, pair[0]);
                        let end = to_board_point(config, pair[1]);
                        push_track(&mut items, start, end, trace_width_nm, layer_id, &net_name);
                    }
                }
            }

            if !scope.include_bus {
                continue;
            }

            let center_rad = coil_geometry::coil_center_angle_rad(coil_index, config.coils);
            let ((entry, entry_layer), (exit, exit_layer)) =
                coil_terminals(&paths, start_layer_id, end_layer_id, center_rad);

            if let Some((prev_exit, prev_exit_layer)) = previous_exit {
                push_bus_connection(
                    &mut items,
                    config,
                    prev_exit,
                    prev_exit_layer,
                    entry,
                    Some(entry_layer),
                    bus_layer,
                    bus_radius_mm,
                    bus_arc_segments,
                    trace_width_nm,
                    bus_trace_width_nm,
                    via_drill_nm,
                    via_diameter_nm,
                    via_stub_mm,
                    &net_name,
                );
            }
            previous_exit = Some((exit, exit_layer));
        }

        if scope.include_bus {
            if let Some((prev_exit, prev_exit_layer)) = previous_exit {
                let termination = phase_termination_point(config, chain.phase);
                push_bus_connection(
                    &mut items,
                    config,
                    prev_exit,
                    prev_exit_layer,
                    termination,
                    None,
                    bus_layer,
                    bus_radius_mm,
                    bus_arc_segments,
                    trace_width_nm,
                    bus_trace_width_nm,
                    via_drill_nm,
                    via_diameter_nm,
                    via_stub_mm,
                    &net_name,
                );
            }
        }
    }

    Ok(items)
}

/// Generates and writes one motor's winding to the live board, batched
/// under a single commit. `scope` controls how much gets written — pass
/// `GenerationScope::full(config.layers.len() as u32)` for everything, or
/// a narrower scope to build the board up incrementally for visual
/// checking (see `GenerationScope`'s doc comment).
pub fn write_motor(
    client: &KiCadClientBlocking,
    config: &MotorConfig,
    arc_segments: u32,
    scope: &GenerationScope,
) -> Result<WriteReport, WriteError> {
    let items = build_items(config, arc_segments, scope)?;

    let commit = client.begin_commit()?;
    let created = match client.create_items(items, None) {
        Ok(created) => created,
        Err(err) => {
            let _ = client.end_commit(commit, CommitAction::Drop, format!("{} winding failed", config.name));
            return Err(err.into());
        }
    };
    client.end_commit(commit, CommitAction::Commit, format!("generate {} stator winding", config.name))?;

    Ok(WriteReport {
        motor_name: config.name.clone(),
        created_item_count: created.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MotorConfig, PhaseTerminations, PointMm, Placement, ViaConfig, WindingDirectionConfig};

    fn two_layer_config() -> MotorConfig {
        MotorConfig {
            name: "test".to_string(),
            placement: Placement { center_x_mm: 10.0, center_y_mm: -5.0, rotation_deg: 15.0 },
            poles: 8,
            coils: 9,
            mean_radius_mm: 7.0,
            active_radial_length_mm: 6.0,
            trace_width_mm: 0.15,
            trace_spacing_mm: 0.1,
            bus_trace_width_mm: 0.3,
            copper_weight_oz: None,
            layers: vec!["F.Cu".to_string(), "B.Cu".to_string()],
            layer0_direction: WindingDirectionConfig::Ccw,
            via: ViaConfig { drill_mm: 0.3, diameter_mm: 0.6 },
            phase_terminations: PhaseTerminations {
                a: PointMm { x_mm: 13.0, y_mm: 0.0 },
                b: PointMm { x_mm: -6.5, y_mm: 11.26 },
                c: PointMm { x_mm: -6.5, y_mm: -11.26 },
            },
            phase_start_terminations: PhaseTerminations {
                a: PointMm { x_mm: 14.28, y_mm: 2.52 },
                b: PointMm { x_mm: -9.32, y_mm: 11.11 },
                c: PointMm { x_mm: -4.96, y_mm: -13.63 },
            },
        }
    }

    #[test]
    fn builds_items_without_touching_kicad() {
        let config = two_layer_config();
        let items = build_items(&config, 12, &GenerationScope::full(config.layer_count())).unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.type_url == kicad_proto::TRACK_TYPE_URL));
        assert!(items.iter().any(|i| i.type_url == kicad_proto::VIA_TYPE_URL));
    }

    #[test]
    fn scope_limits_phases_layers_and_bus_independently() {
        let config = two_layer_config();

        let phase_a_layer0_only = GenerationScope {
            phases: vec![Phase::A],
            layer_indices: vec![0],
            include_bus: false,
        };
        let items = build_items(&config, 12, &phase_a_layer0_only).unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.type_url == kicad_proto::TRACK_TYPE_URL));

        let bus_only = GenerationScope {
            phases: vec![Phase::A, Phase::B, Phase::C],
            layer_indices: vec![],
            include_bus: true,
        };
        let items = build_items(&config, 12, &bus_only).unwrap();
        // include_bus's own straight/arc segments are tracks too, so this
        // scope isn't empty, but the per-turn coil geometry (which needs a
        // non-empty layer_indices) must be absent.
        let per_turn_track_count = build_items(&config, 12, &GenerationScope::full(config.layer_count()))
            .unwrap()
            .iter()
            .filter(|i| i.type_url == kicad_proto::TRACK_TYPE_URL)
            .count();
        let this_track_count = items.iter().filter(|i| i.type_url == kicad_proto::TRACK_TYPE_URL).count();
        assert!(this_track_count < per_turn_track_count);
    }

    #[test]
    fn outer_to_outer_transition_is_a_through_via() {
        assert_eq!(
            via_type_and_pad_stack_type(kicad_proto::BOARD_LAYER_F_CU, kicad_proto::BOARD_LAYER_B_CU),
            (kicad_proto::VIA_TYPE_THROUGH, kicad_proto::PAD_STACK_TYPE_NORMAL)
        );
    }

    #[test]
    fn f_cu_to_inner_transition_is_a_blind_via_with_normal_pad_stack() {
        let in1 = BoardLayerInfo::id_from_name("In1.Cu").unwrap();
        assert_eq!(
            via_type_and_pad_stack_type(kicad_proto::BOARD_LAYER_F_CU, in1),
            (kicad_proto::VIA_TYPE_BLIND, kicad_proto::PAD_STACK_TYPE_NORMAL)
        );
    }

    #[test]
    fn coil_terminals_puts_entry_behind_and_exit_ahead_regardless_of_sense() {
        // Picking entry/exit by fixed physical role (start = entry, end =
        // exit) put a `Forward` coil's entry on the wrong side of its own
        // exit relative to the chain's direction of travel — the bus arc
        // feeding its entry and the arc leaving its exit both had to cross
        // the same stretch of the ring to get there, physically
        // overlapping and shorting the coil's entry straight to its exit.
        // `coil_terminals` must keep entry behind center (toward the
        // previous coil) and exit ahead (toward the next) no matter which
        // physical endpoint that turns out to be.
        let config = two_layer_config();
        let arc_segments = 12;
        let geometry_params = config.to_geometry_params(arc_segments);
        let assignments = winding_map::assign_coils(config.coils, config.poles);
        let layer_ids: Vec<i32> = config.layers.iter().map(|n| BoardLayerInfo::id_from_name(n).unwrap()).collect();
        let start_layer_id = layer_ids[0];
        let end_layer_id = layer_ids[layer_ids.len() - 1];

        for a in &assignments {
            let coil_params = coil_geometry::CoilGeometryParams {
                layer0_direction: match a.sense {
                    WindingSense::Forward => geometry_params.layer0_direction,
                    WindingSense::Reversed => geometry_params.layer0_direction.flipped(),
                },
                ..geometry_params
            };
            let paths = coil_geometry::generate_coil_paths(&coil_params, a.coil_index).unwrap();
            let center_rad = coil_geometry::coil_center_angle_rad(a.coil_index, config.coils);
            let ((entry, _), (exit, _)) = coil_terminals(&paths, start_layer_id, end_layer_id, center_rad);
            let entry_delta = shortest_angle_delta(center_rad, point_angle_rad(entry));
            let exit_delta = shortest_angle_delta(center_rad, point_angle_rad(exit));
            assert!(
                entry_delta <= exit_delta,
                "coil{} ({:?}): entry ({entry_delta}) should be behind exit ({exit_delta})",
                a.coil_index,
                a.sense
            );
        }
    }

    #[test]
    fn b_cu_to_inner_transition_is_a_blind_via_with_custom_pad_stack() {
        // Confirmed live (probe_bcu_variants.rs): B.Cu is NOT symmetric with
        // F.Cu here — PST_NORMAL is rejected for B.Cu-inner, unlike F.Cu-inner.
        let in4 = BoardLayerInfo::id_from_name("In4.Cu").unwrap();
        assert_eq!(
            via_type_and_pad_stack_type(in4, kicad_proto::BOARD_LAYER_B_CU),
            (kicad_proto::VIA_TYPE_BLIND, kicad_proto::PAD_STACK_TYPE_CUSTOM)
        );
    }

    #[test]
    fn inner_to_inner_transition_is_a_buried_via_with_custom_pad_stack() {
        let in1 = BoardLayerInfo::id_from_name("In1.Cu").unwrap();
        let in2 = BoardLayerInfo::id_from_name("In2.Cu").unwrap();
        assert_eq!(
            via_type_and_pad_stack_type(in1, in2),
            (kicad_proto::VIA_TYPE_BURIED, kicad_proto::PAD_STACK_TYPE_CUSTOM)
        );
    }

    #[test]
    fn unknown_layer_name_is_reported_before_any_geometry_work() {
        let mut config = two_layer_config();
        config.layers = vec!["Not.A.Real.Layer".to_string()];
        let err = build_items(&config, 12, &GenerationScope::full(config.layer_count())).unwrap_err();
        assert!(matches!(err, WriteError::UnknownLayer(name) if name == "Not.A.Real.Layer"));
    }

    #[test]
    fn bus_connection_only_adds_a_via_when_the_terminal_isnt_already_on_the_bus_layer() {
        let config = two_layer_config();
        let f_cu = kicad_proto::BOARD_LAYER_F_CU;
        let b_cu = kicad_proto::BOARD_LAYER_B_CU;
        let from = Point2 { x_mm: 10.0, y_mm: 0.0 };
        let to = Point2 { x_mm: -5.0, y_mm: 8.66 };

        let count_vias = |from_layer: i32, to_layer: Option<i32>, bus_layer: i32| {
            let mut items = Vec::new();
            push_bus_connection(&mut items, &config, from, from_layer, to, to_layer, bus_layer, 20.0, 8, 150_000, 300_000, 300_000, 600_000, 1.0, "TEST");
            items.iter().filter(|i| i.type_url == kicad_proto::VIA_TYPE_URL).count()
        };

        // Both terminals already on the bus layer: no bridging vias.
        assert_eq!(count_vias(f_cu, Some(f_cu), f_cu), 0);
        // Only the "to" terminal needs bridging.
        assert_eq!(count_vias(f_cu, Some(b_cu), f_cu), 1);
        // Only the "from" terminal needs bridging.
        assert_eq!(count_vias(b_cu, Some(f_cu), f_cu), 1);
        // Both need bridging (bus layer matches neither).
        assert_eq!(count_vias(b_cu, Some(b_cu), f_cu), 2);
        // Termination point (`to_layer: None`) never gets a via, even when
        // "from" needs one.
        assert_eq!(count_vias(b_cu, None, f_cu), 1);
    }

    #[test]
    fn bus_connection_stays_at_or_beyond_bus_radius_except_at_its_two_endpoints() {
        // Zeroed placement so board-space coordinates are directly
        // comparable to motor-local radii without undoing a transform.
        let mut config = two_layer_config();
        config.placement = Placement { center_x_mm: 0.0, center_y_mm: 0.0, rotation_deg: 0.0 };
        let from = Point2 { x_mm: 10.0, y_mm: 0.0 };
        let to = Point2 { x_mm: -5.0, y_mm: 8.66 };
        let bus_radius = 20.0;

        let mut items = Vec::new();
        push_bus_connection(
            &mut items, &config, from, kicad_proto::BOARD_LAYER_F_CU, to, None,
            kicad_proto::BOARD_LAYER_F_CU, bus_radius, 8, 150_000, 300_000, 300_000, 600_000, 1.0, "TEST",
        );

        for item in &items {
            if item.type_url != kicad_proto::TRACK_TYPE_URL {
                continue;
            }
            let track: Track = prost::Message::decode(item.value.as_slice()).unwrap();
            for endpoint in [track.start.unwrap(), track.end.unwrap()] {
                let radius_mm = ((endpoint.x_nm as f64).powi(2) + (endpoint.y_nm as f64).powi(2)).sqrt() / 1_000_000.0;
                assert!(
                    radius_mm >= bus_radius - 1e-6 || radius_mm <= from.x_mm.hypot(from.y_mm).max(to.x_mm.hypot(to.y_mm)) + 1e-6,
                    "bus track endpoint at radius {radius_mm}mm is inside the bus radius ({bus_radius}mm) \
                     without being one of the connection's own two terminals"
                );
            }
        }
    }
}
