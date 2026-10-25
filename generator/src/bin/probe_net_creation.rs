//! One-off probe: does creating a track with a brand-new net name register
//! that net on the board, or does it need to be pre-declared some other
//! way? Creates one track on a never-before-seen net, checks get_nets(),
//! then cleans up.

use generator::kicad_proto::{self, Kiid, Net, Track, Vector2};
use kicad_ipc_rs::{CommitAction, KiCadClientBlocking, PcbObjectTypeCode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;

    let before = client.get_nets()?;
    println!("nets before: {}", before.len());

    let commit = client.begin_commit()?;

    let track = Track {
        id: Some(Kiid {
            value: "5b1f7a3e-0000-4000-8000-000000000301".to_string(),
        }),
        start: Some(Vector2 {
            x_nm: 50_000_000,
            y_nm: 50_000_000,
        }),
        end: Some(Vector2 {
            x_nm: 55_000_000,
            y_nm: 50_000_000,
        }),
        width: Some(kicad_proto::Distance { value_nm: 250_000 }),
        locked: kicad_proto::LOCKED_STATE_UNLOCKED,
        layer: kicad_proto::BOARD_LAYER_F_CU,
        net: Some(Net {
            code: None,
            name: "PROBE_NET_A".to_string(),
        }),
    };

    match client.create_items(vec![kicad_proto::pack_any(&track, kicad_proto::TRACK_TYPE_URL)], None) {
        Ok(_) => println!("create_items: ok"),
        Err(err) => println!("create_items ERROR: {err}"),
    }

    client.end_commit(commit, CommitAction::Commit, "probe_net_creation")?;

    let after = client.get_nets()?;
    println!("nets after: {}", after.len());
    for net in &after {
        println!("  code={} name={:?}", net.code, net.name);
    }

    let found = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_trace().code])?;
    for item in &found {
        println!("found item: {item:?}");
    }

    let cleanup = client.begin_commit()?;
    let ids: Vec<String> = found
        .iter()
        .filter_map(|item| match item {
            kicad_ipc_rs::PcbItem::Track(t) => t.id.clone(),
            _ => None,
        })
        .collect();
    let deleted = client.delete_items(ids)?;
    client.end_commit(cleanup, CommitAction::Commit, "probe_net_creation: cleanup")?;
    println!("cleaned up {} item(s)", deleted.len());

    Ok(())
}
