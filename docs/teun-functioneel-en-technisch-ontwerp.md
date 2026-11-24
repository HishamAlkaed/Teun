# Teun — functionele requirements & technisch ontwerp

## 1. Product en context

**Teun** is een digitale assistent voor hypotheekadviseurs/acceptanten van **MUNT Hypotheken** (DMFCO). Hij beantwoordt vragen over het acceptatiebeleid, uitsluitend op basis van drie officiële beleidsdocumenten in `resources/acceptatie/`:

1. `Handboek acceptatie versie 2026.3.md` — standaard acceptatiecriteria
2. `MUNT Hypotheekgids 2026.5.md` — uitgebreide hypotheekgids
3. `MUNT Voorleggids 2026.001.md` — uitzonderingen die de **acceptant zelf binnen mandaat** mag afhandelen (géén lijst van zaken die voorgelegd moeten worden)

Tagline: "Teun je hypotheek geheugensteun".

---

## 2. Functionele requirements

### 2.1 Chat / vraag beantwoorden

- Adviseur stelt vraag → Teun zoekt in beleidsdocumenten → geeft antwoord in Nederlands met **expliciete bronverwijzingen** (document, sectie, citaat, regelnummer).
- Antwoord wordt **gestructureerd geclassificeerd** in één van drie categorieën:
  - `standard` — antwoord uit Handboek/Hypotheekgids
  - `mandaat_uitzondering` — situatie uit Voorleggids; acceptant kan binnen mandaat afhandelen. Antwoord krijgt kop `## Acceptanten-Mandaat (voorleggids)`
  - `doorverwijzen_speciale_afhandeling` — buiten beleid; verwijst naar **MUNT Maatwerkdesk** of **MUNT Voorlegdesk** (vaste contactgegevens uit het system prompt)
- Twee chatmodi (instelbaar):
  - **tools mode** — agent doorzoekt documenten met Grep/Read tools (preciezer, langzamer)
  - **inline mode** — alle docs in system prompt (sneller, met prompt caching)
- Twee zoekdieptes: `quick` (max 3 searches) of `thorough`.
- Taal: standaard NL; UI-toggle naar EN.
- Antwoorden **streamen** via Server-Sent Events; tussenstappen (thinking, tool_use, partial) zijn zichtbaar in de UI.

### 2.2 Sessies en geschiedenis

- Gesprekken worden persistent opgeslagen per session_id (PostgreSQL).
- Sidebar met sessielijst; vervolgvragen hervatten dezelfde sessie.
- Standaard retentie: 90 dagen (configureerbaar via `CHAT_RETENTION_DAYS`).
- Sessies handmatig verwijderbaar.

### 2.3 Kwaliteitsbewaking (judge pipeline)

Elk antwoord wordt automatisch beoordeeld in twee fases:

- **Fase 1 — programmatische verificatie**: voor elke geciteerde bron wordt het citaat opgezocht in het document (fuzzy match op woord-LCS, tolerant voor markdown), met statussen `ok` / `document_not_found` / `line_range_out_of_bounds` / `quote_mismatch`.
- **Fase 2 — LLM-faithfulness check**: Haiku scoort het antwoord 1–100 op trouwheid aan geverifieerde bronnen.
- Tools-mode: bij score < `JUDGE_RETRY_THRESHOLD` (default 50) wordt het antwoord automatisch hergenereerd met feedback uit de judge; beste antwoord wint.

### 2.4 Feedback van gebruikers

- Per assistant-bericht: approved / partial / rejected, met optionele opmerking (max 2000 chars).
- Admin-dashboard toont 7-daagse trend en recente negatieve feedback.

### 2.5 Evaluatie (admin)

- Beheer van **testvragen** (vraag, verwachte categorie, verwachte kernpunten, punten, beschrijving).
- **Eval runs**: voert alle testvragen tegen Teun uit, scoort met LLM-judge (1–5, pass/partial/fail), aggregeert in summary (pass_rate, avg score).
- Runs zijn stopbaar; resultaten persistent zichtbaar.

### 2.6 PII-scrub (optioneel)

- Optionele integratie met externe scrub-service om BSN, e-mail, telefoonnummers etc. uit input te verwijderen vóór verzending naar het LLM.
- OAuth2 token client met automatische refresh.

### 2.7 Documentenviewer

- Inkijk in de drie beleidsdocumenten met line-range highlighting; gebruikers kunnen vanuit bronverwijzingen direct naar de regel springen.

### 2.8 Admin-functies

- Tabbed admin (`/admin`): Evaluatie, Vragen, Feedback, Documentatie, Instellingen.

### 2.9 Niet-functionele eisen

- Anti-prompt-injection: end-of-options marker (`--`) richting Claude CLI, validatie session_id format, path-traversal guard op documentnamen.
- Rate limiting per IP (optioneel via `RATE_LIMIT_PER_SECOND`).
- Max bericht 10 000 chars; max request body 64 KB.
- Antwoord-streaming overleeft tab-disconnects (persistence channel ontkoppeld van SSE).

---

## 3. Technisch ontwerp

### 3.1 Architectuur

```
┌──────────────┐    HTTPS/SSE     ┌──────────────────────────┐
│ React (Vite) │ ───────────────► │ axum (Rust) op poort 3000│
│ teun-web SPA │                  │  - serveert SPA (dist)   │
└──────────────┘                  │  - REST + SSE API        │
                                  └────────┬─────────────────┘
                                           │
              ┌────────────────────────────┼─────────────────────────┐
              ▼                            ▼                         ▼
       ┌─────────────┐            ┌──────────────────┐       ┌────────────────┐
       │ PostgreSQL  │            │ Claude (CLI of   │       │ Scrub service  │
       │ - sessions  │            │ Messages API)    │       │ (PII, optional)│
       │ - messages  │            │ + Haiku judge    │       └────────────────┘
       │ - eval runs │            └──────────────────┘
       │ - questions │
       │ - settings  │
       └─────────────┘
```

### 3.2 Backend (Rust, axum 0.8, tokio)

Workspace `apps/teun/service`:

- **Entrypoint** `main.rs`: bouwt `AppState`, draait migraties, spawnt sessie-cleanup-loop, mountt routers, serveert SPA-fallback.
- **Routes** (`routes/`):

  - `POST /api/teun/chat` — SSE-stream van `thinking`, `tool_use`, `partial`, `result`, `judge`, `error` events
  - `GET/DELETE /api/teun/sessions[/:id]`, `PUT .../messages/:id/feedback`
  - `GET/POST/PUT/DELETE /api/teun/questions[/:id]`
  - `POST/GET /api/teun/eval/runs`, `POST .../stop`
  - `GET /api/teun/feedback/{stats,recent}`
  - `GET/PUT /api/teun/settings`
  - `GET /api/teun/documents[/:filename]?line_range=`
  - `POST /api/teun/scrub`
  - `GET /api/teun/health`
- **Agent-laag** (`agent/`):

  - `claude.rs` — spawnt **Claude Code CLI** subprocess in tools-mode, parseert `--output-format stream-json`, vangt `Read`/`Grep` tool-aanroepen op als `ToolEvidence` (welke regels zijn echt gelezen).
  - `inline.rs` — direct **Anthropic Messages API**-call met system prompt + alle docs inline (regelgenummerd), met `cache_control: ephemeral` op de grote policy-blob; streaming SSE-parser zoekt `\n---JSON---` separator om antwoord van metadata te scheiden (UTF-8-veilig).
  - Output: `MortgageAnswer { answer, rationale, sources[], category }`.
- **Judge** (`judge/`):

  - `verifier.rs` — voor elke `SourceReference` zoekt het citaat fuzzy (woord-LCS ≥ 0.7) eerst binnen tool-evidence-ranges, dan in hele document; berekent `line_range` programmatisch.
  - `llm.rs` — vraagt Haiku `claude-haiku-4-5-20251001` om faithfulness-score 1–100 + reasoning, met timeout (default 15s).
- **Eval-laag** (`eval/`):

  - `runner.rs` roept zichzelf aan via `POST /api/teun/chat` (localhost), parseert SSE, geeft door aan `judge.rs` voor 1–5 pass/partial/fail-verdict.
- **Sessie-store** (`session/store.rs`): atomic `append_messages` in transactie (geen read-modify-write race).
- **Persistence**: PostgreSQL via `sqlx` met `sqlx::migrate!` (`migrations/`). Tabellen: `sessions`, `messages` (met JSONB `structured_answer`, `timeline`, `judge_result`), `test_questions`, `eval_runs`, `eval_results`, `settings`.
- **Token client** `token_client.rs`: OAuth2 client-credentials voor scrub-service.
- **Error handling** `error.rs`.

### 3.3 Frontend (React 19 + Vite + TypeScript + Tailwind 4)

`apps/teun/web/`:

- **Routes** (`App.tsx`): `/` landing, `/chat[/:sessionId]` chat, `/admin/*` beheer.
- **`useChat` hook** (`useChat.ts`) — POST naar `/api/teun/chat`, leest SSE via `sse.ts`, bouwt incrementele `ChatMessage` met `timeline[]`, `structuredAnswer`, `judgeResult`. Overleeft session-wissel: pending state in `useRef`.
- **`useSettings`** — chatmode, taal, search depth, scrub toggle.
- **Chat-componenten**: `ChatContainer`, `UserMessage`, `AssistantResponse` (rendert markdown via `react-markdown`), `StructuredAnswer`, `SourceReference` (klikbaar naar `DocumentViewer`), `JudgePanel`, `ReasoningSidebar`, `FeedbackPanel`, `VoiceButton` (browser + server STT), `SessionSidebar`.
- **Admin-tabs**: `EvalTab`, `QuestionsTab`, `FeedbackTab`, `DocsTab`, `SettingsPanel`.

### 3.4 Configuratie (env vars)

Belangrijkste:

- `DATABASE_URL`, `PORT` (default 3000), `STATIC_DIR`
- `CLAUDE_MODEL` (default `claude-sonnet-4-6`), `INLINE_MODEL` (default `claude-opus-4-6`)
- `JUDGE_MODEL` (default `claude-haiku-4-5-20251001`), `JUDGE_API_KEY` / `ANTHROPIC_API_KEY`, `JUDGE_TIMEOUT_SECS`, `JUDGE_RETRY_THRESHOLD`, `JUDGE_MAX_RETRIES`
- `RESOURCES_DIR`, `SKILL_PATH`, `WORKING_DIR`
- `SCRUB_SERVICE_URL` + OAuth2 vars
- `CHAT_RETENTION_DAYS` (default 90), `RATE_LIMIT_PER_SECOND`, `RATE_LIMIT_BURST`

### 3.5 Deployment (`Dockerfile`)

Multi-stage:

1. **frontend** (`node:22`) — `npm ci && npm run build` → `/app/dist`
2. **builder** (`rust:1.92`) — `cargo build --release` met dependency-caching truc (dummy main.rs)
3. **runtime** (`debian:bookworm-slim`) — Node 22 + `@anthropic-ai/claude-code` CLI globaal geïnstalleerd voor tools-mode; binary + dist + config + resources gekopieerd; non-root `app` user; credentials gemount op `~/.claude/.credentials.json`.

CI/CD: push naar `main` → tests → Docker build → registry → auto-deploy (NextEpoch-pipeline).

---

## 4. Belangrijke ontwerpkeuzes / invarianten

- **Bronverificatie is hard requirement**: zonder geldig citaat op de aangegeven regels wordt een bron als onbetrouwbaar gemarkeerd; judge prompt instrueert om die niet mee te wegen.
- **Twee execution modes** delen één antwoordformaat (`MortgageAnswer`) en één judge-pipeline — eval-runner kan beide modi testen via één endpoint.
- **Persistence ontkoppeld van SSE**: unbounded `persist_tx` channel zorgt dat sessieopslag doorgaat ook als browser disconnect.
- **Voorleggids-semantiek** is expliciet ingebakken in zowel system prompt als UI-tekst — "uitzonderingen binnen mandaat", niet "voorleggen".
