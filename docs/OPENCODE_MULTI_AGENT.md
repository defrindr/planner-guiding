# OpenCode Multi-Agent Handoff

## Working rules

- Each agent uses a separate worktree or an explicitly disjoint write set.
- `legacy/` is read-only reference material.
- Do not change public route paths, methods, JSON field names, or status codes without
  updating `docs/COMPATIBILITY_MATRIX.md`, tests, and README.
- Agents must run formatting and focused tests before handoff and report changed paths.
- The integrator owns conflict resolution and final Docker verification.

## Agent assignments

### Contract agent

Owns `docs/` and contract fixtures. Extracts the OpenAPI route matrix, runs legacy
smoke tests, captures representative responses, and records quirks not expressed in
OpenAPI.

### Database/domain agent

Owns `src/domain/`, `src/repository/`, `src/db/`, and `migrations/`. Implements the
legacy schema, seed data, pricing, coupon rules, stock reservation/restock, wallet
transfers, payment/order transitions, and transactional boundaries.

### HTTP/auth agent

Owns `src/http/` and API-facing models. Implements Axum routing, JSON parsing, bearer
auth, admin authorization, users, products, orders, wallet, admin routes, and the bank
webhook.

### Reports/jobs/CLI agent

Owns reporting, exports/downloads, async notifications, and the Rust maintenance
command dispatcher. Must preserve CSV header/content and gzip naming.

### Infrastructure/test agent

Owns `Dockerfile`, `docker-compose.yml`, `bank_sim/`, integration harness, and black-box
test setup. The final Compose must not import or mount `legacy/`.

### Integrator/reviewer

Owns final integration. Runs `cargo fmt --check`, `cargo test`, Compose build/start,
all smoke/contract tests, and reviews authentication, path safety, transaction use,
and clean-checkout reproducibility.

## Handoff protocol

Every agent reports:

1. files changed;
2. commands/tests run;
3. compatibility decisions;
4. known gaps or follow-up work.

Merge in this order: contract, database/domain, HTTP/auth, reports/CLI, infrastructure,
then integrator. The integrator resolves conflicts by the compatibility matrix and
legacy fixtures, not by introducing new behavior.
