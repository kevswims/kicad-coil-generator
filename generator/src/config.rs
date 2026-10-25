//! Per-motor configuration, loaded from TOML.
//!
//! One file fully specifies one motor island (`feed_motor.toml`,
//! `peel_motor.toml`); `generate_stator` runs once per file. Coordinates
//! everywhere in this struct are motor-local (relative to the motor's own
//! center, before `placement` is applied) so a config is self-contained
//! regardless of where its motor ends up on the shared board.

use crate::coil_geometry::{CoilGeometryParams, WindingDirection};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MotorConfig {
    pub name: String,
    pub placement: Placement,
    pub poles: u32,
    pub coils: u32,
    pub mean_radius_mm: f64,
    pub active_radial_length_mm: f64,
    pub trace_width_mm: f64,
    pub trace_spacing_mm: f64,
    /// Trace width for the phase bus (inter-coil and termination wiring)
    /// — deliberately separate from `trace_width_mm`. The bus carries the
    /// same current as a coil's own turns (they're all in series) but
    /// isn't squeezed into a coil's tight angular wedge, so there's no
    /// reason to accept the coil turns' resistance there — wider is free.
    pub bus_trace_width_mm: f64,
    /// Informational only for now — not consumed by the layout geometry.
    pub copper_weight_oz: Option<f64>,
    /// KiCad copper layer names this motor's winding occupies, outer to
    /// inner in winding order. Length is this motor's layer count.
    pub layers: Vec<String>,
    pub layer0_direction: WindingDirectionConfig,
    pub via: ViaConfig,
    /// Where each phase's *last* coil's exit lead routes to.
    pub phase_terminations: PhaseTerminations,
    /// Where each phase's *first* coil's entry lead routes to — every
    /// phase needs both ends brought out to a connector pad (per
    /// `coil-generator-spec.md`'s "Target connector pads (where each
    /// phase's start/end should land)"), not just the end; leaving this
    /// out left the first coil in every phase chain with one terminal
    /// wired to nothing.
    pub phase_start_terminations: PhaseTerminations,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Placement {
    pub center_x_mm: f64,
    pub center_y_mm: f64,
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ViaConfig {
    pub drill_mm: f64,
    pub diameter_mm: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindingDirectionConfig {
    Cw,
    Ccw,
}

impl From<WindingDirectionConfig> for WindingDirection {
    fn from(value: WindingDirectionConfig) -> Self {
        match value {
            WindingDirectionConfig::Cw => WindingDirection::Clockwise,
            WindingDirectionConfig::Ccw => WindingDirection::CounterClockwise,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PointMm {
    pub x_mm: f64,
    pub y_mm: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PhaseTerminations {
    pub a: PointMm,
    pub b: PointMm,
    pub c: PointMm,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file `{path}`: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file `{path}`: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error(
        "motor `{name}`: {} layer names given but layer_count implied by winding is derived \
         from that same list — this can only happen if `layers` is empty",
        layers_len
    )]
    NoLayers { name: String, layers_len: usize },
}

impl MotorConfig {
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config: MotorConfig = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.display().to_string(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.layers.is_empty() {
            return Err(ConfigError::NoLayers {
                name: self.name.clone(),
                layers_len: self.layers.len(),
            });
        }
        Ok(())
    }

    pub fn layer_count(&self) -> u32 {
        self.layers.len() as u32
    }

    /// Builds the pure-geometry parameters `coil_geometry` needs, dropping
    /// everything KiCad/placement-specific — except a via-sized inner gap
    /// (see `CoilGeometryParams::min_inner_gap_mm`'s doc comment): two via
    /// diameters is a rough allowance for a transition via to land near a
    /// coil's own center without crowding the innermost turn, without
    /// eating so much of a small coil's radial room that turn count
    /// suffers badly (verified against both motors' real proportions).
    pub fn to_geometry_params(&self, arc_segments: u32) -> CoilGeometryParams {
        CoilGeometryParams {
            coils: self.coils,
            mean_radius_mm: self.mean_radius_mm,
            active_radial_length_mm: self.active_radial_length_mm,
            trace_width_mm: self.trace_width_mm,
            trace_spacing_mm: self.trace_spacing_mm,
            layer_count: self.layer_count(),
            layer0_direction: self.layer0_direction.into(),
            arc_segments,
            min_inner_gap_mm: self.via.diameter_mm * 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_toml() -> &'static str {
        r#"
        name = "test"
        poles = 8
        coils = 9
        mean_radius_mm = 7.0
        active_radial_length_mm = 6.0
        trace_width_mm = 0.15
        trace_spacing_mm = 0.1
        bus_trace_width_mm = 0.3
        layers = ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]
        layer0_direction = "ccw"

        [placement]
        center_x_mm = 0.0
        center_y_mm = 0.0
        rotation_deg = 0.0

        [via]
        drill_mm = 0.3
        diameter_mm = 0.6

        [phase_terminations.a]
        x_mm = 13.0
        y_mm = 0.0
        [phase_terminations.b]
        x_mm = -6.5
        y_mm = 11.26
        [phase_terminations.c]
        x_mm = -6.5
        y_mm = -11.26

        [phase_start_terminations.a]
        x_mm = 12.0
        y_mm = 2.0
        [phase_start_terminations.b]
        x_mm = -7.5
        y_mm = 9.26
        [phase_start_terminations.c]
        x_mm = -5.5
        y_mm = -13.26
        "#
    }

    #[test]
    fn parses_a_well_formed_config() {
        let config: MotorConfig = toml::from_str(sample_toml()).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.layer_count(), 4);
        assert!(config.copper_weight_oz.is_none());
        config.validate().unwrap();
    }

    #[test]
    fn feed_and_peel_configs_on_disk_parse_and_validate() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in ["feed_motor.toml", "peel_motor.toml"] {
            let config = MotorConfig::load(&manifest_dir.join(name))
                .unwrap_or_else(|err| panic!("{name}: {err}"));
            config.validate().unwrap_or_else(|err| panic!("{name}: {err}"));
        }
    }
}
