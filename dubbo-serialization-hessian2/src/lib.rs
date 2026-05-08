#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::checked_conversions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::doc_markdown,
    clippy::redundant_closure_for_method_calls,
    clippy::unreadable_literal
)]
//! Hessian2 serialization protocol implementation.
//!
//! This module implements the Hessian 2.0 serialization protocol as used by
//! Apache Dubbo for cross-language RPC communication.
//!
//! ## Supported Types
//!
//! - `null` (`N`)
//! - `boolean` (`T`/`F`)
//! - `int` (`I`) — 32-bit compact integer
//! - `long` (`L`) — 64-bit compact integer
//! - `double` (`D`) — 64-bit IEEE 754 float
//! - `date` (`d`) — 64-bit millisecond timestamp
//! - `string` (`S`/`R`) — UTF-8 string with chunking and references
//! - `binary` (`B`) — binary data with chunking
//! - `list` (`V`) — variable-length typed/untyped list
//! - `map` (`M`/`H`) — map and untyped map
//! - `ref` (`R`) — shared reference to prior object
//! - `class-def` (`C`) — object class definition
//! - `object` (`O`) — object instance

pub mod class_def;
pub mod codec;
pub mod decoder;
pub mod dubbo_types;
pub mod type_descriptor;
pub mod type_registry;
pub mod types;
