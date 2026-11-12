# Clean `src` Layout Design

## Intent

Make the Rust source tree communicate ownership clearly without changing the
ShopCore HTTP, CLI, database, or webhook contract.

## Chosen approach

Use namespace-oriented modules:

- `main.rs` only loads configuration, initializes logging, parses CLI, and
  delegates to `cli::run`.
- `cli.rs` owns command definitions and maintenance command orchestration.
- `application/` owns application context, DTOs, and the current use-case
  service implementation.
- `domain/` owns pure business helpers and domain value behavior.
- `http/` owns transport models, error responses, and routes.
- `repository/` owns database access and seed data.
- `db.rs` and `config.rs` remain top-level infrastructure modules because they
  are shared by CLI and HTTP startup.

The unused `core.rs` compatibility facade is removed because this is a binary
crate and no compiled caller imports it. Existing behavior remains protected
by Rust unit tests and the HTTP black-box suite.

## Non-goals

This pass does not redesign the SQL schema, alter endpoint behavior, or split
the large application dispatcher into artificial wrappers. That deeper split
is a separate change once operation boundaries have dedicated tests.
