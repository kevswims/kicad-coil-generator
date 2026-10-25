//! Numeric overlap check: computes the minimum distance between every
//! pair of non-adjacent segments (a) within one coil's layer-0 polyline,
//! and (b) between two adjacent coils' layer-0 polylines — and flags any
//! pair closer than `trace_width_mm` (same-coil) or
//! `trace_width_mm + trace_spacing_mm` (different coils/nets, since
//! that's the actual designed clearance between unrelated copper).

use generator::coil_geometry::{self, CoilGeometryParams, Point2, WindingDirection};

fn dist(a: Point2, b: Point2) -> f64 {
    ((a.x_mm - b.x_mm).powi(2) + (a.y_mm - b.y_mm).powi(2)).sqrt()
}

fn point_to_segment_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let ab_x = b.x_mm - a.x_mm;
    let ab_y = b.y_mm - a.y_mm;
    let len_sq = ab_x * ab_x + ab_y * ab_y;
    if len_sq < 1e-12 {
        return dist(p, a);
    }
    let t = (((p.x_mm - a.x_mm) * ab_x + (p.y_mm - a.y_mm) * ab_y) / len_sq).clamp(0.0, 1.0);
    let proj = Point2 { x_mm: a.x_mm + t * ab_x, y_mm: a.y_mm + t * ab_y };
    dist(p, proj)
}

fn segment_to_segment_dist(a1: Point2, b1: Point2, a2: Point2, b2: Point2) -> f64 {
    let samples = 20;
    let mut min_d = f64::MAX;
    for i in 0..=samples {
        let t = i as f64 / samples as f64;
        let p = Point2 { x_mm: a1.x_mm + t * (b1.x_mm - a1.x_mm), y_mm: a1.y_mm + t * (b1.y_mm - a1.y_mm) };
        min_d = min_d.min(point_to_segment_dist(p, a2, b2));
    }
    min_d
}

fn check_within_coil(pts: &[Point2], min_clearance: f64) {
    let mut worst: Option<(usize, usize, f64)> = None;
    let mut worst_nontrivial: Option<(usize, usize, f64)> = None;
    let mut violations = 0;
    for i in 0..pts.len() - 1 {
        for j in (i + 2)..pts.len() - 1 {
            let d = segment_to_segment_dist(pts[i], pts[i + 1], pts[j], pts[j + 1]);
            if worst.is_none_or(|(_, _, wd)| d < wd) {
                worst = Some((i, j, d));
            }
            if d < min_clearance {
                violations += 1;
                if d > 1e-6 && worst_nontrivial.is_none_or(|(_, _, wd)| d > wd) {
                    worst_nontrivial = Some((i, j, d));
                }
            }
        }
    }
    println!("WITHIN-COIL (min_clearance={min_clearance}mm): {violations} violating pairs");
    if let Some((i, j, d)) = worst {
        println!("  closest pair: segment[{i}] vs segment[{j}], dist={d:.4}mm");
    }
    if let Some((i, j, d)) = worst_nontrivial {
        println!("  worst NON-degenerate (dist>1e-6) violating pair: segment[{i}] vs segment[{j}], dist={d:.4}mm");
    } else {
        println!("  all violations are degenerate (dist<=1e-6, near-coincident points)");
    }
}

fn check_between_coils(pts_a: &[Point2], pts_b: &[Point2], min_clearance: f64) {
    let mut worst = f64::MAX;
    let mut violations = 0;
    for i in 0..pts_a.len() - 1 {
        for j in 0..pts_b.len() - 1 {
            let d = segment_to_segment_dist(pts_a[i], pts_a[i + 1], pts_b[j], pts_b[j + 1]);
            worst = worst.min(d);
            if d < min_clearance {
                violations += 1;
            }
        }
    }
    println!("BETWEEN-COILS (min_clearance={min_clearance}mm): {violations} violating pairs, closest={worst:.10}mm");
}

fn check_motor(name: &str, params: CoilGeometryParams, layer: u32) {
    println!("=== {name}, layer {layer} ===");

    let coil0 = coil_geometry::generate_coil_layer_path(&params, 0, layer).unwrap();
    let coil1 = coil_geometry::generate_coil_layer_path(&params, 1, layer).unwrap();
    println!("coil0: {} points, coil1: {} points", coil0.points.len(), coil1.points.len());

    check_within_coil(&coil0.points, params.trace_width_mm);
    check_between_coils(&coil0.points, &coil1.points, params.trace_width_mm + params.trace_spacing_mm);

    let span = coil_geometry::coil_angular_span_rad(&params).unwrap();
    let half_span_deg = (span / 2.0).to_degrees();
    println!("nominal half-span from coil center: {half_span_deg:.3} deg");
    let mut max_angle_deg = 0.0_f64;
    let mut max_at: Option<(usize, f64, f64)> = None;
    for (i, p) in coil0.points.iter().enumerate() {
        let angle_deg = p.y_mm.atan2(p.x_mm).to_degrees();
        let r = (p.x_mm.powi(2) + p.y_mm.powi(2)).sqrt();
        if angle_deg.abs() > max_angle_deg {
            max_angle_deg = angle_deg.abs();
            max_at = Some((i, angle_deg, r));
        }
    }
    if let Some((i, a, r)) = max_at {
        println!("max |angle| along coil0's trace: {a:.3} deg at point[{i}] (r={r:.3}mm) -- {}", if a.abs() > half_span_deg { "EXCEEDS nominal half-span!" } else { "within allocation" });
    }
    println!();
}

fn main() {
    let feed = CoilGeometryParams {
        coils: 18,
        mean_radius_mm: 25.0,
        active_radial_length_mm: 10.0,
        trace_width_mm: 0.3,
        trace_spacing_mm: 0.15,
        layer_count: 6,
        layer0_direction: WindingDirection::CounterClockwise,
        arc_segments: 3,
        min_inner_gap_mm: 1.2,
    };
    let peel = CoilGeometryParams {
        coils: 9,
        mean_radius_mm: 7.0,
        active_radial_length_mm: 6.0,
        trace_width_mm: 0.15,
        trace_spacing_mm: 0.1,
        layer_count: 6,
        layer0_direction: WindingDirection::CounterClockwise,
        arc_segments: 3,
        min_inner_gap_mm: 1.2,
    };

    let layer = std::env::args().nth(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    check_motor("feed", feed, layer);
    check_motor("peel", peel, layer);
}
