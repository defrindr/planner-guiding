# ShopCore Layered Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the migrated ShopCore service into maintainable Rust layers while preserving the legacy HTTP, command, database, webhook, and black-box behavior.

**Architecture:** Keep Axum as the transport boundary, introduce typed request/response models and a centralized error mapper, move use cases into `application/`, keep SQLx queries in `repository/`, and leave configuration/CLI/runtime wiring in dedicated modules. The current `ShopCore` behavior becomes application services rather than a monolithic dispatcher.

**Tech Stack:** Rust 1.85-compatible dependencies, Axum 0.7, Tokio, SQLx 0.8/PostgreSQL, Serde, Reqwest, Flate2, Clap.

**Spec:** `MIGRATION_GUIDE.md`, `legacy/docs/openapi.yaml`, and `docs/COMPATIBILITY_MATRIX.md`.

## Global Constraints

- Preserve public URL/method/status/body/header behavior from the legacy service and OpenAPI contract.
- Build must succeed with the repository `Dockerfile` using Rust 1.85 and `cargo build --locked`.
- Runtime must use PostgreSQL and the included bank simulator; it must not depend on `legacy/`.
- Every error response must remain a JSON object with an `error` string.
- Money and date serialization must remain compatible with the legacy JSON contract.
- Do not commit secrets, runtime data, `target/`, Python caches, or local environment files.
- Run `cargo fmt --check`, `cargo test --locked`, Docker Compose build/start, and black-box smoke tests before completion.

## Review Focus

- Malformed JSON and wrong JSON field types must return JSON errors, never framework-default HTML/plain text.
- Duplicate requests and repeated bank settlement callbacks must not corrupt balances, stock, or payment state.
- Pagination, sorting, filtering, report grouping, and empty result sets must be deterministic.
- Export/download filenames must remain inside `EXPORT_DIR` and retain the exact gzip/CSV contract.
- Maintenance commands must work against a clean PostgreSQL database and must not require the HTTP server.

### Task 1: Establish typed layers and error boundary

**Files:**
- Create: `src/models.rs`
- Create: `src/errors.rs`
- Create: `src/application/mod.rs`
- Create: `src/application/context.rs`
- Modify: `src/http/models.rs`, `src/http/error.rs`, `src/main.rs`
- Test: `tests/unit_models.rs` or module tests in the new files

**Interfaces:**
- `ApplicationContext { pool: PgPool, config: Config, http_client: reqwest::Client }` is the shared application dependency container.
- `AppError` maps to `ApiError` with status and `{"error": message}`.
- Domain/application DTOs use explicit Serde structs for users, products, orders, payments, wallet, reports, and errors.

- [ ] Write tests for error serialization, malformed input mapping, and representative DTO serialization.
- [ ] Run the focused tests and verify they fail against the current untyped boundary.
- [ ] Add typed models, `AppError`, and context; map all errors centrally.
- [ ] Run focused tests and then `cargo test --locked`.
- [ ] Commit `refactor: add typed application boundary`.

### Task 2: Extract repositories and application services

**Files:**
- Create: `src/repository/users.rs`, `products.rs`, `orders.rs`, `wallet.rs`, `reports.rs`, `payments.rs`
- Create: `src/application/auth.rs`, `catalog.rs`, `orders.rs`, `wallet.rs`, `reports.rs`, `payments.rs`
- Modify: `src/repository.rs`, `src/core.rs`
- Test: `tests/application_services.rs`

**Interfaces:**
- Repository functions accept `&PgPool` or `&mut Transaction<'_, Postgres>` and return typed records/results.
- Application functions accept `&ApplicationContext` plus typed request DTOs and return typed response DTOs.
- `create_order`, `pay_order`, `cancel_order`, `refund_order`, and `transfer_wallet` retain transaction boundaries.
- Bank settlement remains HMAC-verified at transport and idempotent at application/repository layers.

- [ ] Add service tests for registration/login, stock reservation, coupon rules, wallet transfer, payment state transitions, and report aggregation.
- [ ] Verify tests fail or expose the current monolith coupling.
- [ ] Extract SQL and business rules into focused repository/service modules without changing route behavior.
- [ ] Keep `src/core.rs` as a temporary compatibility adapter only; it must delegate rather than contain SQL/business branches.
- [ ] Run `cargo fmt --check` and the full Rust test suite.
- [ ] Commit `refactor: extract ShopCore application services`.

### Task 3: Make transport and CLI thin adapters

**Files:**
- Create: `src/http/handlers.rs`, `src/http/auth.rs`, `src/http/routes.rs` replacement or split modules
- Create: `src/cli.rs`
- Modify: `src/http/mod.rs`, `src/main.rs`, `src/config.rs`
- Test: `tests/http_contract.rs`, `tests/cli_contract.rs`

**Interfaces:**
- Handlers parse Axum input, call one application function, and serialize one response; no SQL is allowed in HTTP modules.
- Bearer auth returns 401 for missing/invalid tokens and 403 for authenticated non-admin users.
- CLI supports `init-db`, `seed`, `import-products <csv>`, `export-sales <file>`, `purge-sessions`, and `serve`.
- `HOST`, `PORT`, `DATABASE_URL`, `BANK_URL`, `BANK_SECRET`, and `EXPORT_DIR` are explicit configuration inputs.

- [ ] Add contract tests for status/body/error shape, auth boundaries, malformed JSON, and CLI command exit behavior.
- [ ] Run them against the current transport and record failures.
- [ ] Split handlers/auth/routes and CLI wiring, delegating to application services.
- [ ] Run Rust tests and the HTTP black-box suite against Compose.
- [ ] Commit `refactor: thin HTTP and CLI adapters`.

### Task 4: Complete compatibility coverage and delivery verification

**Files:**
- Modify: `tests/test_blackbox_smoke.py`, `tests/requirements.txt`, `scripts/test-compose.sh`, `README.md`
- Create: `tests/fixtures/`, `tests/legacy_compare.py` if legacy execution is available
- Modify: `docs/COMPATIBILITY_MATRIX.md`, `MIGRATION_GUIDE.md` only for verified decisions

- [ ] Add tests for empty/malformed bodies, wrong types, duplicate requests, pagination, empty reports, export/download safety, maintenance commands, and repeated webhook delivery.
- [ ] Add legacy-vs-Rust fixture comparison for representative success/error responses when both stacks can run.
- [ ] Run `cargo fmt --check`, `cargo test --locked`, `docker compose build --no-cache`, `docker compose up -d`, and all black-box tests.
- [ ] Verify `docker compose ps` reports healthy `db`, `bank`, and `app`, and `/health` returns `{"status":"ok"}`.
- [ ] Update README with exact commands, environment variables, deliberate behavior changes, and schema status.
- [ ] Remove build/cache artifacts, verify `git status` is clean, and commit `test: complete migration compatibility coverage`.

## Completion Criteria

- `src/core.rs` is no longer the owner of SQL and business behavior.
- HTTP handlers and CLI contain no persistence/business SQL.
- All Rust tests and black-box tests pass.
- Clean Docker Compose build/start succeeds with the locked dependency graph.
- The final commit is pushed only after the local tree and remote commit are verified.
