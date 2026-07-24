---
phase: 02-admin-documents
plan: 02
subsystem: admin-documents-frontend
tags: [react, document-manager, upload, polling, admin-ui]
requires:
  - "02-01: POST/GET/DELETE /api/teun/admin/documents + GET /api/teun/documents/{filename}/pdf (endpoint contract)"
provides:
  - "Documentatie admin tab: functional document manager (upload/list/delete/serve) — ADM-04"
affects: []
tech-stack:
  added: []
  patterns:
    - "self-scheduling setTimeout poll (not setInterval): refresh() fetches, clears any pending timer, re-arms only if any doc is pending/indexing — avoids overlapping timers and stops cleanly once all docs are stable"
    - "Dutch status-badge convention reused from EvalTab/QuestionsTab (bg-{color}-100 text-{color}-700 pill), extended with pending/indexing/indexed/error labels for documents"
    - "collapsible help section via useState(false) toggle button, same idiom as JudgePanel's Toon/Verberg details"
key-files:
  created:
    - apps/teun/web/src/components/DocumentManager.tsx
  modified:
    - apps/teun/web/src/components/DocsTab.tsx
    - apps/teun/web/src/lib/api.ts
decisions:
  - "Upload UI is a single dropzone (drag-drop + click-to-browse) that uploads immediately on file selection, showing a busy state ('Uploaden...'/'Bezig met uploaden...') rather than a two-step select-then-confirm flow — matches the plan's 'upload button with busy state' requirement while keeping the interaction one step"
  - "202 upload response has no id (per 02-01 contract): uploadDocuments() returns UploadResult[] (filename+status only) purely for error surfacing; the manager always re-fetches the full list via refresh() after upload/delete rather than merging the 202 body into state"
  - "Delete confirmation uses window.confirm per explicit plan instruction, diverging from QuestionsTab's inline confirm-row pattern used elsewhere in this admin"
metrics:
  duration: "~20 min"
  completed: "2026-07-24"
---

# Phase 2 Plan 02: Frontend Document Manager Tab Summary

**One-liner:** Drag-drop multi-file PDF upload + polling status table + delete (window.confirm) wired to the 02-01 admin-documents endpoints, replacing the static Documentatie tab while keeping all existing help content in a closed-by-default collapsible section.

## What Was Built

### Task 1 — DocumentManager component + tab rewire (commit a154d00)

- **`lib/api.ts`**: added `AdminDocument`/`UploadResult` types and `listAdminDocuments()`, `uploadDocuments(files)`,
  `deleteDocument(id)` following the existing wrapper conventions (`fetch` + `res.ok` check + `{error}` body
  surfaced via `Error`, matching `createQuestion`/`deleteQuestion` style). `uploadDocuments` builds a
  `FormData` with repeated `file` fields per the 02-01 multipart contract and throws the exact backend error
  string on a whole-request 400.
- **`components/DocumentManager.tsx`** (new, ~245 lines): a single dashed dropzone (drag-drop + click-to-browse,
  `accept="application/pdf"`, `multiple`) that uploads on selection with a busy state; a table listing filename
  (linking to `GET /api/teun/documents/{filename}/pdf` in a new tab), Dutch status badge (Wachtend/Bezig/
  Geïndexeerd/Fout — reusing EvalTab's pill styling extended with a `blue` "indexing" tone), human-readable size
  (B/KB/MB), page count, chunk count, indexed-at date (`nl-NL` locale, same `formatDate` pattern as EvalTab), and
  a delete button gated by `window.confirm`. Error rows show `error_message` both as a `title` tooltip on the
  badge and as inline red text under it.
  Polling: a single `refresh()` (via `useCallback`, stable identity) fetches the list, clears any pending
  timer, and re-arms a `setTimeout` 3s later only if any document is `pending`/`indexing`; called on mount and
  after every upload/delete so newly-created pending rows immediately resume polling without a page reload.
  Self-scheduling `setTimeout` (not `setInterval`) avoids overlapping in-flight requests and naturally stops once
  all documents reach `indexed`/`error`.
- **`components/DocsTab.tsx`**: `DocumentManager` rendered at the top; all 8 existing static `<Section>` blocks
  (Wat is Teun?, Hoe gebruik je Teun?, Betrouwbaarheidsscore, Chat modi, PII-bescherming, Gesprekken en opslag,
  Taal, Hulp nodig?) preserved verbatim, moved under a `border-t` divider behind a "▸ Toon help & uitleg" /
  "▾ Verberg help & uitleg" toggle button (`useState(false)`, same `showDetails`-toggle idiom as `JudgePanel`),
  defaulting closed.

## Verification Results

- **Typecheck**: `tsc -b` — clean, no diagnostics (ran directly on host; pure JS, unaffected by the host
  environment issue below).
- **Build**: `npm run build` (`tsc -b && vite build`) could not run directly on the Windows host — see
  "Deviations" below for why and how it was verified instead. Authoritative result via
  `docker build -f apps/teun/Dockerfile --target frontend .` (the exact `node:22-bookworm-slim` stage the
  production image uses):
  ```
  > teun-web@0.1.0 build
  > tsc -b && vite build

  vite v6.4.1 building for production...
  transforming...
  ✓ 234 modules transformed.
  rendering chunks...
  computing gzip size...
  dist/index.html                   0.53 kB │ gzip:   0.36 kB
  dist/assets/index-OzUdw7vA.css   61.67 kB │ gzip:  10.87 kB
  dist/assets/index-B2zwqWPD.js   464.57 kB │ gzip: 139.33 kB
  ✓ built in 2.86s
  ```
  Build succeeded, 0 errors, 234 modules transformed (up from the pre-existing baseline — new component compiles
  and bundles cleanly).
- **Tests**: `npx vitest run` inside the same Linux toolchain (`node:22-bookworm-slim`) — pre-existing suite
  unaffected: `3 passed (3 files) / 20 passed (20 tests)` (`chat/lib/api.test.ts`, `chat/hooks/useSettings.test.tsx`,
  `chat/lib/sse.test.ts`). No new tests were required by the plan (no `tdd="true"` on this task) and none of the
  3 existing test files touch documents/admin.
- **Lint** (extra due-diligence, not part of the plan's verify gate): `eslint` on the 3 files this plan
  created/modified is clean (0 errors). Repo-wide `eslint src/` surfaces 30 pre-existing errors in unrelated
  `chat/*` files — out of scope, logged to `deferred-items.md`, not touched.

## Deviations from Plan

### Auto-fixed / adjusted

**1. [Rule 3 - blocking issue, environment] `npm run build` cannot run directly on the Windows host — verified via the Dockerfile's `frontend` stage instead**
- **Found during:** Task 1 verification step.
- **Issue:** This host's Windows Group Policy blocks execution of unsigned native binaries from user directories
  (`ERROR_ACCESS_DISABLED_BY_POLICY`, code `1260`). `tsc -b` (pure JS) runs fine, but the `vite build` step spawns
  `esbuild.exe` natively to bundle `vite.config.ts` and fails with `spawn UNKNOWN` before that policy block was
  isolated (confirmed via a standalone batch-file repro: `Dit programma wordt geblokkeerd door Groepsbeleid`).
  This is a host-only restriction — not fixable by the executor (no admin rights, and modifying OS security
  policy is out of scope for a code-execution task) — and does not affect the real build/deploy path, since
  `apps/teun/Dockerfile`'s `frontend` stage already builds the exact same `npm run build` inside
  `node:22-bookworm-slim` (Linux) for the production image.
- **Fix:** Ran `docker build -f apps/teun/Dockerfile --target frontend -t teun-web-buildcheck .` to execute the
  identical `npm run build` command in the production toolchain; result captured above. The throwaway image was
  removed after verification (`docker rmi teun-web-buildcheck`). Tests were run the same way
  (`docker run node:22-bookworm-slim sh -c "npm ci && npx vitest run"`, bind-mounting the web directory).
- **Files modified:** None (verification-only; no code changes).
- **Commit:** n/a (verification step, not a code change).
- **Note for future local work:** the host's `node_modules` now contains Linux-built native packages (from the
  container's `npm ci` writing into the bind-mounted directory) rather than Windows ones; it is gitignored and
  unaffected by version control. Anyone building on this Windows host locally should route `npm ci`/`npm run
  build`/tests through Docker rather than the host toolchain until the Group Policy restriction is lifted.

### Out-of-scope discoveries (not fixed — logged)

- Pre-existing ESLint errors (30, across unrelated `chat/*` files) and a tsconfig `include`-scoping issue on
  the 3 pre-existing `*.test.ts(x)` files. Logged in
  `.planning/phases/02-admin-documents/deferred-items.md`. Not touched — out of this plan's scope per the
  executor's scope-boundary rule.

## Known Stubs

None. `DocumentManager` is fully wired to the live 02-01 endpoints (list/upload/delete/pdf-serve); no mock or
placeholder data paths.

## Threat Flags

None. No new network surface introduced beyond what 02-01 already exposes; the frontend only consumes the
existing admin endpoints (no in-service auth, per the 02-01 plan-mandated deployment model where the portal
fronts `/admin`). Filenames used in the PDF link are URL-encoded (`encodeURIComponent`) before being placed in
the `href`, consistent with the backend's own filename sanitization.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | a154d00 | feat(02-02): add document manager UI to admin Documentatie tab |

## Self-Check: PASSED

- Files exist: `apps/teun/web/src/components/DocumentManager.tsx` (created), `DocsTab.tsx` and `lib/api.ts`
  (modified) — confirmed via `git show --stat a154d00`.
- Commit `a154d00` present on `gsd/rag-rebuild` (`git log --oneline -1`).
- Must-have truths: upload (multi-file) + list (status/chunk_count/size) + delete (confirm) all present in
  `DocumentManager.tsx`; polling only while pending/indexing (self-scheduling `setTimeout`, verified by code
  inspection — no doc data available to exercise live status transitions in this frontend-only plan, that is
  the orchestrator's live-container verification step per the plan); existing static help content fully
  preserved verbatim inside the closed-by-default collapsible; styling matches EvalTab/QuestionsTab table and
  button class idioms, Dutch labels throughout.

## Self-Check: PASSED (verified by tool)
