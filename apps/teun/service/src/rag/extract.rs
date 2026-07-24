//! PDF text extraction via pdfium-render (ING-01).
//!
//! Produces the line-numbered canonical text body plus a per-line source-page
//! map. The canonical body IS the citation contract: `verifier.rs` and the
//! DocumentViewer treat a document as `content.lines()`, 1-indexed, so the
//! body stores one `\n`-terminated line per logical line with NO line-number
//! prefixes embedded in the text itself.

use anyhow::bail;
use pdfium_render::prelude::*;
use std::sync::Mutex;

/// PDFium is NOT thread-safe: concurrent FPDF_* calls from multiple threads
/// crash the process (observed as SIGTRAP in tests). All native access is
/// serialized through this process-wide lock.
static PDFIUM_LOCK: Mutex<()> = Mutex::new(());

/// Reject PDFs larger than 50 MB before they reach the native parser (T-02-01).
pub const MAX_PDF_BYTES: usize = 50 * 1024 * 1024;

/// Cap the number of pages fed to extraction (T-02-01).
pub const MAX_PAGES: usize = 2000;

/// The line-numbered canonical body of one document.
///
/// `page_of_line[i]` is the 1-indexed source page of 0-indexed line `i` of
/// `text`. Invariant: `page_of_line.len() == text.lines().count()`.
#[derive(Debug)]
pub struct Canonical {
    pub text: String,
    pub page_of_line: Vec<u32>,
}

/// Result of extracting a whole PDF from bytes.
#[derive(Debug)]
pub struct Extracted {
    pub canonical: Canonical,
    pub page_count: u32,
}

/// Build the canonical body + page map from an already-open PDF document.
///
/// A page whose text extraction fails (garbled/empty page) contributes no
/// lines — best effort, never a panic.
pub fn build_canonical(doc: &PdfDocument) -> anyhow::Result<Canonical> {
    let mut pages: Vec<String> = Vec::new();
    for page in doc.pages().iter() {
        match page.text() {
            Ok(text) => pages.push(text.all()),
            Err(e) => {
                // Best effort: log the failure (counts only, never content —
                // T-02-03) and treat the page as empty.
                tracing::warn!(page_index = pages.len(), error = ?e, "pdfium page text extraction failed; treating page as empty");
                pages.push(String::new());
            }
        }
    }
    Ok(canonical_from_pages(pages.iter().map(String::as_str)))
}

/// Extract a PDF from raw bytes into the canonical body + page map.
///
/// Binds PDFium once per call: first the bundled library at `/app`
/// (Docker runtime image), falling back to the system library search path.
/// All pdfium failures are returned as `Err`, never panics. Native access is
/// serialized process-wide (PDFium is single-threaded only).
pub fn extract_from_bytes(bytes: &[u8]) -> anyhow::Result<Extracted> {
    // Size guard BEFORE any native code sees the input (T-02-01).
    if bytes.len() > MAX_PDF_BYTES {
        bail!(
            "PDF too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_PDF_BYTES
        );
    }

    let _pdfium_guard = PDFIUM_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("/app"))
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|e| anyhow::anyhow!("failed to load PDFium native library: {e:?}"))?;
    let pdfium = Pdfium::new(bindings);

    let doc = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| anyhow::anyhow!("pdfium failed to open PDF: {e:?}"))?;

    // PdfPageIndex is c_int; a negative count never occurs in practice.
    let page_count = u32::try_from(doc.pages().len()).unwrap_or(0);
    if page_count as usize > MAX_PAGES {
        bail!("PDF has too many pages: {page_count} (max {MAX_PAGES})");
    }

    let canonical = build_canonical(&doc)?;
    // T-02-03: log page/line counts only, never document contents.
    tracing::info!(
        pages = page_count,
        lines = canonical.page_of_line.len(),
        "extracted PDF into canonical body"
    );
    Ok(Extracted {
        canonical,
        page_count,
    })
}

/// Pure canonical-body builder over per-page text (1 entry per page, in page
/// order). Kept separate from pdfium so the line/page-map invariants are unit
/// testable without the native library.
fn canonical_from_pages<'a, I>(pages: I) -> Canonical
where
    I: IntoIterator<Item = &'a str>,
{
    let mut text = String::new();
    let mut page_of_line = Vec::new();
    for (page_idx, page_text) in pages.into_iter().enumerate() {
        for line in page_text.lines() {
            text.push_str(line);
            text.push('\n');
            page_of_line.push(page_idx as u32 + 1); // 1-indexed page
        }
    }
    Canonical { text, page_of_line }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Pure line/page-map assertions (no native library needed) ----

    #[test]
    fn two_page_lines_map_to_their_source_pages() {
        let canonical = canonical_from_pages(["regel een\nregel twee", "regel drie"]);
        assert_eq!(canonical.text, "regel een\nregel twee\nregel drie\n");
        assert_eq!(canonical.page_of_line, vec![1, 1, 2]);
    }

    #[test]
    fn page_map_length_matches_line_count() {
        let canonical = canonical_from_pages(["a\nb\nc", "d\ne", "f"]);
        assert_eq!(canonical.page_of_line.len(), canonical.text.lines().count());
    }

    #[test]
    fn empty_page_yields_no_lines_but_map_stays_consistent() {
        // Middle page is empty (garbled page treated as empty): it contributes
        // zero lines and page numbering of surrounding pages is unaffected.
        let canonical = canonical_from_pages(["pagina een", "", "pagina drie"]);
        assert_eq!(canonical.text, "pagina een\npagina drie\n");
        assert_eq!(canonical.page_of_line, vec![1, 3]);
        assert_eq!(canonical.page_of_line.len(), canonical.text.lines().count());
    }

    #[test]
    fn fully_empty_document_yields_empty_body_and_empty_map() {
        let canonical = canonical_from_pages(["", ""]);
        assert_eq!(canonical.text, "");
        assert!(canonical.page_of_line.is_empty());
    }

    #[test]
    fn crlf_page_text_is_normalized_to_lf_lines() {
        // pdfium may emit \r\n between layout lines; `lines()` strips the \r.
        let canonical = canonical_from_pages(["regel een\r\nregel twee"]);
        assert_eq!(canonical.text, "regel een\nregel twee\n");
        assert_eq!(canonical.page_of_line, vec![1, 1]);
    }

    #[test]
    fn oversized_input_is_rejected_before_parsing() {
        // The size guard fires before PDFium is bound, so this runs without
        // the native library present.
        let too_big = vec![0u8; MAX_PDF_BYTES + 1];
        let err = extract_from_bytes(&too_big).unwrap_err();
        assert!(err.to_string().contains("PDF too large"));
    }

    // ---- Native-library tests (need libpdfium.so; run inside Docker) ----

    #[test]
    #[ignore = "requires libpdfium.so at /app or on the system library path"]
    fn fixture_two_pages_extract_with_correct_page_map() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/two_page.pdf"
        ));
        let extracted = extract_from_bytes(bytes).expect("fixture PDF should extract");
        assert_eq!(extracted.page_count, 2);

        let canonical = &extracted.canonical;
        assert_eq!(canonical.page_of_line.len(), canonical.text.lines().count());

        // Every line of page-1 text maps to page 1, page-2 text to page 2.
        for (i, line) in canonical.text.lines().enumerate() {
            if line.contains("Pagina een") {
                assert_eq!(canonical.page_of_line[i], 1, "line {i:?} ({line}) should map to page 1");
            }
            if line.contains("Pagina twee") {
                assert_eq!(canonical.page_of_line[i], 2, "line {i:?} ({line}) should map to page 2");
            }
        }
        assert!(canonical.text.contains("Pagina een regel een"));
        assert!(canonical.text.contains("Pagina een regel twee"));
        assert!(canonical.text.contains("Pagina twee regel een"));
    }

    #[test]
    #[ignore = "requires libpdfium.so at /app or on the system library path"]
    fn garbled_bytes_error_without_panicking() {
        let res = extract_from_bytes(b"this is definitely not a PDF document");
        assert!(res.is_err());
    }
}
