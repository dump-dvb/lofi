#![deny(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
//! this library provides the correlation facility to infer the location where r09 telegram was
//! transmitted. You most probably want to look at [`crate::correlate`]

/// Tools to correlate telegrams to positions.
pub mod correlate;
