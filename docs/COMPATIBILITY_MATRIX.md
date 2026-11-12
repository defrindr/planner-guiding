# ShopCore Compatibility Matrix

This matrix is the integration checklist. The OpenAPI file and legacy smoke tests are
the source of expected public behavior; agents must update the fixture/test reference
when they discover a behavior that is not obvious from the schema.

| Area | Public surface | Auth | Main compatibility requirements |
|---|---|---|---|
| Liveness | `GET /health` | none | `200`, `{"status":"ok"}` |
| Auth | register/login/logout | logout bearer | `201` registration, `401` invalid credentials, in-memory sessions |
| Profile | `GET/PATCH /api/users/me` | bearer | public user fields and update behavior |
| Products | `GET/POST /api/products`, `GET/PUT /api/products/{id}` | admin for writes | search, ascending sort, page/limit, `404`, admin `403` |
| Orders | `POST/GET /api/orders`, `GET /api/orders/{id}` | bearer | stock reservation, coupons, newest-first pagination, item shape |
| Invoice | `GET /api/orders/{id}/invoice` | bearer | customer, lines, line totals, status |
| Payment | `POST /api/orders/{id}/pay` | bearer | wallet `402`, bank retries, bank ref, pending-only `409` |
| Lifecycle | cancel/refund endpoints | bearer | restock, wallet credit, status transitions, repeated-action `409` |
| Wallet | balance/topup/transfer | bearer | balances, recipient `404`, insufficient funds `402` |
| Reports | sales/export/download | admin | date bounds, day/status/user buckets, top products, gzip CSV |
| Admin | users/role/order status | admin | missing/invalid bearer `401`, non-admin `403` |
| Webhook | `POST /api/webhooks/bank` | HMAC only | raw-body signature, settlement, invalid signature `401` |
| Commands | init-db/seed/import/export/purge | process | equivalent effects and clean exit codes |
| Runtime | Compose + bank simulator | service | Postgres dependency, port 8000, bank port 5001 |

## Required edge cases

- malformed or empty JSON;
- missing required fields;
- duplicate username/email;
- unknown product/order/user;
- empty items and insufficient stock;
- invalid/expired/already-used coupons;
- insufficient wallet balance;
- paying a non-pending order;
- repeated cancellation/refund/webhook;
- invalid webhook signature;
- missing export file;
- clean startup after database initialization.
