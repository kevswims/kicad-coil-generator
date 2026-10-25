//! Read-only diagnostic: prints the live board's current physical stackup.
//! Useful for confirming the manual layer-count change described in
//! `update_stackup.rs`'s doc comment actually landed.

use kicad_ipc_rs::KiCadClientBlocking;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let stackup = client.get_board_stackup()?;
    println!("{stackup:#?}");
    Ok(())
}
