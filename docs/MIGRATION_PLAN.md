# ShopCore Rust Migration Plan

## Source inventory

`legacy/` is the behavioral source of truth. It contains the Flask/PostgreSQL API,
OpenAPI 3 contract, smoke tests, `manage.py`, seed data, Docker Compose, and bank
simulator. The final service must not depend on `legacy/` at build or runtime because
the graded checkout contains the migrated service and its own runtime assets.

## Target architecture

- Rust stable binary using Axum/Tokio for HTTP.
- SQLx with PostgreSQL and embedded SQL migrations.
- Explicit modules for config, HTTP, auth, domain, repositories, jobs, reports, and CLI.
- Preserve the legacy PostgreSQL tables and externally visible JSON/status behavior.
- Use `f64` for money at the HTTP boundary to preserve legacy JSON/rounding behavior.
- Use transactions for order creation, wallet transfers, payment, cancellation, refund,
  and stock changes.

## Required behavior

Implement every route in `legacy/docs/openapi.yaml`: health, registration/login/logout,
profile, products, orders/invoice/payment/cancel/refund, wallet, sales reports/export/
download, admin users/roles/order status, and signed bank settlement webhook.

Also implement equivalent maintenance commands:

```text
shopcore init-db
shopcore seed
shopcore import-products <csv>
shopcore export-sales <file>
shopcore purge-sessions
shopcore serve
```

Preserve session lifetime, pagination defaults, coupon rules, order statuses, report
formats, gzip CSV output, bank retry behavior, HMAC-SHA256 webhook signatures, and
asynchronous notify callbacks. The bank simulator must be included in the final
checkout so Compose does not depend on `legacy/`.

The documented OpenAPI auth contract takes precedence for missing credentials: return
401 for missing/invalid bearer tokens and 403 for authenticated non-admin users. This
corrects the legacy anonymous admin-route bypass and must be covered by regression tests.

## Delivery order

1. Extract the route/status/body/auth matrix and legacy fixtures.
2. Add Rust project, config, schema migrations, seed, and repository layer.
3. Implement auth, users, products, orders, wallet, admin, webhook, reports, jobs, and CLI.
4. Add bank simulator, Dockerfile, Compose, and clean-container test harness.
5. Compare Rust responses with legacy fixtures, fix compatibility gaps, and run final
   build/start/smoke verification.

## Acceptance

- `docker compose build` works from a clean checkout.
- Compose starts PostgreSQL, bank simulator, and Rust service without manual steps.
- Rust listens on `0.0.0.0:8000` and supports the full OpenAPI contract.
- `cargo test` passes inside the build/test container.
- Legacy smoke scenarios pass against the Rust service.
- README documents build, run, configuration, commands, schema changes, and deliberate
  behavior changes.
