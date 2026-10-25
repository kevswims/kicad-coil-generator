//! Live validation of `kicad_writer::write_motor` against the board's
//! current 2 real layers (F.Cu/B.Cu) — a stand-in for peel_motor.toml's
//! real 4-layer config, which can't be tested until the inner layers exist
//! (see the plan's Milestone 4 board-stackup note). Writes one full motor's
//! winding, reports what got created, then cleans it back off the board.

use generator::config::{MotorConfig, Placement, PhaseTerminations, PointMm, ViaConfig, WindingDirectionConfig};
use generator::kicad_writer::{self, GenerationScope};
use kicad_ipc_rs::{CommitAction, KiCadClientBlocking, PcbObjectTypeCode};

fn test_config() -> MotorConfig {
    MotorConfig {
        name: "peel_2layer_test".to_string(),
        placement: Placement { center_x_mm: 100.0, center_y_mm: 0.0, rotation_deg: 0.0 },
        poles: 8,
        coils: 9,
        mean_radius_mm: 7.0,
        active_radial_length_mm: 6.0,
        trace_width_mm: 0.15,
        trace_spacing_mm: 0.1,
        bus_trace_width_mm: 0.3,
        copper_weight_oz: None,
        layers: vec!["F.Cu".to_string(), "B.Cu".to_string()],
        layer0_direction: WindingDirectionConfig::Ccw,
        via: ViaConfig { drill_mm: 0.3, diameter_mm: 0.6 },
        phase_terminations: PhaseTerminations {
            a: PointMm { x_mm: 13.0, y_mm: 0.0 },
            b: PointMm { x_mm: -6.5, y_mm: 11.26 },
            c: PointMm { x_mm: -6.5, y_mm: -11.26 },
        },
        phase_start_terminations: PhaseTerminations {
            a: PointMm { x_mm: 14.28, y_mm: 2.52 },
            b: PointMm { x_mm: -9.32, y_mm: 11.11 },
            c: PointMm { x_mm: -4.96, y_mm: -13.63 },
        },
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KiCadClientBlocking::connect()?;
    let config = test_config();

    let scope = GenerationScope::full(config.layer_count());
    let report = kicad_writer::write_motor(&client, &config, 12, &scope)?;
    println!("wrote {} item(s) for motor `{}`", report.created_item_count, report.motor_name);

    let nets = client.get_nets()?;
    println!("nets on board now: {}", nets.len());
    for net in &nets {
        if net.name.starts_with("PEEL_2LAYER_TEST") {
            println!("  {}", net.name);
        }
    }

    let tracks = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_trace().code])?;
    let vias = client.get_items_by_type_codes(vec![PcbObjectTypeCode::new_via().code])?;
    println!("board now has {} track(s), {} via(s)", tracks.len(), vias.len());

    let cleanup = client.begin_commit()?;
    let mut ids = Vec::new();
    for item in tracks {
        if let kicad_ipc_rs::PcbItem::Track(t) = item {
            if let Some(id) = t.id {
                ids.push(id);
            }
        }
    }
    for item in vias {
        if let kicad_ipc_rs::PcbItem::Via(v) = item {
            if let Some(id) = v.id {
                ids.push(id);
            }
        }
    }
    let deleted = client.delete_items(ids)?;
    client.end_commit(cleanup, CommitAction::Commit, "write_test_motor: cleanup")?;
    println!("cleaned up {} item(s)", deleted.len());

    Ok(())
}
