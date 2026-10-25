//! Follow-up to probe_blind_via.rs: VT_BLIND(4) F.Cu-In1 worked; VT_BURIED(5)
//! In1-In2 (and VT_BLIND_BURIED(2)) did not. Try variations on padstack
//! shape/type for the purely-inner-to-inner case, since that's the majority
//! of transitions for both motors (all 3 of peel's, 3 of feed's 5).

use generator::kicad_proto::{self, DrillProperties, PadStack, PadStackLayer, Via, Vector2};
use kicad_ipc_rs::{BoardLayerInfo, CommitAction, KiCadClientBlocking};

fn try_via(client: &KiCadClientBlocking, label: &str, via_type: i32, pad_stack: PadStack) {
    let commit = match client.begin_commit() {
        Ok(c) => c,
        Err(err) => {
            println!("{label}: begin_commit ERROR {err}");
            return;
        }
    };

    let via = Via {
        id: None,
        position: Some(Vector2 { x_nm: 40_000_000, y_nm: 40_000_000 }),
        pad_stack: Some(pad_stack),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        net: Some(kicad_proto::Net { code: Some(kicad_proto::NetCode { value: 0 }), name: String::new() }),
        r#type: via_type,
    };

    match client.create_items(vec![kicad_proto::pack_any(&via, kicad_proto::VIA_TYPE_URL)], None) {
        Ok(_) => println!("{label}: OK"),
        Err(err) => println!("{label}: ERROR {err}"),
    }

    let _ = client.end_commit(commit, CommitAction::Drop, format!("probe_buried_via: {label}"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let in1 = BoardLayerInfo::id_from_name("In1.Cu").unwrap();
    let in2 = BoardLayerInfo::id_from_name("In2.Cu").unwrap();
    println!("In1.Cu id={in1} In2.Cu id={in2}");

    let drill = || DrillProperties {
        start_layer: in1,
        end_layer: in2,
        diameter: Some(Vector2 { x_nm: 300_000, y_nm: 300_000 }),
        shape: kicad_proto::DRILL_SHAPE_CIRCLE,
    };
    let size = Some(Vector2 { x_nm: 600_000, y_nm: 600_000 });

    // A: VT_BURIED(5), two copper_layers entries (one per layer) instead of one.
    try_via(&client, "A: buried(5), 2 copper_layers entries", 5, PadStack {
        r#type: kicad_proto::PAD_STACK_TYPE_NORMAL,
        layers: vec![in1, in2],
        drill: Some(drill()),
        unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
        copper_layers: vec![
            PadStackLayer { layer: in1, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size },
            PadStackLayer { layer: in2, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size },
        ],
    });

    // B: VT_BURIED(5), PST_FRONT_INNER_BACK(2) type.
    try_via(&client, "B: buried(5), PST_FRONT_INNER_BACK", 5, PadStack {
        r#type: 2,
        layers: vec![in1, in2],
        drill: Some(drill()),
        unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
        copper_layers: vec![PadStackLayer { layer: in1, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size }],
    });

    // C: VT_BURIED(5), PST_CUSTOM(3) type, one copper_layers entry.
    try_via(&client, "C: buried(5), PST_CUSTOM", 5, PadStack {
        r#type: 3,
        layers: vec![in1, in2],
        drill: Some(drill()),
        unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
        copper_layers: vec![PadStackLayer { layer: in1, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size }],
    });

    // D: VT_BURIED(5), PST_CUSTOM(3) type, two copper_layers entries.
    try_via(&client, "D: buried(5), PST_CUSTOM, 2 entries", 5, PadStack {
        r#type: 3,
        layers: vec![in1, in2],
        drill: Some(drill()),
        unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
        copper_layers: vec![
            PadStackLayer { layer: in1, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size },
            PadStackLayer { layer: in2, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size },
        ],
    });

    // E: VT_BLIND_BURIED(2) legacy type, PST_CUSTOM, 2 entries.
    try_via(&client, "E: blind_buried(2), PST_CUSTOM, 2 entries", 2, PadStack {
        r#type: 3,
        layers: vec![in1, in2],
        drill: Some(drill()),
        unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
        copper_layers: vec![
            PadStackLayer { layer: in1, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size },
            PadStackLayer { layer: in2, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size },
        ],
    });

    // F: VT_BURIED(5), reversed layer order [in2, in1] with drill matching.
    try_via(&client, "F: buried(5), reversed layer order", 5, PadStack {
        r#type: kicad_proto::PAD_STACK_TYPE_NORMAL,
        layers: vec![in2, in1],
        drill: Some(DrillProperties { start_layer: in2, end_layer: in1, ..drill() }),
        unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
        copper_layers: vec![PadStackLayer { layer: in2, shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE, size }],
    });

    Ok(())
}
