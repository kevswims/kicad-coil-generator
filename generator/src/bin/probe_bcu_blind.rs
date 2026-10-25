use generator::kicad_proto::{self, DrillProperties, PadStack, PadStackLayer, Via, Vector2};
use kicad_ipc_rs::{BoardLayerInfo, CommitAction, KiCadClientBlocking};

fn try_via(client: &KiCadClientBlocking, label: &str, layers: Vec<i32>, start_layer: i32, end_layer: i32, copper_layer: i32) {
    let commit = client.begin_commit().unwrap();
    let via = Via {
        id: None,
        position: Some(Vector2 { x_nm: 40_000_000, y_nm: 40_000_000 }),
        pad_stack: Some(PadStack {
            r#type: kicad_proto::PAD_STACK_TYPE_NORMAL,
            layers,
            drill: Some(DrillProperties {
                start_layer,
                end_layer,
                diameter: Some(Vector2 { x_nm: 300_000, y_nm: 300_000 }),
                shape: kicad_proto::DRILL_SHAPE_CIRCLE,
            }),
            unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
            copper_layers: vec![PadStackLayer {
                layer: copper_layer,
                shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE,
                size: Some(Vector2 { x_nm: 600_000, y_nm: 600_000 }),
            }],
        }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        net: Some(kicad_proto::Net { code: Some(kicad_proto::NetCode { value: 0 }), name: String::new() }),
        r#type: kicad_proto::VIA_TYPE_BLIND,
    };
    match client.create_items(vec![kicad_proto::pack_any(&via, kicad_proto::VIA_TYPE_URL)], None) {
        Ok(_) => println!("{label}: OK"),
        Err(err) => println!("{label}: ERROR {err}"),
    }
    let _ = client.end_commit(commit, CommitAction::Drop, format!("probe_bcu_blind: {label}"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let in4 = BoardLayerInfo::id_from_name("In4.Cu").unwrap();
    let b_cu = kicad_proto::BOARD_LAYER_B_CU;
    println!("In4.Cu id={in4} B.Cu id={b_cu}");

    // Numeric ascending (what failed in the real run): [in4, b_cu], start=in4, end=b_cu.
    try_via(&client, "ascending [in4,b_cu] start=in4 end=b_cu", vec![in4, b_cu], in4, b_cu, in4);
    // Outer-first, mirroring the working F.Cu-In1 case structurally: [b_cu, in4], start=b_cu, end=in4.
    try_via(&client, "outer-first [b_cu,in4] start=b_cu end=in4", vec![b_cu, in4], b_cu, in4, b_cu);
    // Outer-first layers, but copper_layers entry still at the inner layer.
    try_via(&client, "outer-first [b_cu,in4] start=b_cu end=in4 copper@in4", vec![b_cu, in4], b_cu, in4, in4);

    Ok(())
}
