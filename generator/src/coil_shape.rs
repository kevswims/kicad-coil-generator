//! **This is the file to edit if you want to change what a coil's turns
//! look like.** Everything else (angular-wedge safety, mirroring for
//! alternating winding direction, phase/bus wiring, talking to KiCad) is
//! handled elsewhere and doesn't need to change — see `coil_geometry.rs`'s
//! module doc comment for how the pieces fit together.
//!
//! ## The contract
//!
//! [`generate_coil_shape`] describes ONE physical layer's turns, in that
//! coil's own local 2D frame:
//!
//! - `radius_mm` — distance from the motor's center.
//! - `tangential_offset_mm` — signed distance from the coil's own center
//!   line, measured along the arc at that radius (positive is
//!   counterclockwise, but the sign convention doesn't actually matter —
//!   nothing downstream cares which side is which).
//! - `layer_index` — which physical layer (0-based, 0 = the motor's first
//!   configured layer) you're being asked for. `coil_geometry.rs` calls
//!   this function once per layer, so every layer can look different if
//!   you want (e.g. a different via-hookup lead-in per layer) — and it
//!   mirrors whatever odd-numbered layers return so they still wind the
//!   same rotational sense as the even ones — see its own doc comment —
//!   so you don't need any winding-direction bookkeeping of your own,
//!   regardless of how different you make each layer's shape.
//!
//! You don't need to worry about:
//! - **Which coil this is, or its rotation.** `coil_geometry.rs` places
//!   your shape at this specific coil's own center angle after you
//!   return it.
//! - **Crossing into a neighboring coil.** Every point's
//!   `tangential_offset_mm` gets clamped to whatever's actually safe at
//!   that specific radius before it's placed on the board — ask for more
//!   than what's available and you'll just get flattened against that
//!   boundary, not an error. If part of your shape looks unexpectedly
//!   flat, that's usually why; `params.mean_radius_mm`,
//!   `params.active_radial_length_mm`, and `params.coils` together
//!   determine how much room is actually available at a given radius.
//! - **Winding direction, or the other 5 layers.** Layer 0 is drawn
//!   exactly as you return it (or its mirror image, if the motor config
//!   says so); layers 1, 2, 3... alternate automatically so every layer's
//!   contribution to the magnetic field adds up instead of canceling.
//!
//! What you're responsible for:
//! - **Self-overlap.** Nothing checks that your own turns don't cross
//!   each other — use `bin/check_overlap.rs` (numeric) and
//!   `bin/dump_coil_svg.rs` (visual, no KiCad needed) to check as you
//!   iterate.
//! - **Turn-to-turn spacing.** `turn_pitch_mm` (`trace_width_mm +
//!   trace_spacing_mm`) is the minimum center-to-center distance two of
//!   your own turns should ever get to, if you don't want them touching.
//! - **Where the shape starts and ends.** The first point you return
//!   becomes this coil's outer bus-wiring terminal — keep it near the
//!   true outer radius (`mean_radius_mm + active_radial_length_mm / 2`)
//!   so the bus routing stays sensible. The last point is where the
//!   layer-transition via to layer 1 will go by hand — leaving
//!   `params.min_inner_gap_mm` of real clearance from the center there
//!   gives that via somewhere to actually sit.
//!
//! ## Iterating
//!
//! `cargo run --bin dump_coil_svg` renders your shape straight to SVG —
//! no KiCad connection needed, fast to loop on. Once it looks right,
//! `cargo run --bin check_overlap` checks real clearances numerically,
//! and `generate_stator --config feed_motor.toml` pushes it to the live
//! board.

use crate::coil_geometry::CoilGeometryParams;

/// Turn-to-turn pitch: the center-to-center distance between adjacent
/// turns that gives them exactly `trace_spacing_mm` of clear copper
/// between them (each turn's own `trace_width_mm` eats into the pitch
/// from its side).
pub fn turn_pitch_mm(params: &CoilGeometryParams) -> f64 {
    params.trace_width_mm + params.trace_spacing_mm
}

/// Starter shape: a nested meander — each turn sweeps almost all the way
/// across the coil's angular wedge at a roughly constant radius (a
/// "rung"), then steps `turn_pitch_mm` inward to the next rung,
/// alternating which side it sweeps from. A rung's own half-width is
/// scaled to the *local* safe angular room at its own radius
/// (`SAFETY_MARGIN` short of the true limit, so it doesn't ride the exact
/// clamp boundary) — wider near the outer radius, narrowing toward the
/// inner one.
///
/// This is a deliberately simple, verified-clean (`check_overlap`: 0
/// violating pairs against both motors' real parameters) starting point,
/// not a from-scratch attempt at the classic tightly-nested round spiral
/// look — getting *that* right without reintroducing self-crossings took
/// several failed tries (see the plan file for what didn't work and why),
/// and this project has already burned a lot of time chasing that one
/// shape. A meander is a completely normal, real inductor pattern in its
/// own right, especially in a constrained angular wedge like this one —
/// worth trying as-is before assuming it needs to look like the
/// reference photo's spiral. If you do want that tighter round-spiral
/// look: the core lesson from the failed attempts is that a turn's own
/// radial depth needs to stay comparable to its tangential width (a rung
/// with depth = `turn_pitch_mm` but width = several mm reads as a row of
/// needles once there are enough turns to matter) — and that if turns
/// re-cross the *same* tangential offset repeatedly (e.g. every turn
/// sharing one fixed half-width), their sides end up sitting on top of
/// each other rather than nesting, however that offset was chosen.
pub fn generate_coil_shape(
    params: &CoilGeometryParams,
    half_span_rad: f64,
    layer_index: u32,
) -> Vec<(f64, f64)> {
    const SAFETY_MARGIN: f64 = 0.95;
    let is_odd_layer = layer_index % 2 == 1;

    let mut outer_r = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let mut inner_r =
        params.mean_radius_mm - params.active_radial_length_mm / 2.0 + params.min_inner_gap_mm;
    let pitch = turn_pitch_mm(params);
    let half_w_at = |r: f64| r * half_span_rad.sin() * SAFETY_MARGIN;

    let mut points = Vec::new();

    let trace_space = params.trace_spacing_mm + params.trace_width_mm;

    let mut side_subtraction = 0.0;
    if is_odd_layer {
        points.push((
            outer_r + trace_space,
            -1.0 * half_w_at(outer_r + trace_space) + trace_space * 2.0,
        ));
        points.push((outer_r + trace_space, half_w_at(outer_r + trace_space)));
    }
    loop {
        points.push((outer_r, half_w_at(outer_r) - side_subtraction));
        points.push((inner_r, half_w_at(inner_r) - side_subtraction));
        points.push((inner_r, -1.0 * half_w_at(inner_r) + side_subtraction));
        points.push((outer_r, -1.0 * half_w_at(outer_r) + side_subtraction));
        points.push((
            outer_r,
            half_w_at(outer_r) - params.trace_spacing_mm - params.trace_width_mm - side_subtraction,
        ));

        outer_r -= params.trace_spacing_mm + params.trace_width_mm;
        inner_r += params.trace_spacing_mm + params.trace_width_mm;
        side_subtraction += params.trace_spacing_mm + params.trace_width_mm;

        if outer_r < inner_r
            || side_subtraction + params.trace_width_mm + params.trace_spacing_mm
                > half_w_at(inner_r)
        {
            break;
        }
    }

    if layer_index == 1 || layer_index == 2 {
        points.pop();
    }

    if layer_index == 4 || layer_index == 5 {
        points.pop();
        points.pop();
    }
    points
}
