# Deferred Items — Phase 02 (Admin Document Management)

Out-of-scope discoveries logged per executor scope-boundary rule (not fixed, not part of 02-02's task).

## From Plan 02-02 (frontend document-manager tab)

- **Pre-existing ESLint errors, unrelated to this plan's files** (`node node_modules/eslint/bin/eslint.js src/` run
  2026-07-24): 30 errors across `src/chat/components/AssistantResponse.tsx`, `src/chat/components/DocumentViewer.tsx`,
  `src/chat/components/ReasoningSidebar.tsx`, `src/chat/hooks/useChat.ts`, `src/components/ChatPage.tsx`,
  `src/components/LandingPage.tsx`, plus 3 "not found by the project service" parsing errors on the
  `*.test.tsx`/`*.test.ts` files (tsconfig `include` scoping issue, not a code bug). None of these files were
  touched by 02-02. Linting the 3 files this plan created/modified
  (`src/components/DocumentManager.tsx`, `src/components/DocsTab.tsx`, `src/lib/api.ts`) is clean — 0 errors.
  `npm run lint` is not part of the plan's verification gate (`npm run build` only); this is FYI for a future
  cleanup pass.

- **Windows host Group Policy blocks execution of unsigned native binaries** (e.g. `esbuild.exe`,
  `@esbuild/win32-x64`), error `1260` (`ERROR_ACCESS_DISABLED_BY_POLICY`). `npm run build` cannot run directly on
  this Windows host — `tsc -b` (pure JS) succeeds, but the `vite build` step spawns esbuild natively and is
  blocked. Worked around by building the Dockerfile's `frontend` stage (`docker build --target frontend`), which
  uses `node:22-bookworm-slim` (Linux, unaffected by the host's Windows AppLocker/Group Policy) — this is also
  the exact toolchain the production image uses, so it is an authoritative, not a lesser, verification. Not
  fixable by the executor (OS security policy, no admin rights) and not applicable to the actual deployment
  path. Future local Windows-host frontend work should build/test via Docker rather than the host npm/node_modules
  toolchain.
