use generator::kicad_proto::{self, Distance, Net, Track, Vector2};
use kicad_ipc_rs::{CommitAction, KiCadClientBlocking, PcbObjectTypeCode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let commit = client.begin_commit()?;

    let track = Track {
        id: None,
        start: Some(Vector2 { x_nm: 60_000_000, y_nm: 60_000_000 }),
        end: Some(Vector2 { x_nm: 65_000_000, y_nm: 60_000_000 }),
        width: Some(Distance { value_nm: 250_000 }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        layer: kicad_proto::BOARD_LAYER_F_CU,
        net: Some(Net { code: Some(kicad_proto::NetCode { value: 0 }), name: String::new() }),
    };

    match client.create_items(vec![kicad_proto::pack_any(&track, kicad_proto::TRACK_TYPE_URL)], None) {
        Ok(created) => println!("create_items ok: {} item(s), raw_len={:?}", created.len(), created.iter().map(|i| i.value.len()).collect::<Vec<_>>()),
        Err(err) => println!("create_items ERROR: {err}"),
    }
    client.end_commit(commit, CommitAction::Commit, "probe_id_none")?;

    let found = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_trace().code])?;
    for item in &found {
        println!("found: {item:?}");
    }

    let cleanup = client.begin_commit()?;
    let ids: Vec<String> = found.iter().filter_map(|i| match i {
        kicad_ipc_rs::PcbItem::Track(t) => t.id.clone(),
        _ => None,
    }).collect();
    client.delete_items(ids)?;
    client.end_commit(cleanup, CommitAction::Commit, "probe_id_none: cleanup")?;

    Ok(())
}
