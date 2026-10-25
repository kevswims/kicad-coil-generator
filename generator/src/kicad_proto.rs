//! Minimal wire-compatible subset of KiCad's IPC board-item protobuf schema
//! (`kiapi.board.types` / `kiapi.common.types`), for the item kinds
//! `kicad-ipc-rs` doesn't expose a public constructor for (`Track`, `Via`).
//!
//! `kicad-ipc-rs` vendors the full generated schema but keeps it
//! `pub(crate)`, so it can't be imported directly. These structs carry only
//! the fields we set; protobuf is sparse by design, so omitted fields are
//! indistinguishable on the wire from a message that set them to their
//! default. Field tags and enum discriminants below were read from
//! `kicad-ipc-rs`'s own vendored generated code
//! (`kicad-ipc-rs-0.5.1/src/proto/generated/kiapi.{board,common}.types.rs`),
//! which is the exact schema this KiCad instance already round-trips
//! successfully through other `kicad-ipc-rs` calls (`get_version`,
//! `get_nets`, `begin_commit`) — so it's the right ground truth, not KiCad's
//! upstream `.proto` sources (which could drift from the installed 10.0.5
//! build in ways the vendored copy already accounts for).
//!
//! Enum fields are declared as plain `int32` (protobuf enums are int32 on
//! the wire) rather than as real Rust enums, using the raw discriminants
//! documented alongside each constant below — this avoids needing to
//! transcribe enum types we never decode.

use prost::Message;

pub const TRACK_TYPE_URL: &str = "type.googleapis.com/kiapi.board.types.Track";
pub const VIA_TYPE_URL: &str = "type.googleapis.com/kiapi.board.types.Via";

/// `BoardLayer::BL_F_Cu`
pub const BOARD_LAYER_F_CU: i32 = 3;
/// `BoardLayer::BL_B_Cu`
pub const BOARD_LAYER_B_CU: i32 = 34;

/// `LockedState::LS_UNLOCKED`
pub const LOCKED_STATE_UNLOCKED: i32 = 1;

/// `PadStackType::PST_NORMAL` (same shape on every copper layer). Confirmed
/// live: works when `BOARD_LAYER_F_CU` is one of the via's two layers
/// (through F.Cu-B.Cu, or blind F.Cu-inner) — but KiCad rejects it ("could
/// not unpack PCB_VIA") for anything else, INCLUDING a blind via between
/// B.Cu and an inner layer. It's specifically about F.Cu, not "any outer
/// layer" — B.Cu is treated differently. See
/// `kicad_writer::via_type_and_pad_stack_type`'s doc comment for the full
/// story (`probe_bcu_variants.rs` is what found this).
pub const PAD_STACK_TYPE_NORMAL: i32 = 1;
/// `PadStackType::PST_CUSTOM` ("arbitrary copper layers"). Confirmed live:
/// required (not `PST_NORMAL`) whenever `BOARD_LAYER_F_CU` is NOT one of
/// the via's two layers — that includes buried vias (both inner) AND blind
/// vias between B.Cu and an inner layer. `PST_FRONT_INNER_BACK` (=2) also
/// works in the cases tried but `PST_CUSTOM` is the more semantically
/// fitting one for "just these two layers".
pub const PAD_STACK_TYPE_CUSTOM: i32 = 3;
/// `PadStackShape::PSS_CIRCLE`
pub const PAD_STACK_SHAPE_CIRCLE: i32 = 1;
/// `UnconnectedLayerRemoval::ULR_KEEP`
pub const UNCONNECTED_LAYER_REMOVAL_KEEP: i32 = 1;
/// `DrillShape::DS_CIRCLE`
pub const DRILL_SHAPE_CIRCLE: i32 = 1;
/// `ViaType::VT_THROUGH` (spans the board's two outermost copper layers).
pub const VIA_TYPE_THROUGH: i32 = 1;
/// `ViaType::VT_BLIND` — one endpoint on an outer layer (F.Cu/B.Cu), the
/// other on an inner layer. Confirmed live with `pad_stack.layers` in
/// ascending KiCad-layer-id order and `PAD_STACK_TYPE_NORMAL`.
pub const VIA_TYPE_BLIND: i32 = 4;
/// `ViaType::VT_BURIED` — both endpoints on inner layers, no outer layer
/// involved. Confirmed live with `pad_stack.layers` in ascending
/// KiCad-layer-id order and `PAD_STACK_TYPE_CUSTOM` (not `_NORMAL`, which
/// KiCad rejects here).
pub const VIA_TYPE_BURIED: i32 = 5;

#[derive(Clone, PartialEq, Message)]
pub struct Kiid {
    #[prost(string, tag = "1")]
    pub value: String,
}

#[derive(Clone, Copy, PartialEq, Message)]
pub struct Vector2 {
    #[prost(int64, tag = "1")]
    pub x_nm: i64,
    #[prost(int64, tag = "2")]
    pub y_nm: i64,
}

#[derive(Clone, Copy, PartialEq, Message)]
pub struct Distance {
    #[prost(int64, tag = "1")]
    pub value_nm: i64,
}

#[derive(Clone, Copy, PartialEq, Message)]
pub struct NetCode {
    #[prost(int32, tag = "1")]
    pub value: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Net {
    #[prost(message, optional, tag = "1")]
    pub code: Option<NetCode>,
    #[prost(string, tag = "2")]
    pub name: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Track {
    #[prost(message, optional, tag = "1")]
    pub id: Option<Kiid>,
    #[prost(message, optional, tag = "2")]
    pub start: Option<Vector2>,
    #[prost(message, optional, tag = "3")]
    pub end: Option<Vector2>,
    #[prost(message, optional, tag = "4")]
    pub width: Option<Distance>,
    #[prost(int32, tag = "5")]
    pub locked: i32,
    #[prost(int32, tag = "6")]
    pub layer: i32,
    #[prost(message, optional, tag = "7")]
    pub net: Option<Net>,
}

#[derive(Clone, Copy, PartialEq, Message)]
pub struct DrillProperties {
    #[prost(int32, tag = "1")]
    pub start_layer: i32,
    #[prost(int32, tag = "2")]
    pub end_layer: i32,
    #[prost(message, optional, tag = "3")]
    pub diameter: Option<Vector2>,
    #[prost(int32, tag = "4")]
    pub shape: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct PadStackLayer {
    #[prost(int32, tag = "1")]
    pub layer: i32,
    #[prost(int32, tag = "2")]
    pub shape: i32,
    #[prost(message, optional, tag = "3")]
    pub size: Option<Vector2>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PadStack {
    #[prost(int32, tag = "1")]
    pub r#type: i32,
    #[prost(int32, repeated, tag = "2")]
    pub layers: Vec<i32>,
    #[prost(message, optional, tag = "3")]
    pub drill: Option<DrillProperties>,
    #[prost(int32, tag = "4")]
    pub unconnected_layer_removal: i32,
    #[prost(message, repeated, tag = "5")]
    pub copper_layers: Vec<PadStackLayer>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Via {
    #[prost(message, optional, tag = "1")]
    pub id: Option<Kiid>,
    #[prost(message, optional, tag = "2")]
    pub position: Option<Vector2>,
    #[prost(message, optional, tag = "3")]
    pub pad_stack: Option<PadStack>,
    #[prost(int32, tag = "4")]
    pub locked: i32,
    #[prost(message, optional, tag = "5")]
    pub net: Option<Net>,
    #[prost(int32, tag = "6")]
    pub r#type: i32,
}

pub fn pack_any<T: Message>(message: &T, type_url: &str) -> prost_types::Any {
    prost_types::Any {
        type_url: type_url.to_string(),
        value: message.encode_to_vec(),
    }
}
