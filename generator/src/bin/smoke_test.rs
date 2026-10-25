//! Milestone 1 proof: can we create a track segment and a via on the live
//! board via `create_items()` with hand-encoded protobuf payloads, and read
//! them back? (`parse_and_create_items_from_string` was tried first and
//! confirmed dead: it returns an empty response and creates nothing.)

use generator::kicad_proto::{
    self, DrillProperties, Kiid, Net, NetCode, PadStack, PadStackLayer, Track, Vector2, Via,
};
use kicad_ipc_rs::{CommitAction, KiCadClientBlocking, PcbObjectTypeCode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;

    let version = client.get_version()?;
    println!("connected: KiCad {}", version.full_version);

    let nets = client.get_nets()?;
    println!("nets on board: {}", nets.len());
    let net0 = Net {
        code: Some(NetCode { value: 0 }),
        name: String::new(),
    };

    let commit = client.begin_commit()?;

    let track = Track {
        id: Some(Kiid {
            value: "5b1f7a3e-0000-4000-8000-000000000201".to_string(),
        }),
        start: Some(Vector2 {
            x_nm: 100_000_000,
            y_nm: 100_000_000,
        }),
        end: Some(Vector2 {
            x_nm: 120_000_000,
            y_nm: 100_000_000,
        }),
        width: Some(kicad_proto::Distance {
            value_nm: 250_000,
        }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        layer: kicad_proto::BOARD_LAYER_F_CU,
        net: Some(net0.clone()),
    };

    let via = Via {
        id: Some(Kiid {
            value: "5b1f7a3e-0000-4000-8000-000000000202".to_string(),
        }),
        position: Some(Vector2 {
            x_nm: 120_000_000,
            y_nm: 100_000_000,
        }),
        pad_stack: Some(PadStack {
            r#type: kicad_proto::PAD_STACK_TYPE_NORMAL,
            layers: vec![kicad_proto::BOARD_LAYER_F_CU, kicad_proto::BOARD_LAYER_B_CU],
            drill: Some(DrillProperties {
                start_layer: kicad_proto::BOARD_LAYER_F_CU,
                end_layer: kicad_proto::BOARD_LAYER_B_CU,
                diameter: Some(Vector2 {
                    x_nm: 300_000,
                    y_nm: 300_000,
                }),
                shape: kicad_proto::DRILL_SHAPE_CIRCLE,
            }),
            unconnected_layer_removal: kicad_proto::UNCONNECTED_LAYER_REMOVAL_KEEP,
            copper_layers: vec![PadStackLayer {
                layer: kicad_proto::BOARD_LAYER_F_CU,
                shape: kicad_proto::PAD_STACK_SHAPE_CIRCLE,
                size: Some(Vector2 {
                    x_nm: 600_000,
                    y_nm: 600_000,
                }),
            }],
        }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        net: Some(net0),
        r#type: kicad_proto::VIA_TYPE_THROUGH,
    };

    let items = vec![
        kicad_proto::pack_any(&track, kicad_proto::TRACK_TYPE_URL),
        kicad_proto::pack_any(&via, kicad_proto::VIA_TYPE_URL),
    ];

    match client.create_items(items, None) {
        Ok(created) => {
            println!("create_items returned {} item(s):", created.len());
            for item in &created {
                println!("  type_url={} raw_len={}", item.type_url, item.value.len());
            }
        }
        Err(err) => println!("create_items ERROR: {err}"),
    }

    client.end_commit(commit, CommitAction::Commit, "smoke_test: track + via")?;
    println!("commit finalized");

    let track_code = PcbObjectTypeCode::new_trace();
    let via_code = PcbObjectTypeCode::new_via();
    let found = client.get_items_by_type_codes(vec![track_code.code, via_code.code])?;
    println!("round-trip query found {} track/via item(s) on board", found.len());
    for item in &found {
        println!("  {item:?}");
    }

    // Throwaway smoke-test items; remove them so the real design starts clean.
    let cleanup_commit = client.begin_commit()?;
    let ids: Vec<String> = found.iter().filter_map(item_id).collect();
    let deleted = client.delete_items(ids)?;
    client.end_commit(cleanup_commit, CommitAction::Commit, "smoke_test: cleanup")?;
    println!("cleaned up {} item(s)", deleted.len());

    Ok(())
}

fn item_id(item: &kicad_ipc_rs::PcbItem) -> Option<String> {
    match item {
        kicad_ipc_rs::PcbItem::Track(t) => t.id.clone(),
        kicad_ipc_rs::PcbItem::Via(v) => v.id.clone(),
        _ => None,
    }
}
