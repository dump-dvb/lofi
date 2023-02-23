#![deny(missing_docs)]
//! this library provides the correlation facility to infer the location where r09 telegram was
//! transmitted. You most probably want to look at [`crate::correlate`]

/// Smol module for type aliases
#[cfg(any(feature = "correlate", feature = "filter"))]
pub mod types;

/// Tools to correlate telegrams to positions.
#[cfg(feature = "correlate")]
pub mod correlate;

/// Tools to fliter the telegrams.
#[cfg(feature = "filter")]
pub mod filter;

/// lofi GPS abstraction layer.
#[cfg(feature = "gps")]
pub mod gps;
