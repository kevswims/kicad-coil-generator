//! Debug tool: dumps one coil's polyline points as an SVG (and prints raw
//! coordinates) so shape/angle issues can be inspected quickly without
//! round-tripping through KiCad.

use generator::coil_geometry::{self, CoilGeometryParams, WindingDirection};

/// Path to write `coil_debug_{name}.svg` to — `generator/svg_out/`,
/// created on first use if it doesn't exist yet. Anchored on
/// `CARGO_MANIFEST_DIR` (not the current directory) so it lands in the
/// same place regardless of where `cargo run` is invoked from.
fn svg_path(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("svg_out");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("coil_debug_{name}.svg")).display().to_string()
}

fn render_single_layer_zoomed(name: &str, params: CoilGeometryParams) {
    let path = coil_geometry::generate_coil_layer_path(&params, 0, 0).unwrap();

    let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let inner = params.mean_radius_mm - params.active_radial_length_mm / 2.0;
    let scale = 60.0;
    let width_mm = outer - inner + 4.0;
    let view_w = width_mm * scale;
    let view_h = width_mm * scale;
    // Center the view on the coil itself (around mean_radius, angle 0),
    // not on the motor's origin which the coil sits far away from.
    let cx = view_w / 2.0 - params.mean_radius_mm * scale;
    let cy = view_h / 2.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view_w}\" height=\"{view_h}\" viewBox=\"0 0 {view_w} {view_h}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n\
         <line x1=\"0\" y1=\"{cy}\" x2=\"{view_w}\" y2=\"{cy}\" stroke=\"#ddd\" stroke-width=\"1\"/>\n\
         <line x1=\"{cx}\" y1=\"0\" x2=\"{cx}\" y2=\"{view_h}\" stroke=\"#ddd\" stroke-width=\"1\"/>\n"
    );

    let points: Vec<String> = path
        .points
        .iter()
        .map(|p| format!("{},{}", cx + p.x_mm * scale, cy - p.y_mm * scale))
        .collect();
    svg.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"red\" stroke-width=\"1.5\"/>\n",
        points.join(" ")
    ));
    // Mark every point with a small circle so individual vertices are visible.
    for p in &path.points {
        let x = cx + p.x_mm * scale;
        let y = cy - p.y_mm * scale;
        svg.push_str(&format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"2\" fill=\"blue\"/>\n"));
    }

    svg.push_str("</svg>\n");
    let out_path = svg_path(name);
    std::fs::write(&out_path, svg).unwrap();
    println!("wrote {out_path} ({} points)", path.points.len());
    for (i, p) in path.points.iter().enumerate() {
        println!("  [{i}] x={:.4} y={:.4} r={:.4}", p.x_mm, p.y_mm, (p.x_mm.powi(2) + p.y_mm.powi(2)).sqrt());
    }
}

/// Renders with the polyline stroked at the *actual* trace width (in mm,
/// scaled the same as the geometry) rather than a thin debug line — this
/// is what actually shows whether adjacent turns' copper overlaps, which
/// a thin centerline render can't reveal.
fn render_single_layer_true_width(name: &str, params: CoilGeometryParams) {
    let path = coil_geometry::generate_coil_layer_path(&params, 0, 0).unwrap();

    let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let inner = params.mean_radius_mm - params.active_radial_length_mm / 2.0;
    let scale = 200.0;
    let width_mm = outer - inner + 2.0;
    let view_w = width_mm * scale;
    let view_h = width_mm * scale;
    let cx = view_w / 2.0 - params.mean_radius_mm * scale;
    let cy = view_h / 2.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view_w}\" height=\"{view_h}\" viewBox=\"0 0 {view_w} {view_h}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );

    let points: Vec<String> = path
        .points
        .iter()
        .map(|p| format!("{},{}", cx + p.x_mm * scale, cy - p.y_mm * scale))
        .collect();
    let stroke_width = params.trace_width_mm * scale;
    svg.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"#2a7\" stroke-opacity=\"0.6\" \
         stroke-width=\"{stroke_width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
        points.join(" ")
    ));
    // Thin centerline on top so overlap vs. intentional adjacency is easy to tell apart.
    svg.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.5\"/>\n",
        points.join(" ")
    ));

    svg.push_str("</svg>\n");
    let out_path = svg_path(name);
    std::fs::write(&out_path, svg).unwrap();
    println!("wrote {out_path} (trace_width={}mm, stroke_width_px={stroke_width})", params.trace_width_mm);
}

fn render_ring(name: &str, params: CoilGeometryParams) {
    let all_paths = coil_geometry::generate_all_paths(&params).unwrap();
    let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let scale = 8.0;
    let view = outer * scale * 2.4;
    let cx = view / 2.0;
    let cy = view / 2.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view}\" height=\"{view}\" viewBox=\"0 0 {view} {view}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );
    for path in &all_paths {
        if path.layer_index != 0 {
            continue;
        }
        let points: Vec<String> = path
            .points
            .iter()
            .map(|p| format!("{},{}", cx + p.x_mm * scale, cy - p.y_mm * scale))
            .collect();
        svg.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.8\"/>\n",
            points.join(" ")
        ));
    }
    svg.push_str("</svg>\n");
    let path = svg_path(name);
    std::fs::write(&path, svg).unwrap();
    println!("wrote {path}");
}

/// Same as `render_ring` but strokes at the actual `trace_width_mm`, and
/// zoomed into a crop near the outer radius / a coil-to-coil boundary —
/// matching what round 6's true-width render did for the center, but for
/// the outer edge and inter-coil corners instead.
fn render_ring_true_width_crop(name: &str, params: CoilGeometryParams) {
    let all_paths = coil_geometry::generate_all_paths(&params).unwrap();
    let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let scale = 40.0;
    let view_w = outer * scale * 1.6;
    let view_h = outer * scale * 0.9;
    let cx = view_w / 2.0;
    let cy = view_h - 20.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view_w}\" height=\"{view_h}\" viewBox=\"0 0 {view_w} {view_h}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );
    let stroke_width = params.trace_width_mm * scale;
    for path in &all_paths {
        if path.layer_index != 0 {
            continue;
        }
        let points: Vec<String> = path
            .points
            .iter()
            .map(|p| format!("{},{}", cx + p.x_mm * scale, cy - p.y_mm * scale))
            .collect();
        svg.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"#2a7\" stroke-opacity=\"0.6\" \
             stroke-width=\"{stroke_width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
            points.join(" ")
        ));
        svg.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.5\"/>\n",
            points.join(" ")
        ));
    }
    svg.push_str("</svg>\n");
    let path = svg_path(name);
    std::fs::write(&path, svg).unwrap();
    println!("wrote {path}");
}

/// Single coil, true width, zoomed on just the outer 2 laps (not the
/// whole radial span) so the outer-ring-to-next-ring gap and the
/// arc-to-side corners are large enough on screen to inspect closely.
fn render_outer_corner_zoom(name: &str, params: CoilGeometryParams) {
    let path = coil_geometry::generate_coil_layer_path(&params, 0, 0).unwrap();
    let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let scale = 500.0;
    let view_w = 6.0 * scale;
    let view_h = 6.0 * scale;
    let cx = view_w / 2.0 - outer * scale + 1.0 * scale;
    let cy = view_h / 2.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view_w}\" height=\"{view_h}\" viewBox=\"0 0 {view_w} {view_h}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );
    let points: Vec<String> = path
        .points
        .iter()
        .map(|p| format!("{},{}", cx + p.x_mm * scale, cy - p.y_mm * scale))
        .collect();
    let stroke_width = params.trace_width_mm * scale;
    svg.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"#2a7\" stroke-opacity=\"0.6\" \
         stroke-width=\"{stroke_width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
        points.join(" ")
    ));
    svg.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.5\"/>\n",
        points.join(" ")
    ));
    for (i, p) in path.points.iter().enumerate().take(12) {
        let x = cx + p.x_mm * scale;
        let y = cy - p.y_mm * scale;
        svg.push_str(&format!(
            "<circle cx=\"{x}\" cy=\"{y}\" r=\"4\" fill=\"red\"/>\n<text x=\"{}\" y=\"{}\" font-size=\"20\">{i}</text>\n",
            x + 6.0, y - 6.0
        ));
    }
    svg.push_str("</svg>\n");
    let out_path = svg_path(name);
    std::fs::write(&out_path, svg).unwrap();
    println!("wrote {out_path}");
}

/// Renders layer 0 and layer 1 of a single coil overlaid in different
/// colors (true trace width) so the winding-direction fix (each layer
/// should wind the same rotational sense, not just reverse point order)
/// can be checked visually — layer 1 should look like layer 0 mirrored
/// across the coil's near-center connection point, not an identical
/// shape retraced backwards.
fn render_two_layers_true_width(name: &str, params: CoilGeometryParams) {
    let paths = coil_geometry::generate_coil_paths(&params, 0).unwrap();
    let layer0 = &paths[0];
    let layer1 = &paths[1];

    let outer = params.mean_radius_mm + params.active_radial_length_mm / 2.0;
    let inner = params.mean_radius_mm - params.active_radial_length_mm / 2.0;
    let scale = 200.0;
    let width_mm = outer - inner + 2.0;
    let view_w = width_mm * scale;
    let view_h = width_mm * scale;
    let cx = view_w / 2.0 - params.mean_radius_mm * scale;
    let cy = view_h / 2.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{view_w}\" height=\"{view_h}\" viewBox=\"0 0 {view_w} {view_h}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );
    let stroke_width = params.trace_width_mm * scale;
    for (path, color) in [(layer0, "#2a7"), (layer1, "#a37")] {
        let points: Vec<String> = path
            .points
            .iter()
            .map(|p| format!("{},{}", cx + p.x_mm * scale, cy - p.y_mm * scale))
            .collect();
        svg.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"0.55\" \
             stroke-width=\"{stroke_width}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
            points.join(" ")
        ));
        svg.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.5\"/>\n",
            points.join(" ")
        ));
    }
    // Mark the start of each layer with a circle so direction is legible.
    for (path, color) in [(layer0, "#0a5"), (layer1, "#805")] {
        let p = path.start();
        let x = cx + p.x_mm * scale;
        let y = cy - p.y_mm * scale;
        svg.push_str(&format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"5\" fill=\"{color}\"/>\n"));
    }
    svg.push_str("</svg>\n");
    let out_path = svg_path(name);
    std::fs::write(&out_path, svg).unwrap();
    println!("wrote {out_path} (green=layer0 start@green-dot, magenta=layer1 start@magenta-dot)");
}

fn main() {
    let feed_params = CoilGeometryParams {
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

    render_single_layer_zoomed("zoom", feed_params);
    render_ring("feed_ring", feed_params);
    render_single_layer_true_width("true_width", feed_params);
    render_ring_true_width_crop("ring_true_width_crop", feed_params);
    render_outer_corner_zoom("outer_corner_zoom", feed_params);
    render_two_layers_true_width("two_layers", feed_params);
}
