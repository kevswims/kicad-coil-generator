//! Isolates which via transition in the real feed motor config actually
//! fails, by creating each coil's inter-layer vias one at a time in its own
//! dropped commit, rather than one giant batch where a single bad via's
//! error message doesn't say which one it was.

use generator::coil_geometry;
use generator::config::MotorConfig;
use generator::kicad_proto::{self, DrillProperties, PadStack, PadStackLayer, Via, Vector2};
use kicad_ipc_rs::{BoardLayerInfo, CommitAction, KiCadClientBlocking};

fn via_type_and_pad_stack_type(a: i32, b: i32) -> (i32, i32) {
    let is_outer = |id: i32| id == kicad_proto::BOARD_LAYER_F_CU || id == kicad_proto::BOARD_LAYER_B_CU;
    match (is_outer(a), is_outer(b)) {
        (true, true) => (kicad_proto::VIA_TYPE_THROUGH, kicad_proto::PAD_STACK_TYPE_NORMAL),
        (true, false) | (false, true) => (kicad_proto::VIA_TYPE_BLIND, kicad_proto::PAD_STACK_TYPE_NORMAL),
        (false, false) => (kicad_proto::VIA_TYPE_BURIED, kicad_proto::PAD_STACK_TYPE_CUSTOM),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let config = MotorConfig::load(std::path::Path::new("feed_motor.toml"))?;
    let geometry_params = config.to_geometry_params(12);

    let layer_ids: Vec<i32> = config
        .layers
        .iter()
        .map(|name| BoardLayerInfo::id_from_name(name).unwrap())
        .collect();
    println!("layer_ids: {layer_ids:?}");

    for coil_index in 0..config.coils {
        let paths = coil_geometry::generate_coil_paths(&geometry_params, coil_index)?;
        for layer_index in 0..paths.len() - 1 {
            let a = layer_ids[layer_index];
            let b = layer_ids[layer_index + 1];
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let (via_type, pad_stack_type) = via_type_and_pad_stack_type(low, high);
            let position = paths[layer_index].end();

            let via = Via {
                id: None,
                position: Some(Vector2 {
                    x_nm: (position.x_mm * 1_000_000.0).round() as i64,
                    y_nm: (position.y_mm * 1_000_000.0).round() as i64,
                }),
                pad_stack: Some(PadStack {
                    r#type: pad_stack_type,
                    layers: vec![low, high],
                    drill: Some(DrillProperties {
                        start_layer: low,
                        end_layer: high,
                        diameter: Some(Vector2 { x_nm: 300_000, y_nm: 300_000 }),
                        shape: kicad_proto::DRILL_SHAPE_CIRCLE,
                    }),
                    unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
                    copper_layers: vec![PadStackLayer {
                        layer: low,
                        shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE,
                        size: Some(Vector2 { x_nm: 600_000, y_nm: 600_000 }),
                    }],
                }),
                locked: kicad_proto::LOCKED_STATE_UNLOCKED,
                net: Some(kicad_proto::Net { code: Some(kicad_proto::NetCode { value: 0 }), name: String::new() }),
                r#type: via_type,
            };

            let commit = client.begin_commit()?;
            let result = client.create_items(vec![kicad_proto::pack_any(&via, kicad_proto::VIA_TYPE_URL)], None);
            client.end_commit(commit, CommitAction::Drop, "probe_feed_vias")?;

            if let Err(err) = result {
                println!(
                    "FAIL coil={coil_index} layer_index={layer_index} layers=({a},{b}) low_high=({low},{high}) via_type={via_type} pad_stack_type={pad_stack_type} pos=({:.4},{:.4}): {err}",
                    position.x_mm, position.y_mm
                );
            }
        }
    }
    println!("done");

    Ok(())
}
