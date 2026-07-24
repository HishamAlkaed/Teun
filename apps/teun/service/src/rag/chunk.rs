//! Token-aware chunking of the canonical body (RET-03).
//!
//! Splits the line-numbered canonical text into token-bounded chunks via
//! `text_splitter::TextSplitter` sized by the `cl100k_base` BPE (the tokenizer
//! of `text-embedding-3-large`). Each chunk carries `line_start`/`line_end`
//! (1-indexed lines of the canonical body, recovered from the splitter's byte
//! offsets) and the source `page` (read from `Canonical::page_of_line`), so
//! citations resolve deterministically against the canonical body.

use text_splitter::{ChunkConfig, TextSplitter};

use crate::rag::extract::Canonical;
use crate::rag::store::Chunk;

/// Chunk capacity in cl100k tokens (phase CONTEXT discretion).
pub const CHUNK_SIZE_TOKENS: usize = 700;

/// Overlap between consecutive chunks in cl100k tokens.
pub const CHUNK_OVERLAP_TOKENS: usize = 120;

/// Split a canonical body into token-bounded chunks tagged with
/// `{document_id, line_start, line_end, page}`.
///
/// - `line_start` = (count of `\n` before the chunk's byte offset) + 1.
/// - `line_end`   = `line_start` + (count of `\n` inside the chunk content).
///   The splitter trims chunk boundaries, so chunk content never starts or
///   ends with a newline.
/// - `page`       = `page_of_line[line_start - 1]`, clamped to the last known
///   page if the index is somehow out of range (defensive; the invariant
///   `page_of_line.len() == text.lines().count()` normally holds).
///
/// A body shorter than one chunk yields exactly one chunk spanning all its
/// lines; an empty body yields no chunks.
pub fn chunk_document(canonical: &Canonical, document_id: &str) -> Vec<Chunk> {
    if canonical.text.trim().is_empty() {
        return Vec::new();
    }

    let tokenizer = tiktoken_rs::cl100k_base()
        .expect("cl100k_base BPE is embedded in the binary and always loads");
    let config = ChunkConfig::new(CHUNK_SIZE_TOKENS)
        .with_overlap(CHUNK_OVERLAP_TOKENS)
        .expect("overlap is smaller than the chunk capacity")
        .with_sizer(tokenizer);
    let splitter = TextSplitter::new(config);

    let mut chunks = Vec::new();
    for (byte_off, content) in splitter.chunk_indices(&canonical.text) {
        if content.trim().is_empty() {
            continue;
        }
        let line_start = canonical.text[..byte_off]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1;
        let line_end = line_start + content.bytes().filter(|&b| b == b'\n').count();
        let page = canonical
            .page_of_line
            .get(line_start - 1)
            .or_else(|| canonical.page_of_line.last())
            .copied()
            .unwrap_or(1);

        chunks.push(Chunk {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: document_id.to_string(),
            content: content.to_string(),
            line_start: line_start as i32,
            line_end: line_end as i32,
            page: page as i32,
        });
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-built canonical body: `lines[i]` is line `i+1`, mapped to
    /// `pages[i]`. No PDF or network needed.
    fn canonical(lines: &[String], pages: &[u32]) -> Canonical {
        assert_eq!(lines.len(), pages.len(), "test fixture invariant");
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        Canonical {
            text,
            page_of_line: pages.to_vec(),
        }
    }

    /// A multi-chunk body: 300 unique Dutch-ish lines (~15 cl100k tokens each,
    /// ~4500 total), 30 lines per "page".
    fn large_canonical() -> Canonical {
        let lines: Vec<String> = (0..300)
            .map(|i| {
                format!(
                    "regel {i:03} van het beleidsdocument bevat richtlijnen over hypotheekacceptatie en beheer"
                )
            })
            .collect();
        let pages: Vec<u32> = (0..300).map(|i| (i / 30) as u32 + 1).collect();
        canonical(&lines, &pages)
    }

    /// Independently recompute a chunk's byte offset in the source text. Each
    /// test line is unique, so the chunk's first line pins the offset even
    /// with overlapping chunks.
    fn byte_offset_of(canonical: &Canonical, chunk_content: &str) -> usize {
        let first_line = chunk_content.lines().next().expect("chunk has content");
        let line_off = canonical
            .text
            .find(first_line)
            .expect("chunk's first line must occur in the source");
        // The chunk may start mid-line (word-boundary split); locate the exact
        // chunk start at-or-after the first line's start.
        canonical.text[line_off..]
            .find(chunk_content)
            .map(|off| line_off + off)
            .expect("chunk content must occur in the source at its line")
    }

    #[test]
    fn chunks_cover_the_source_body() {
        let canonical = large_canonical();
        let chunks = chunk_document(&canonical, "doc-1");
        assert!(
            chunks.len() > 1,
            "fixture must produce multiple chunks, got {}",
            chunks.len()
        );

        // Every chunk's content is a verbatim slice of the source.
        for chunk in &chunks {
            assert!(
                canonical.text.contains(&chunk.content),
                "chunk content must be a verbatim substring of the canonical body"
            );
            assert_eq!(chunk.document_id, "doc-1");
        }

        // Coverage modulo overlap: every source line appears in >= 1 chunk.
        let all_content: String = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for line in canonical.text.lines() {
            assert!(
                all_content.contains(line),
                "source line missing from all chunks: {line}"
            );
        }
    }

    #[test]
    fn line_start_matches_newline_count_before_byte_offset() {
        let canonical = large_canonical();
        let chunks = chunk_document(&canonical, "doc-1");

        for chunk in &chunks {
            assert!(
                chunk.line_start <= chunk.line_end,
                "line_start {} must be <= line_end {}",
                chunk.line_start,
                chunk.line_end
            );
            let byte_off = byte_offset_of(&canonical, &chunk.content);
            let expected_start = canonical.text[..byte_off]
                .bytes()
                .filter(|&b| b == b'\n')
                .count()
                + 1;
            assert_eq!(
                chunk.line_start as usize, expected_start,
                "line_start must equal newline-count-before-offset + 1"
            );
            let expected_end = expected_start
                + chunk.content.bytes().filter(|&b| b == b'\n').count();
            assert_eq!(chunk.line_end as usize, expected_end);
        }
    }

    #[test]
    fn page_is_read_from_page_of_line_at_line_start() {
        let canonical = large_canonical();
        let chunks = chunk_document(&canonical, "doc-1");
        assert!(chunks.len() > 1);

        for chunk in &chunks {
            let expected_page = canonical.page_of_line[chunk.line_start as usize - 1];
            assert_eq!(
                chunk.page as u32, expected_page,
                "chunk page must equal page_of_line[line_start-1]"
            );
        }
        // The fixture spans 10 pages; later chunks must not all sit on page 1.
        assert!(chunks.iter().any(|c| c.page > 1));
    }

    #[test]
    fn short_body_yields_exactly_one_chunk_spanning_all_lines() {
        let lines = vec![
            "regel een over acceptatie".to_string(),
            "regel twee over beheer".to_string(),
            "regel drie over voorleggen".to_string(),
        ];
        let canonical = canonical(&lines, &[1, 1, 2]);
        let chunks = chunk_document(&canonical, "doc-kort");

        assert_eq!(chunks.len(), 1, "short body must yield exactly one chunk");
        let chunk = &chunks[0];
        assert_eq!(chunk.line_start, 1);
        assert_eq!(chunk.line_end, 3);
        assert_eq!(chunk.page, 1);
        for line in canonical.text.lines() {
            assert!(chunk.content.contains(line));
        }
    }

    #[test]
    fn empty_body_yields_no_chunks() {
        let canonical = Canonical {
            text: String::new(),
            page_of_line: Vec::new(),
        };
        assert!(chunk_document(&canonical, "doc-leeg").is_empty());
    }
}
