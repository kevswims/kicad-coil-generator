//! Saves the live board document to disk. IPC-created items exist in
//! KiCad's in-memory document immediately but the on-disk `.kicad_pcb`
//! file doesn't reflect them until something calls save — this does that.

use kicad_ipc_rs::KiCadClientBlocking;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    client.save_document()?;
    println!("saved");
    Ok(())
}
