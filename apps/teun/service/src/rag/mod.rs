//! RAG data layer: pgvector-backed document/chunk storage and embeddings.
//!
//! Phase 1 modules land incrementally:
//! - `store`: `documents`/`chunks` CRUD over sqlx (no top-K query yet — Plan 04)
//! - `embed`: OpenAI/Azure `text-embedding-3-large` client (batch + single)
//! - `extract`: pdfium extraction → line-numbered canonical body + page map
//! - `chunk`: token-aware chunking with line/page metadata
//! - `ingest`: extract → chunk → embed → store orchestration

pub mod chunk;
pub mod embed;
pub mod extract;
pub mod ingest;
pub mod store;
