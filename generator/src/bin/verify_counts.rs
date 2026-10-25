//! Read-only diagnostic: prints live track/via/net counts on the board.
//! Queries the live document directly rather than the on-disk `.kicad_pcb`
//! file, which only reflects a save (`save_board.rs`) — IPC-created items
//! exist in KiCad's in-memory document immediately, independent of saving.

use kicad_ipc_rs::{KiCadClientBlocking, PcbObjectTypeCode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let tracks = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_trace().code])?;
    let vias = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_via().code])?;
    let nets = client.get_nets()?;
    println!("tracks={} vias={} nets={}", tracks.len(), vias.len(), nets.len());
    for n in &nets {
        println!("  net: {} (code {})", n.name, n.code);
    }
    Ok(())
}
