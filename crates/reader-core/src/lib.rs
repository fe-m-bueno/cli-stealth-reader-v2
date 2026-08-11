//! Deterministic domain model for the reader.
//!
//! This crate deliberately has no terminal, database, archive, PDF, or HTTP
//! dependencies. Adapters convert external data into these types.

mod book;

pub use book::{
    BlockKind, CanonicalBlock, CanonicalBook, CanonicalChapter, DiagnosticSeverity,
    ImportDiagnostic,
};
