# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Offre Taf is a personal IT-job aggregator focused on three views: IT jobs in Burkina Faso collected from employers' own sites, remote jobs (with where they can be done from), and jobs mentioning relocation/visa. Java listings are auto-favourited. Stack: a Rust (axum + sqlx) API that polls sources into PostgreSQL, and a React 19 + TypeScript/Vite UI. `AGENTS.md` holds the same core rules for other agents — keep both files consistent when changing sources, schedules or conventions. User-facing text (README, UI strings, API error messages) is in French.

## Commands

Local stack (copy `.env.example` to `.env` at the root first):

```sh
docker compose up -d db          # PostgreSQL 17 on host port 5433
cargo run -p offre-taf-api       # run from the repo root (reads root .env); listens on API_ADDR (default 127.0.0.1:3000)
cd frontend && npm install && npm run dev   # UI on 127.0.0.1:5173
```

Verification:

```sh
cargo check --workspace          # backend type check (from root)
cargo test -p offre-taf-api      # backend unit tests (classify, ats, burkina, francophone modules)
cargo test -p offre-taf-api <test_name>     # run a single test
cd frontend && npm run build     # tsc -b + vite build; this is the frontend type check
```

There is no linter config and no frontend test runner. On this Windows machine `cargo` may not be on PATH: prefix with `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.

The UI reads `VITE_API_URL` (fallback `http://127.0.0.1:3000`). `.env.example` uses port 3000, but this machine's `.env` sets `API_ADDR=127.0.0.1:43987` and `frontend/.env.local` sets `VITE_API_URL` to the same port — keep them in sync (both files are gitignored).

## Architecture

**Backend (`backend/src/`)**, single binary crate `offre-taf-api` in a Cargo workspace rooted at the repo:

- `main.rs` — HTTP routes and the scheduler loop. At startup it runs `migrations/0001_create_jobs.sql` via `sqlx::raw_sql` (no sqlx migrator; the file is idempotent `CREATE/ALTER ... IF NOT EXISTS`, plus a `DO $$` block for one-time backfills — append schema changes there in the same style). It spawns one `collect_periodically` loop per entry of `sources::schedule()`; `tokio::time::interval` ticks immediately, so every source is polled at startup. Routes: `GET /health`, `GET /api/jobs` (filters `q`, `country`, `work_mode`, `contract_type`, `relocation`, `unread`, `favorite`, `java`, `scope` — `scope=africa` means remote_scope in worldwide/africa/emea; limit 500), `GET /api/jobs/summary` (counts per UI tab), `GET /api/jobs/unread-count`, `PATCH /api/jobs/{id}/read`, `PATCH /api/jobs/{id}/favorite` (JSON `{"favorite": bool}`). CORS only allows the Vite origins on port 5173.
- `sources.rs` — `CollectedJob`, the source registry (`schedule()` + `collect()` dispatch), Remotive/Arbeitnow/OpenToAfrica, `save_job`, `mark_seen_and_prune`.
- `sources/classify.rs` — all text heuristics on accent-folded word tokens: `is_it_role` (re-exported as `is_developer_role`; dev + network/data/security/support, excludes sales/business roles), `mentions_java` (never matches JavaScript), `mentions_relocation` (ignores negated mentions), `remote_scope`, `html_to_text`.
- `sources/ats.rs` — `BOARDS` registry of ~100 employers' Greenhouse/Lever/Ashby boards and `WORKABLE_SEARCHES` against Workable's employer-job index (max 20 per page). Keeps IT roles that are remote, in Burkina Faso, or mention relocation.
- `sources/burkina.rs` — `SITES` registry of Burkina employers' WordPress sites; reads `/wp-json/wp/v2/posts?search=…` for recruitment notices of the last 120 days, drops admissions/results/scholarship posts, keeps IT roles.
- `sources/francophone.rs` — older direct connectors (Wave, Yassir, Gozem, Djamo, EmploiDakar, SABMA, Qualive, BFemploi) and the `FRANCOPHONE_COUNTRIES` allowlist; they keep a listing only when its location/conditions explicitly mention a francophone African country.

**Adding a source:** write `pub async fn xxx(client) -> Result<Vec<CollectedJob>, reqwest::Error>`, add an arm in `sources::collect` and an entry with its interval in `sources::schedule`. Unregistered names hit `unreachable!`. To add an employer, usually just append to `ats::BOARDS` (verify the slug against the board API first) or `burkina::SITES` (verify `/wp-json/wp/v2/posts` responds).

**Persistence:** `jobs.source_url` is `UNIQUE`; `save_job` cleans the description to plain text, computes `relocation_support`, `mentions_java` and `remote_scope` centrally (connectors don't set them), and upserts with `ON CONFLICT (source_url)`, updating only when data changed. New Java listings get `is_favorite = true`; a manual un-favourite sticks unless the listing later starts mentioning Java. After each successful non-empty collection, `mark_seen_and_prune` bumps `last_seen_at` and deletes non-favourite listings of the same `source_name`s unseen for 21 days. Favourites are never pruned.

**Failure model:** a source error, a failing board/site inside a multi-employer source, or a failed row save is logged with `tracing::warn!` and never stops the API or other sources/employers.

**Frontend (`frontend/src/App.tsx`)** is a single component with tabs (Burkina Faso, Télétravail, Relocalisation, Favoris, Toutes) that map to `/api/jobs` params, tab counts from `/api/jobs/summary`, a star to toggle favourites, and a 1-minute refresh.

## Source-specific rules

- Poll intervals: Arbeitnow every `COLLECT_INTERVAL_MINUTES` (default 60); Remotive every 360 min (its API asks for at most four calls/day — do not lower this); Wave/Yassir/Gozem/Djamo every 360 min; EntreprisesATS, Workable, BurkinaEntreprises every 720 min; OpenToAfrica, EmploiDakar, BFemploi, SABMA, Qualive every 1440 min.
- OpenToAfrica dates are derived from relative homepage labels (`parse_relative_date`); after a successful non-empty OpenToAfrica collection, its non-favourite listings with no date or older than 180 days are deleted.
- `relocation_support` and `remote_scope` are keyword heuristics; the UI must present them as "mentioned"/"à vérifier", not guaranteed.
- Many `.bf` sites have expired TLS certificates or are unreachable; rustls rejects them, which is logged per site and is expected.
- Keyword lists in `classify.rs` (`IT_ROLES`, `EXCLUDED_ROLES`, place lists…) match **whole token sequences** after `words()` lowercases, folds accents and splits on non-alphanumerics: `"dev"` does not match `"developer"`, and `"c++"` becomes the single token `"c"`. Add every word form you need and cover new keywords with a `#[test]` in that module.
- All sources share one `reqwest::Client` built in `main.rs` (crate-name User-Agent, 60 s timeout); connectors must not build their own.
