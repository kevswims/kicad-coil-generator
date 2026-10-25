//! Deletes every track and via on the live board. Used to clear out a
//! previous (buggy) generator run before regenerating — confirm with
//! verify_counts.rs first that the board's tracks/vias really are only
//! generator output before running this.

use kicad_ipc_rs::{CommitAction, KiCadClientBlocking, PcbItem, PcbObjectTypeCode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;

    let tracks = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_trace().code])?;
    let vias = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_via().code])?;
    println!("deleting {} track(s), {} via(s)", tracks.len(), vias.len());

    let mut ids = Vec::new();
    for item in tracks {
        if let PcbItem::Track(t) = item {
            if let Some(id) = t.id {
                ids.push(id);
            }
        }
    }
    for item in vias {
        if let PcbItem::Via(v) = item {
            if let Some(id) = v.id {
                ids.push(id);
            }
        }
    }

    let commit = client.begin_commit()?;
    let deleted = client.delete_items(ids)?;
    client.end_commit(commit, CommitAction::Commit, "clear_board_items")?;
    println!("deleted {} item(s)", deleted.len());

    Ok(())
}
