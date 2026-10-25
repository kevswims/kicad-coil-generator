use generator::kicad_proto::{self, DrillProperties, PadStack, PadStackLayer, Via, Vector2};
use kicad_ipc_rs::{BoardLayerInfo, CommitAction, KiCadClientBlocking};

fn try_via(client: &KiCadClientBlocking, label: &str, layers: Vec<i32>, start: i32, end: i32, copper: i32, via_type: i32, pst: i32) {
    let commit = client.begin_commit().unwrap();
    let via = Via {
        id: None,
        position: Some(Vector2 { x_nm: 40_000_000, y_nm: 40_000_000 }),
        pad_stack: Some(PadStack {
            r#type: pst,
            layers,
            drill: Some(DrillProperties {
                start_layer: start,
                end_layer: end,
                diameter: Some(Vector2 { x_nm: 300_000, y_nm: 300_000 }),
                shape: kicad_proto::DRILL_SHAPE_CIRCLE,
            }),
            unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
            copper_layers: vec![PadStackLayer {
                layer: copper,
                shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE,
                size: Some(Vector2 { x_nm: 600_000, y_nm: 600_000 }),
            }],
        }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        net: Some(kicad_proto::Net { code: Some(kicad_proto::NetCode { value: 0 }), name: String::new() }),
        r#type: via_type,
    };
    match client.create_items(vec![kicad_proto::pack_any(&via, kicad_proto::VIA_TYPE_URL)], None) {
        Ok(_) => println!("{label}: OK"),
        Err(err) => println!("{label}: ERROR {err}"),
    }
    let _ = client.end_commit(commit, CommitAction::Drop, format!("probe: {label}"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let in1 = BoardLayerInfo::id_from_name("In1.Cu").unwrap();
    let in4 = BoardLayerInfo::id_from_name("In4.Cu").unwrap();
    let f_cu = kicad_proto::BOARD_LAYER_F_CU;
    let b_cu = kicad_proto::BOARD_LAYER_B_CU;

    // Sanity re-check: F.Cu-In1 blind (known good) still works with this harness.
    try_via(&client, "F.Cu-In1 blind (control)", vec![f_cu, in1], f_cu, in1, f_cu, kicad_proto::VIA_TYPE_BLIND, kicad_proto::PAD_STACK_TYPE_NORMAL);

    // B.Cu with a non-adjacent inner layer (In1), same shape as the control but mirrored.
    try_via(&client, "B.Cu-In1 blind [b_cu,in1]", vec![b_cu, in1], b_cu, in1, b_cu, kicad_proto::VIA_TYPE_BLIND, kicad_proto::PAD_STACK_TYPE_NORMAL);

    // B.Cu-In4 blind but with PST_CUSTOM instead of PST_NORMAL.
    try_via(&client, "B.Cu-In4 blind, PST_CUSTOM", vec![in4, b_cu], in4, b_cu, in4, kicad_proto::VIA_TYPE_BLIND, kicad_proto::PAD_STACK_TYPE_CUSTOM);

    // B.Cu-In4 as VT_BURIED with PST_CUSTOM (treat B.Cu as if not special).
    try_via(&client, "B.Cu-In4 buried, PST_CUSTOM", vec![in4, b_cu], in4, b_cu, in4, kicad_proto::VIA_TYPE_BURIED, kicad_proto::PAD_STACK_TYPE_CUSTOM);

    // B.Cu-In4 as VT_THROUGH (wrong semantically, but let's see what happens).
    try_via(&client, "B.Cu-In4 through, PST_NORMAL", vec![in4, b_cu], in4, b_cu, in4, kicad_proto::VIA_TYPE_THROUGH, kicad_proto::PAD_STACK_TYPE_NORMAL);

    // B.Cu-In4 blind, PST_FRONT_INNER_BACK(2).
    try_via(&client, "B.Cu-In4 blind, PST_FRONT_INNER_BACK", vec![in4, b_cu], in4, b_cu, in4, kicad_proto::VIA_TYPE_BLIND, 2);

    Ok(())
}
