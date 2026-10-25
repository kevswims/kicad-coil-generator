//! Per-coil, per-layer trace geometry for an axial-flux stator winding.
//!
//! Pure math: given a motor's pole/coil/radius/turn parameters, produces the
//! polyline each coil traces on each layer. No KiCad dependency, so this is
//! fully exercised by `cargo test` alone.
//!
//! **The actual per-coil turn shape lives in `coil_shape.rs`, not here** —
//! that's the file to edit to change what a coil looks like; see its own
//! module doc comment for the contract. This module is the pipeline
//! *around* that shape, and shouldn't need to change just because the
//! shape does:
//!
//! - Each coil occupies an angular wedge centered on its own evenly-spaced
//!   angle (`coil_angular_span_rad`, `coil_center_angle_rad`) — sized so
//!   adjacent coils never touch, however wide `coil_shape.rs` asks to be.
//! - `place_point` maps a `coil_shape.rs`-space `(radius_mm,
//!   tangential_offset_mm)` pair onto the board, clamping the tangential
//!   offset to whatever's actually safe at that radius so a shape can't
//!   accidentally cross into a neighboring coil's wedge.
//! - Layer 0 is `coil_shape.rs`'s output as-is (or its mirror image, if
//!   the motor's configured `layer0_direction` is clockwise); each later
//!   layer is derived by mirroring the previous shape across the ray at
//!   the coil's own fixed center angle and reversing its point order (see
//!   `mirror_layer_for_opposite_winding`) — which winds in the *same*
//!   rotational sense as the previous layer, so every layer's
//!   contribution to the coil's magnetic field reinforces the others
//!   instead of alternating and canceling. Layer-to-layer connections
//!   (vias) are placed manually in the KiCad GUI, not generated here.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x_mm: f64,
    pub y_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindingDirection {
    Clockwise,
    CounterClockwise,
}

impl WindingDirection {
    /// The opposite direction — lets a caller flip a motor's configured
    /// `layer0_direction` per coil (e.g. for a coil `winding_map` marked
    /// `Reversed`) without needing its own separate mirroring logic:
    /// mirroring the coil's own shape and reversing which way current
    /// flows through a fixed shape are electrically equivalent ways to
    /// flip its flux contribution, so reusing this one mechanism for both
    /// keeps every coil's series-chain wiring identical (always
    /// physical-start-in, physical-end-out) regardless of winding sense.
    pub fn flipped(self) -> Self {
        match self {
            WindingDirection::Clockwise => WindingDirection::CounterClockwise,
            WindingDirection::CounterClockwise => WindingDirection::Clockwise,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CoilGeometryParams {
    pub coils: u32,
    pub mean_radius_mm: f64,
    pub active_radial_length_mm: f64,
    pub trace_width_mm: f64,
    pub trace_spacing_mm: f64,
    pub layer_count: u32,
    /// Winding direction of layer 0; alternates on every subsequent layer.
    pub layer0_direction: WindingDirection,
    /// Straight-line segments used to approximate each arc. Not used by
    /// the current `coil_shape.rs` starter (its segments are already
    /// straight) — available for a custom shape that wants it.
    pub arc_segments: u32,
    /// Radial room (mm) to deliberately leave unused in the middle of
    /// every coil, so a hand-placed layer-transition via has somewhere to
    /// go — see `coil_shape.rs`'s module doc comment.
    pub min_inner_gap_mm: f64,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GeometryError {
    #[error(
        "active radial length {active_radial_length_mm}mm is too small to fit even one turn \
         of {trace_width_mm}mm trace"
    )]
    NoRoomForTurns {
        active_radial_length_mm: f64,
        trace_width_mm: f64,
    },
    #[error(
        "{coils} coils around a {mean_radius_mm}mm mean radius leave no angular room once \
         {trace_spacing_mm}mm inter-coil spacing is reserved"
    )]
    NoAngularRoomForCoils {
        coils: u32,
        mean_radius_mm: f64,
        trace_spacing_mm: f64,
    },
    #[error("coil index {index} is out of range for a {coils}-coil motor")]
    CoilIndexOutOfRange { index: u32, coils: u32 },
    #[error("layer index {index} is out of range for a {layer_count}-layer motor")]
    LayerIndexOutOfRange { index: u32, layer_count: u32 },
}

/// Radial pitch between successive turn centerlines.
fn turn_pitch_mm(params: &CoilGeometryParams) -> f64 {
    params.trace_width_mm + params.trace_spacing_mm
}

/// Angular width (radians) reserved as a gap between adjacent coils, sized
/// so their centerlines stay a full `turn_pitch_mm` apart (each coil's own
/// trace width eats half the gap from its side, leaving `trace_spacing_mm`
/// of actual clear copper — same accounting as spacing between two turns
/// of the same coil) — not just `trace_spacing_mm` on its own, which would
/// leave no room for either coil's trace width and let them touch.
/// Calibrated at the coil's *innermost* centerline radius, not the mean:
/// a fixed angular gap corresponds to less linear clearance at smaller
/// radius, so the inner radius is where the gap is tightest.
fn angular_gap_rad(params: &CoilGeometryParams) -> f64 {
    let inner_radius = params.mean_radius_mm - params.active_radial_length_mm / 2.0;
    let centerline_inner = inner_radius + params.trace_width_mm / 2.0;
    turn_pitch_mm(params) / centerline_inner
}

/// The angular width (radians) one coil's wedge occupies, after reserving
/// `angular_gap_rad` between it and its neighbors.
pub fn coil_angular_span_rad(params: &CoilGeometryParams) -> Result<f64, GeometryError> {
    let full_pitch_angle = 2.0 * PI / params.coils as f64;
    let span = full_pitch_angle - angular_gap_rad(params);
    if span <= 0.0 {
        return Err(GeometryError::NoAngularRoomForCoils {
            coils: params.coils,
            mean_radius_mm: params.mean_radius_mm,
            trace_spacing_mm: params.trace_spacing_mm,
        });
    }
    Ok(span)
}

/// Center angle (radians) of a coil's wedge, measured from the motor's local
/// +x axis, evenly spaced around the full circle.
pub fn coil_center_angle_rad(coil_index: u32, coils: u32) -> f64 {
    coil_index as f64 * 2.0 * PI / coils as f64
}

fn point_at(radius_mm: f64, angle_rad: f64) -> Point2 {
    Point2 {
        x_mm: radius_mm * angle_rad.cos(),
        y_mm: radius_mm * angle_rad.sin(),
    }
}

/// Reflects a point across the line through the origin at angle `axis_rad`
/// — used to mirror a counter-clockwise-wound trace into its clockwise
/// mirror image, rather than duplicating the spiral-generation logic for
/// each winding direction.
fn mirror_point_across_angle(p: Point2, axis_rad: f64) -> Point2 {
    let two_phi = 2.0 * axis_rad;
    Point2 {
        x_mm: p.x_mm * two_phi.cos() + p.y_mm * two_phi.sin(),
        y_mm: p.x_mm * two_phi.sin() - p.y_mm * two_phi.cos(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoilLayerPath {
    pub coil_index: u32,
    pub layer_index: u32,
    pub points: Vec<Point2>,
}

impl CoilLayerPath {
    pub fn start(&self) -> Point2 {
        self.points[0]
    }

    pub fn end(&self) -> Point2 {
        self.points[self.points.len() - 1]
    }
}

/// Angle at radius `r` for a point offset `offset_mm` (tangentially, from
/// `center`) — derived so that connecting two such points (same
/// `offset_mm`, different `r`) with a straight Cartesian segment gives an
/// exact straight line, parallel to the coil's center axis and offset
/// from it by exactly `offset_mm`, not merely an approximation of one.
/// This is what makes the radial sides *uniformly* spaced regardless of
/// radius — a version that narrowed by a constant *angle* per lap instead
/// (`pitch / reference_radius`) put the sides at constant angular
/// intervals, which are physically closer together at small radius than
/// at large radius (arc length shrinks with radius, angle doesn't) — the
/// "too tight toward the inside" Kevin spotted.
fn tangential_offset_angle(offset_mm: f64, r: f64) -> f64 {
    (offset_mm / r).clamp(-1.0, 1.0).asin()
}

/// Maps one `coil_shape.rs`-space `(radius_mm, tangential_offset_mm)`
/// point onto the board: `tangential_offset_mm` becomes an exact angular
/// offset at `radius_mm` via `tangential_offset_angle` — clamped to what
/// the coil's own angular wedge allows at that specific radius, so no
/// custom shape can accidentally cross into a neighboring coil's
/// territory regardless of what it asks for.
fn place_point(radius_mm: f64, tangential_offset_mm: f64, center_rad: f64, half_span_rad: f64) -> Point2 {
    let max_offset = radius_mm * half_span_rad.sin();
    let clamped = tangential_offset_mm.clamp(-max_offset, max_offset);
    point_at(radius_mm, center_rad + tangential_offset_angle(clamped, radius_mm))
}

/// Produces the opposite-winding-sense counterpart of a layer's trace,
/// continuing the same rotational sense of current flow rather than
/// reversing it.
///
/// Reversing point order alone (traverse the identical physical shape
/// backwards) is *not* enough to make consecutive layers wind the same
/// way: current flowing along a fixed curve one direction versus the
/// other has opposite senses (CW vs CCW, as viewed from one fixed side
/// of the board), and by the right-hand rule that flips which way each
/// layer's contribution to the coil's magnetic field points. Layers that
/// alternate field direction cancel each other's contribution instead of
/// adding to it — a real electromagnetic bug, not a cosmetic one.
///
/// Reflecting the shape *before* reversing the order restores the same
/// sense: mirroring turns a CCW trace into a CW trace for the same
/// traversal direction, and then reversing the traversal direction turns
/// it back to CCW — the two flips cancel, so the mirrored layer, walked
/// from its own start to its own end, winds the same way the original
/// did.
///
/// Mirrors across the ray at the coil's fixed `center` angle — not each
/// layer's own ending angle. A first version mirrored around each
/// layer's own connection point specifically to keep that point exactly
/// fixed (reflecting a point across a ray through itself is a no-op).
/// That works for exactly one transition, but chaining it (mirroring
/// layer 2 around layer 1's endpoint, layer 3 around layer 2's, …)
/// compounds: composing reflections around two different axes is a
/// rotation by twice the angle between them, so each additional
/// transition rotates the whole trace further — measured numerically, it
/// diverged past the coil's own wedge by layer 2 and kept getting worse.
/// Mirroring around the same fixed axis every time is stable (it's
/// literally periodic: mirroring twice around the same axis is the
/// identity), at the cost of the connection point no longer landing
/// exactly on the previous layer's endpoint — `kicad_writer` bridges that
/// gap with a short trace alongside each layer-transition via.
fn mirror_layer_for_opposite_winding(points: &[Point2], center: f64) -> Vec<Point2> {
    points.iter().rev().map(|p| mirror_point_across_angle(*p, center)).collect()
}

/// Generates one coil's full layer stack. `coil_shape.rs` is asked for a
/// separate shape per physical layer (passing `layer_index` so it can
/// shape each one differently — e.g. a different via-hookup lead-in per
/// layer — rather than being forced to reuse one shape across several).
/// Each is mirrored (if `layer0_direction` is `Clockwise`) the same way;
/// odd layers' shapes then *additionally* go through
/// `mirror_layer_for_opposite_winding`, so every layer winds the same
/// rotational sense (reinforcing the magnetic field, layer over layer)
/// rather than alternating direction — this holds regardless of whether
/// consecutive layers' shapes are geometrically identical or not. (Odd vs.
/// even is what matters here, not the specific layer number: layer N's
/// via connects to layer N+1 at whichever end left off, so consecutive
/// layers always traverse in alternating directions, and it's *that*
/// alternation the mirroring corrects for.) Mirroring around the coil's
/// fixed center is stable across any number of layers, but doesn't land
/// each layer's connection point exactly on the previous layer's —
/// `kicad_writer` is responsible for bridging that gap with a short trace
/// alongside the layer-transition via.
pub fn generate_coil_paths(
    params: &CoilGeometryParams,
    coil_index: u32,
) -> Result<Vec<CoilLayerPath>, GeometryError> {
    if coil_index >= params.coils {
        return Err(GeometryError::CoilIndexOutOfRange {
            index: coil_index,
            coils: params.coils,
        });
    }

    if params.active_radial_length_mm <= params.trace_width_mm {
        return Err(GeometryError::NoRoomForTurns {
            active_radial_length_mm: params.active_radial_length_mm,
            trace_width_mm: params.trace_width_mm,
        });
    }

    let span = coil_angular_span_rad(params)?;
    let center = coil_center_angle_rad(coil_index, params.coils);

    let mut paths = Vec::with_capacity(params.layer_count as usize);
    for layer_index in 0..params.layer_count {
        let shape = crate::coil_shape::generate_coil_shape(params, span / 2.0, layer_index);
        if shape.len() < 2 {
            return Err(GeometryError::NoRoomForTurns {
                active_radial_length_mm: params.active_radial_length_mm,
                trace_width_mm: params.trace_width_mm,
            });
        }
        let mut points: Vec<Point2> =
            shape.into_iter().map(|(r, v)| place_point(r, v, center, span / 2.0)).collect();
        if params.layer0_direction == WindingDirection::Clockwise {
            for p in &mut points {
                *p = mirror_point_across_angle(*p, center);
            }
        }
        if layer_index % 2 == 1 {
            points = mirror_layer_for_opposite_winding(&points, center);
        }
        paths.push(CoilLayerPath { coil_index, layer_index, points });
    }

    Ok(paths)
}

/// Generates one coil's polyline on one specific layer. Prefer
/// `generate_coil_paths` when you need more than one layer of the same
/// coil — this recomputes the whole chain up to `layer_index` each call.
pub fn generate_coil_layer_path(
    params: &CoilGeometryParams,
    coil_index: u32,
    layer_index: u32,
) -> Result<CoilLayerPath, GeometryError> {
    if layer_index >= params.layer_count {
        return Err(GeometryError::LayerIndexOutOfRange {
            index: layer_index,
            layer_count: params.layer_count,
        });
    }
    let mut paths = generate_coil_paths(params, coil_index)?;
    Ok(paths.remove(layer_index as usize))
}

/// Generates every coil's path on every layer.
pub fn generate_all_paths(params: &CoilGeometryParams) -> Result<Vec<CoilLayerPath>, GeometryError> {
    let mut paths = Vec::with_capacity((params.coils * params.layer_count) as usize);
    for coil_index in 0..params.coils {
        paths.extend(generate_coil_paths(params, coil_index)?);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_params() -> CoilGeometryParams {
        CoilGeometryParams {
            coils: 18,
            mean_radius_mm: 25.0,
            active_radial_length_mm: 10.0,
            trace_width_mm: 0.3,
            trace_spacing_mm: 0.15,
            layer_count: 6,
            layer0_direction: WindingDirection::CounterClockwise,
            arc_segments: 12,
            min_inner_gap_mm: 1.2,
        }
    }

    fn peel_params() -> CoilGeometryParams {
        CoilGeometryParams {
            coils: 9,
            mean_radius_mm: 7.0,
            active_radial_length_mm: 6.0,
            trace_width_mm: 0.15,
            trace_spacing_mm: 0.1,
            layer_count: 4,
            layer0_direction: WindingDirection::CounterClockwise,
            arc_segments: 12,
            min_inner_gap_mm: 1.2,
        }
    }

    #[test]
    fn feed_and_peel_motors_fit_at_least_one_turn() {
        assert!(generate_coil_layer_path(&feed_params(), 0, 0).unwrap().points.len() >= 2);
        assert!(generate_coil_layer_path(&peel_params(), 0, 0).unwrap().points.len() >= 2);
    }

    #[test]
    fn zero_radial_length_has_no_room_for_turns() {
        let mut params = feed_params();
        params.active_radial_length_mm = 0.1;
        params.trace_width_mm = 0.3;
        assert_eq!(
            generate_coil_layer_path(&params, 0, 0),
            Err(GeometryError::NoRoomForTurns {
                active_radial_length_mm: 0.1,
                trace_width_mm: 0.3,
            })
        );
    }

    #[test]
    fn coil_centers_are_evenly_spaced_and_wrap_to_full_circle() {
        let coils = 18;
        for i in 0..coils {
            let angle = coil_center_angle_rad(i, coils);
            assert!((0.0..2.0 * PI).contains(&angle));
        }
        let last_gap = coil_center_angle_rad(coils, coils) - coil_center_angle_rad(coils - 1, coils);
        let first_gap = coil_center_angle_rad(1, coils) - coil_center_angle_rad(0, coils);
        assert!((last_gap - first_gap).abs() < 1e-9);
    }

    #[test]
    fn coil_path_reaches_near_the_outer_radius() {
        // Checks the outermost point reached *anywhere* along the path,
        // not literally `path.start()`: each lap's own points span
        // almost its full local radial reach (that's what makes it read
        // as nested rings rather than a single spiral line), so the
        // outermost point isn't necessarily the very first coordinate.
        // Not pinned to `trace_width_mm` of the true outer edge either —
        // the wide end's tangential offset grows a little faster than
        // the radial one as laps accumulate, so the outermost lap that's
        // still angularly safe usually lands a bit short of the
        // fully-allocated outer radius, not right at it.
        let params = feed_params();
        let path = generate_coil_layer_path(&params, 0, 0).unwrap();

        let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
        let max_r = path
            .points
            .iter()
            .map(|p| (p.x_mm.powi(2) + p.y_mm.powi(2)).sqrt())
            .fold(f64::MIN, f64::max);
        assert!((outer - max_r).abs() < params.active_radial_length_mm / 2.0);
        assert!(max_r > params.mean_radius_mm, "max_r should still be on the outer side of mean_radius");
    }

    #[test]
    fn coil_path_tapers_toward_the_mean_radius() {
        // Deliberately doesn't converge all the way to mean_radius —
        // `min_inner_gap_mm` stops it short on purpose, leaving room for
        // a hand-placed via. Checks the innermost point *reached
        // anywhere* along the path, not just its literal last point: each
        // lap's own points span almost its full local radial reach (that's
        // what makes it read as nested rings rather than a single spiral
        // line), so the very last coordinate isn't necessarily the
        // innermost one.
        let params = feed_params();
        let path = generate_coil_layer_path(&params, 0, 0).unwrap();
        let min_r = path
            .points
            .iter()
            .map(|p| (p.x_mm.powi(2) + p.y_mm.powi(2)).sqrt())
            .fold(f64::MAX, f64::min);
        assert!(
            min_r < params.mean_radius_mm,
            "min_r={min_r} should be on the inner side of mean_radius={}",
            params.mean_radius_mm
        );
        assert!(
            (min_r - params.mean_radius_mm).abs() < params.active_radial_length_mm * 0.5,
            "min_r={min_r} should still be reasonably close to mean_radius={}",
            params.mean_radius_mm
        );
    }

    #[test]
    fn adjacent_layer_transition_gaps_stay_bounded() {
        // Mirroring around the coil's fixed center (needed to keep every
        // layer's winding sense the same, see
        // `mirror_layer_for_opposite_winding`) means adjacent layers
        // don't necessarily meet at exactly the same point —
        // `kicad_writer` bridges the gap with a short trace instead.
        // Each layer can now be shaped independently (`coil_shape.rs`
        // gets the real `layer_index`, not just odd/even), so transition
        // gaps aren't required to repeat in a fixed period-2 pattern any
        // more the way they were when every even layer was a literal
        // clone of layer 0. What must still hold is that gaps stay
        // bounded, not that they grow layer over layer — which is
        // exactly what the rejected "mirror around each layer's own
        // endpoint" approach did, diverging past the coil's own wedge by
        // the second transition.
        fn dist(a: Point2, b: Point2) -> f64 {
            ((a.x_mm - b.x_mm).powi(2) + (a.y_mm - b.y_mm).powi(2)).sqrt()
        }

        for (name, params) in [("feed", feed_params()), ("peel", peel_params())] {
            let paths = generate_coil_paths(&params, 0).unwrap();
            let gaps: Vec<f64> = (0..paths.len() - 1).map(|i| dist(paths[i].end(), paths[i + 1].start())).collect();
            for (i, &gap) in gaps.iter().enumerate() {
                assert!(
                    gap < params.active_radial_length_mm,
                    "{name} motor, transition {i}: gap {gap} is bigger than the coil's whole \
                     radial span ({}) — looks like it's diverging, not a normal mirror offset",
                    params.active_radial_length_mm
                );
            }
        }
    }

    #[test]
    fn every_layer_winds_in_the_same_rotational_sense() {
        // Reversing point order alone (the pre-fix behavior) flips each
        // layer's winding sense, which would cancel rather than
        // reinforce the coil's magnetic field layer over layer. The
        // signed sweep of the radius vector (a shoelace-style sum) is
        // positive for a net-CCW path and negative for net-CW; every
        // layer should land on the same side of zero.
        fn signed_sweep(points: &[Point2]) -> f64 {
            points
                .windows(2)
                .map(|w| w[0].x_mm * w[1].y_mm - w[1].x_mm * w[0].y_mm)
                .sum()
        }

        let params = feed_params();
        let paths = generate_coil_paths(&params, 0).unwrap();
        let signs: Vec<bool> = paths.iter().map(|p| signed_sweep(&p.points) > 0.0).collect();
        assert!(
            signs.iter().all(|&s| s == signs[0]),
            "all layers should wind the same rotational sense, got signs: {signs:?}"
        );
    }

    #[test]
    fn next_layer_starts_at_the_same_point_but_is_not_a_plain_reversal() {
        // Plain point-order reversal (identical shape, opposite
        // traversal) is exactly the bug being fixed, so confirm layer 1
        // is NOT that — it must differ from layer 0's shape somewhere
        // even though (per the via-connectability test) it starts at
        // layer 0's exact ending point.
        let params = feed_params();
        let paths = generate_coil_paths(&params, 0).unwrap();
        let mut layer0_reversed = paths[0].points.clone();
        layer0_reversed.reverse();
        assert_ne!(paths[1].points, layer0_reversed);
    }

    #[test]
    fn clockwise_direction_mirrors_the_counter_clockwise_trace() {
        let mut ccw = feed_params();
        ccw.layer0_direction = WindingDirection::CounterClockwise;
        let mut cw = feed_params();
        cw.layer0_direction = WindingDirection::Clockwise;

        let ccw_path = generate_coil_layer_path(&ccw, 0, 0).unwrap();
        let cw_path = generate_coil_layer_path(&cw, 0, 0).unwrap();

        assert_eq!(ccw_path.points.len(), cw_path.points.len());
        // Mirrored, not identical: at least one point should differ.
        assert!(ccw_path.points.iter().zip(&cw_path.points).any(|(a, b)| {
            (a.x_mm - b.x_mm).abs() > 1e-6 || (a.y_mm - b.y_mm).abs() > 1e-6
        }));
    }

    #[test]
    fn out_of_range_coil_index_is_an_error() {
        let params = feed_params();
        assert_eq!(
            generate_coil_layer_path(&params, params.coils, 0),
            Err(GeometryError::CoilIndexOutOfRange {
                index: params.coils,
                coils: params.coils,
            })
        );
    }

    #[test]
    fn out_of_range_layer_index_is_an_error() {
        let params = feed_params();
        assert_eq!(
            generate_coil_layer_path(&params, 0, params.layer_count),
            Err(GeometryError::LayerIndexOutOfRange {
                index: params.layer_count,
                layer_count: params.layer_count,
            })
        );
    }

    #[test]
    fn generate_all_paths_covers_every_coil_and_layer() {
        let params = peel_params();
        let paths = generate_all_paths(&params).unwrap();
        assert_eq!(paths.len(), (params.coils * params.layer_count) as usize);
    }
}
