---
phase: 01-rag-retrieval-core
plan: 02
subsystem: rag-extraction
tags: [pdfium, pdf-extraction, canonical-body, docker, native-lib]
requires:
  - "01-01: rag module scaffolding (mod.rs) + pdfium-render 0.9 in Cargo.toml"
provides:
  - "rag::extract::build_canonical + extract_from_bytes → Canonical { text, page_of_line } (ING-01)"
  - "Docker runtime stage bundles pinned PDFium chromium/7881 at /app/libpdfium.so"
affects: [01-03, 01-04]
tech-stack:
  added: ["pdfium-binaries chromium/7881 (native .so, pinned + sha256-checked)"]
  patterns: ["process-wide Mutex around all PDFium native calls (PDFium is single-threaded only)", "size/page caps before native parsing"]
key-files:
  created:
    - apps/teun/service/src/rag/extract.rs
    - apps/teun/service/tests/fixtures/two_page.pdf
    - apps/teun/service/tests/fixtures/README.md
  modified:
    - apps/teun/service/src/rag/mod.rs
    - apps/teun/Dockerfile
decisions:
  - "PDFium pinned to bblanchon chromium/7881 — the exact build pdfium-render 0.9.3's pdfium_latest feature (pdfium_7881) targets; sha256 1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d verified against the GitHub release asset digest AND re-verified inside docker build"
  - "PDFium is NOT thread-safe: concurrent binding from two test threads crashed with SIGTRAP; all native access serialized via a process-wide static Mutex in extract.rs"
  - "Extraction quality verdict (executor assessment, human gate pending): GOOD on all 4 seed PDFs — no .md fallback needed"
metrics:
  duration: "~45 min"
  completed: "2026-07-24"
---

# Phase 1 Plan 02: PDFium Extraction + Docker Bundling Summary

**One-liner:** pdfium-render extraction into the line-numbered canonical body + per-line page map, with the pinned chromium/7881 libpdfium.so bundled and proven to load + extract all 4 seed PDFs inside the bookworm-slim runtime image.

## What Was Built

### Task 1 — pdfium extraction → canonical body + page map (commits e4fc501, cf7d436)
- `src/rag/extract.rs`: `Canonical { text, page_of_line }` (page_of_line[i] = 1-indexed source page of 0-indexed line i; invariant `page_of_line.len() == text.lines().count()`), `build_canonical(&PdfDocument)`, `extract_from_bytes(&[u8]) -> Extracted { canonical, page_count }`.
- Binding order per research Pitfall 1: `Pdfium::bind_to_library(pdfium_platform_library_name_at_path("/app")).or_else(|_| bind_to_system_library())`.
- T-02-01: >50 MB input rejected BEFORE any native code runs; 2000-page cap; every pdfium failure is `Err`, never a panic; a garbled page is treated as empty (best effort) with a counts-only warn log (T-02-03).
- Deterministic 905-byte 2-page PDF fixture (`tests/fixtures/two_page.pdf`) + generator script documented in `tests/fixtures/README.md`.
- Tests: 6 pure line/page-map tests (run everywhere, no native lib), 2 `#[ignore]`d native tests, 1 `#[ignore]`d env-gated spike harness (`TEUN_SPIKE_PDF`).

### Task 2 — PDFium bundled into the Docker runtime image (commit e08eecb)
- Runtime stage: `libstdc++6` + `libgcc-s1` added to apt-get; pinned download of `pdfium-linux-x64.tgz` from release tag `chromium/7881` with in-build `sha256sum -c` gate; `lib/libpdfium.so` → `/app/libpdfium.so`.
- Node.js/claude CLI install untouched (removal coupled to deleting `run_claude` in Plan 04); `COPY resources/` kept so seed PDFs are in-image.
- Note: `ldd /app/libpdfium.so` in-image shows the bblanchon build links the C++ runtime statically — only `libgcc_s`/libc/libm needed, all resolved. `libstdc++6` installed anyway per plan (harmless, future-proof).

**PDFium pin record (per plan output spec):**
- Release tag: `chromium/7881` (bblanchon/pdfium-binaries)
- Asset: `pdfium-linux-x64.tgz` (3,644,759 bytes)
- SHA-256: `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d` (matches GitHub release asset digest; re-verified during docker build: `/tmp/pdfium.tgz: OK`)
- Compatibility: pdfium-render 0.9.3's `pdfium_latest` feature = `pdfium_7881` — exact match, and empirically proven (all native tests + 4 seed PDF extractions pass against this .so).

## Verification Results

All cargo runs inside the `teun-rust-build` helper image (no Rust toolchain on host).

- `cargo test --package teun rag::extract`: **6 passed, 0 failed, 3 ignored** (pure tests; native ones gated).
- `cargo test --package teun rag::extract -- --ignored` with libpdfium.so at /app: **2 passed** (fixture page-map + garbled-bytes-no-panic). Initially SIGTRAPped under parallel threads → fixed with the PDFium mutex; passes in parallel after the fix.
- `cargo test --package teun rag::`: **12 passed, 0 failed, 5 ignored** (no regressions in store/embed).
- `docker build -f apps/teun/Dockerfile -t teun-pdfium-spike .`: **exit 0**; checksum layer logged `/tmp/pdfium.tgz: OK`.
- In the built runtime image (bookworm-slim, user `app`): native test binary ran `fixture_two_pages_extract_with_correct_page_map`, `garbled_bytes_error_without_panicking` → **ok**; NO `PdfiumError::LoadLibraryError`, NO `symbol lookup error`.

### Seed-PDF extraction (in the runtime image, /app/resources)

| Document | Pages | Lines | Chars |
|----------|------:|------:|------:|
| handboek_acceptatie_versie_2026_4_definitief.pdf | 64 | 1734 | 118,196 |
| MUNT Beheergids 2026.pdf | 17 | 964 | 57,086 |
| MUNT Hypotheekgids 2026-2.pdf | 35 | 2575 | 148,064 |
| MUNT Voorleggids 2026_002.pdf | 34 | 1101 | 75,061 |

(`handboek_accept_versie_2026_4.pdf` skipped — near-duplicate of `_definitief` per CONTEXT.)

## Extraction Quality Verdict (executor assessment — human gate still pending)

**GOOD on all 4 seed PDFs; no `.md` fallback appears necessary.**

Evidence:
- Text is readable Dutch prose in natural reading order; no column scrambling observed in the first-25-line and mid-document samples of each PDF.
- Line breaks follow layout lines (sentences wrap mid-line) but are clean and stable — matching the `content.lines()` model verifier.rs expects.
- Page attribution is correct (cover text → p1, voorwoord → p2, mid-document samples land on plausible pages).
- Quotes cross-match the parallel `.md` bodies verbatim: e.g. "Acceptanten volgen de acceptatierichtlijnen, maar mogen hier gemotiveerd van afwijken…" (handboek, extracted lines 12–13 vs identical `.md` sentence) and "De maximale hoofdsom gaat per 1 juli omhoog naar € 1.350.000" (hypotheekgids, extracted line 9 vs `.md`). Layout line breaks split sentences across lines, so fuzzy matching must tolerate `\n`-joins — chunk-level quotes will match.
- Known artifacts (acceptable): running headers/footers interleave as their own lines (e.g. "MUNTHYPOTHEKEN.NL 29", "HANDBOEK ACCEPTATIE 2026 2"); cover pages produce sparse display-text lines ("HYPOTHEEK2026"). Neither garbles body prose.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] PDFium not thread-safe — SIGTRAP under parallel tests**
- **Found during:** Task 1 verify (`-- --ignored` run)
- **Issue:** Two `#[ignore]`d tests binding PDFium concurrently crashed the test process with SIGTRAP (concurrent `FPDF_*` calls; PDFium is single-threaded only).
- **Fix:** Process-wide `static PDFIUM_LOCK: Mutex<()>` acquired in `extract_from_bytes` before binding; documented in the module. Tests pass in parallel after the fix. This also protects any future concurrent caller (axum handlers in Phase 2 upload).
- **Files modified:** `src/rag/extract.rs`
- **Commit:** e4fc501 (folded into Task 1 commit)

**2. [Rule 3 - Blocking] `PdfPageIndex` is `c_int`, and `Extracted` needed `Debug`**
- Compile errors on first verify: `u32::from(i32)` doesn't exist (used `u32::try_from(...).unwrap_or(0)`) and `unwrap_err()` needs `Debug` on the Ok type (derived `Debug` on `Canonical`/`Extracted`). Commit e4fc501.

**3. [Note] Spike harness test added (not in plan's file list)**
- Env-gated `#[ignore]`d test `spike_extract_pdf_from_env` added to `extract.rs` (commit cf7d436) to automate the checkpoint's seed-PDF quality evidence — `teun` is a bin-only crate, so a cargo example could not reach the module. Kept: it documents how to re-run the spike.

**4. [Note] TDD RED/GREEN collapsed into one commit** — same rationale as Plan 01-01 (tests and implementation in the same Rust module cannot compile independently; orchestrator instructed atomic per-task commits). All behavior-block bullets are covered by tests.

**5. [Note] Expected dead_code warnings** — 7 warnings (new extract API not yet consumed; Plan 03 wires it). Consistent with the Plan 01-01 precedent.

## Operational Note for Plan 03/04

PDFium native access is serialized by `PDFIUM_LOCK`; `extract_from_bytes` is CPU-bound + blocking — call it via `tokio::task::spawn_blocking` from async contexts. Native `#[ignore]` tests need `libpdfium.so` at `/app` (mount or run in the runtime image).

## Known Stubs

None. `rag::extract` is complete and not yet consumed by design (Plan 03 ingest wires it).

## Threat Flags

None beyond the plan's threat model. T-02-01 (size/page caps, no panics), T-02-02 (pinned tag + checksum, in-build verification), T-02-03 (counts-only logging) all implemented.

## Pending Human Checkpoint (BLOCKING — plan not finished)

Task 3 is `checkpoint:human-verify` (quality gate). Automation is done — evidence above. Remaining human decision:

1. Review the extraction samples/verdict above (or re-run the spike:
   `MSYS_NO_PATHCONV=1 docker run --rm -v teun-cargo-target:/target -e TEUN_SPIKE_PDF="/app/resources/acceptatie/<doc>.pdf" --entrypoint /target/debug/deps/teun-8c0fa45d3dae491b teun-pdfium-spike spike_extract_pdf_from_env --ignored --nocapture`).
2. QUALITY GATE DECISION: executor recommends **"approved: PDFium loads, extraction quality acceptable"** (no `.md` fallback). If you disagree for any document, reply `fallback: [docs]` — Plan 03 ingest must then honor that.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | e4fc501 | feat(01-02): add pdfium extraction to canonical body + line-page map |
| 1 (spike harness) | cf7d436 | test(01-02): add env-gated spike harness for seed PDF extraction quality |
| 2 | e08eecb | feat(01-02): bundle pinned PDFium native lib into Docker runtime stage |

## Self-Check: PASSED

- Files exist: `src/rag/extract.rs` (252 lines ≥ min 40), `tests/fixtures/two_page.pdf`, `tests/fixtures/README.md`, Dockerfile contains `libpdfium` + `libstdc` + `bind_to_library|bind_to_system_library` key-link patterns present in extract.rs.
- Commits e4fc501, cf7d436, e08eecb present on `gsd/rag-rebuild`.
- Must-have truths verified: page_of_line length == line count (test), one page per line (test), Docker image loads .so + extracts seed PDF end-to-end (in-image run above).
