//! `cargo run --bin generate-stator -- --config feed_motor.toml`
//!
//! Reads one motor's config and writes its winding to the already-open
//! KiCad board. Run once per motor island (once per config file).
//!
//! For building the board up incrementally and visually checking each
//! stage (rather than generating 30,000+ items at once with no way to
//! tell which part is wrong), pass a narrower scope:
//!
//!   --phases A                 (default: A,B,C)
//!   --layers 0                 (default: all layers in the config)
//!   --no-bus                   (skip inter-coil/termination bus wiring)
//!
//! Layer-transition vias (coil layer N to layer N+1) aren't generated at
//! all — place and wire those manually in the KiCad GUI.
//!
//! Each run only emits what its scope covers — run again with a wider
//! scope to add the next increment, it won't recreate what's already on
//! the board as long as the scopes don't overlap.

use generator::config::MotorConfig;
use generator::kicad_writer::{self, GenerationScope};
use generator::winding_map::Phase;
use kicad_ipc_rs::KiCadClientBlocking;

// 3 straight segments per arc rather than a smooth many-segment curve —
// simpler traces, fewer items, and the radial sides are already straight.
const DEFAULT_ARC_SEGMENTS: u32 = 3;

struct Args {
    config_path: String,
    phases: Option<Vec<Phase>>,
    layers: Option<Vec<u32>>,
    no_bus: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut config_path = None;
    let mut phases = None;
    let mut layers = None;
    let mut no_bus = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => {
                config_path = Some(iter.next().ok_or("--config requires a path argument")?.clone());
            }
            "--phases" => {
                let value = iter.next().ok_or("--phases requires a value, e.g. A,B")?;
                phases = Some(
                    value
                        .split(',')
                        .map(|s| match s.trim() {
                            "A" => Ok(Phase::A),
                            "B" => Ok(Phase::B),
                            "C" => Ok(Phase::C),
                            other => Err(format!("unknown phase `{other}`, expected A, B, or C")),
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            "--layers" => {
                let value = iter.next().ok_or("--layers requires a value, e.g. 0,1")?;
                layers = Some(
                    value
                        .split(',')
                        .map(|s| s.trim().parse::<u32>().map_err(|e| format!("bad layer index `{s}`: {e}")))
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            "--no-bus" => no_bus = true,
            other => return Err(format!("unrecognized argument `{other}`")),
        }
    }

    Ok(Args {
        config_path: config_path.ok_or("usage: generate-stator --config <path.toml> [--phases A,B] [--layers 0,1] [--no-bus]")?,
        phases,
        layers,
        no_bus,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw_args)?;

    let config = MotorConfig::load(std::path::Path::new(&args.config_path))?;
    println!("loaded motor `{}` from {}", config.name, args.config_path);

    let mut scope = GenerationScope::full(config.layer_count());
    if let Some(phases) = args.phases {
        scope.phases = phases;
    }
    if let Some(layers) = args.layers {
        scope.layer_indices = layers;
    }
    if args.no_bus {
        scope.include_bus = false;
    }
    println!(
        "scope: phases={:?} layers={:?} bus={}",
        scope.phases, scope.layer_indices, scope.include_bus
    );

    let client = KiCadClientBlocking::connect()?;
    let report = kicad_writer::write_motor(&client, &config, DEFAULT_ARC_SEGMENTS, &scope)?;

    println!(
        "wrote {} item(s) for motor `{}`",
        report.created_item_count, report.motor_name
    );

    Ok(())
}
