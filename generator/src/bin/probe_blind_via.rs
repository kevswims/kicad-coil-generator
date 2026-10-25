//! One-off probe: what via_type/pad_stack shape does KiCad accept for a via
//! that only spans two INNER copper layers (not the board's outer F.Cu/
//! B.Cu)? Milestone 1 only proved a through-via (F.Cu-B.Cu). Tries a few
//! candidate encodings against In1.Cu/In2.Cu and reports which succeed.

use generator::kicad_proto::{self, DrillProperties, PadStack, PadStackLayer, Via, Vector2};
use kicad_ipc_rs::{BoardLayerInfo, CommitAction, KiCadClientBlocking, PcbObjectTypeCode};

fn try_via(
    client: &KiCadClientBlocking,
    label: &str,
    via_type: i32,
    pad_stack_layers: Vec<i32>,
    drill_start: i32,
    drill_end: i32,
    copper_layer_id: i32,
) {
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
        pad_stack: Some(PadStack {
            r#type: kicad_proto::PAD_STACK_TYPE_NORMAL,
            layers: pad_stack_layers,
            drill: Some(DrillProperties {
                start_layer: drill_start,
                end_layer: drill_end,
                diameter: Some(Vector2 { x_nm: 300_000, y_nm: 300_000 }),
                shape: kicad_proto::DRILL_SHAPE_CIRCLE,
            }),
            unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
            copper_layers: vec![PadStackLayer {
                layer: copper_layer_id,
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

    let _ = client.end_commit(commit, CommitAction::Drop, format!("probe_blind_via: {label}"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let in1 = BoardLayerInfo::id_from_name("In1.Cu").unwrap();
    let in2 = BoardLayerInfo::id_from_name("In2.Cu").unwrap();
    let f_cu = kicad_proto::BOARD_LAYER_F_CU;

    let b_cu = kicad_proto::BOARD_LAYER_B_CU;

    // Candidate 1: current kicad_writer approach — VT_BLIND_BURIED (2), two inner layers.
    try_via(&client, "blind_buried(2) In1-In2", 2, vec![in1, in2], in1, in2, in1);
    // Candidate 2: VT_BLIND (4) — "starts on an outer layer, ends in the middle". Try F.Cu-In1.
    try_via(&client, "blind(4) F.Cu-In1", 4, vec![f_cu, in1], f_cu, in1, f_cu);
    // Candidate 3: VT_BURIED (5) — "start and end on inner layers". In1-In2.
    try_via(&client, "buried(5) In1-In2", 5, vec![in1, in2], in1, in2, in1);
    // Candidate 4: VT_BLIND_BURIED(2) but pad_stack.layers spans ALL copper layers
    // while drill start/end restricts to In1-In2 — some APIs expect "layers" to
    // mean "all layers this via could theoretically appear on" rather than the
    // actual span.
    try_via(
        &client,
        "blind_buried(2) full-layers, drill In1-In2",
        2,
        vec![f_cu, in1, in2, b_cu],
        in1,
        in2,
        in1,
    );

    // Confirm nothing leaked (all candidates used CommitAction::Drop).
    let found = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_via().code])?;
    println!("vias left on board: {}", found.len());

    Ok(())
}
