//! One-time board setup step: expands the physical stackup from 2 copper
//! layers (F.Cu/B.Cu) to 6 (F.Cu, In1..In4.Cu, B.Cu) so the feed motor (6
//! layers) and peel motor (4 of those 6) have real layers to write to.
//!
//! Preserves F.Cu/B.Cu and every non-copper layer exactly as read; only the
//! single existing dielectric gap between F.Cu and B.Cu is replaced with
//! four new copper layers interleaved with five smaller dielectric gaps,
//! keeping the total dielectric thickness (and so overall board thickness)
//! unchanged from what the board already had.
//!
//! **Confirmed non-functional on KiCad 10.0.5**: `update_board_stackup()` is
//! fully modeled in `kicad-ipc-rs` with real public types, and
//! `get_board_stackup()` (read) works, but the write call returns
//! `AS_UNHANDLED` — "no handler available for request of type
//! kiapi.board.commands.UpdateBoardStackup". The command is declared in the
//! IPC schema but not yet implemented server-side in this KiCad version.
//! Adding the inner layers needs a manual step in KiCad's Board Setup >
//! Physical Stackup dialog instead. Kept here (rather than deleted) in case
//! a future KiCad point release adds the handler.

use kicad_ipc_rs::{BoardLayerInfo, BoardStackupDielectricProperties, BoardStackupLayer, BoardStackupLayerType, KiCadClientBlocking};

const NEW_INNER_LAYER_NAMES: [&str; 4] = ["In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let mut stackup = client.get_board_stackup()?;

    let dielectric_index = stackup
        .layers
        .iter()
        .position(|l| l.layer_type == BoardStackupLayerType::Dielectric)
        .expect("board should have exactly one dielectric layer between F.Cu and B.Cu");

    let original_dielectric = stackup.layers[dielectric_index].clone();
    let original_dielectric_thickness_nm = original_dielectric.thickness_nm.unwrap_or(0);

    let copper_reference = stackup
        .layers
        .iter()
        .find(|l| l.layer_type == BoardStackupLayerType::Copper)
        .expect("board should have at least one existing copper layer")
        .clone();
    let copper_thickness_nm = copper_reference.thickness_nm.unwrap_or(35_000);

    let new_copper_total_nm = copper_thickness_nm * NEW_INNER_LAYER_NAMES.len() as i64;
    let gap_count = NEW_INNER_LAYER_NAMES.len() as i64 + 1;
    let per_gap_thickness_nm = (original_dielectric_thickness_nm - new_copper_total_nm) / gap_count;

    println!(
        "existing dielectric: {original_dielectric_thickness_nm}nm -> {gap_count} gaps of ~{per_gap_thickness_nm}nm \
         around {} new {copper_thickness_nm}nm copper layers",
        NEW_INNER_LAYER_NAMES.len()
    );

    let make_dielectric = || BoardStackupLayer {
        layer: original_dielectric.layer.clone(),
        user_name: original_dielectric.user_name.clone(),
        material_name: original_dielectric.material_name.clone(),
        enabled: original_dielectric.enabled,
        thickness_nm: Some(per_gap_thickness_nm),
        layer_type: BoardStackupLayerType::Dielectric,
        color: original_dielectric.color.clone(),
        dielectric_layers: Vec::<BoardStackupDielectricProperties>::new(),
    };

    let make_copper = |name: &str| {
        let id = BoardLayerInfo::id_from_name(name)
            .unwrap_or_else(|| panic!("unknown KiCad layer name `{name}`"));
        BoardStackupLayer {
            layer: BoardLayerInfo {
                id,
                name: name.replace('.', "_").to_uppercase(),
            },
            user_name: name.to_string(),
            material_name: copper_reference.material_name.clone(),
            enabled: copper_reference.enabled,
            thickness_nm: Some(copper_thickness_nm),
            layer_type: BoardStackupLayerType::Copper,
            color: None,
            dielectric_layers: Vec::new(),
        }
    };

    let mut replacement = Vec::new();
    replacement.push(make_dielectric());
    for name in NEW_INNER_LAYER_NAMES {
        replacement.push(make_copper(name));
        replacement.push(make_dielectric());
    }

    stackup.layers.splice(dielectric_index..=dielectric_index, replacement);

    let updated = client.update_board_stackup(stackup)?;
    println!(
        "board now has {} copper layer(s)",
        updated
            .layers
            .iter()
            .filter(|l| l.layer_type == BoardStackupLayerType::Copper)
            .count()
    );
    for layer in &updated.layers {
        if layer.layer_type == BoardStackupLayerType::Copper {
            println!("  {} (id={}) thickness_nm={:?}", layer.user_name, layer.layer.id, layer.thickness_nm);
        }
    }

    Ok(())
}
