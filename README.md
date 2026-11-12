# ShopCore Migration

Migrasi service legacy ShopCore (Flask/PostgreSQL) ke Rust. Implementasi membaca
kontrak dan behavior legacy, tetapi image/runtime final tidak bergantung pada folder
`legacy/`.

## Status

Service Rust menyediakan API HTTP, persistence PostgreSQL, bank settlement webhook,
maintenance command, dan bank simulator. Kontrak publik mengikuti
`legacy/docs/openapi.yaml`; detail keputusan migrasi ada di
[docs/MIGRATION_PLAN.md](docs/MIGRATION_PLAN.md).

## Quick start (clean checkout)

Prasyarat: Docker dan Docker Compose. Tidak ada langkah manual database.

```sh
docker compose up --build -d
curl http://localhost:8000/health
```

Compose menyalakan PostgreSQL, bank simulator, lalu menjalankan `shopcore init-db`,
`shopcore seed`, dan `shopcore serve`. Default port: app `8000`, bank `5001`.

```sh
# Stop service tanpa menghapus data
docker compose down

# Stop dan hapus volume database
docker compose down -v
```

Override port host bila diperlukan:

```sh
APP_PORT=8080 BANK_PORT=5002 docker compose up --build -d
```

## Build dan test lokal

```sh
cargo build --locked
cargo test --locked
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Rust test tidak membutuhkan database. Satu command untuk verifikasi penuh
(build image, start stack, black-box HTTP, dan maintenance command):

```sh
./scripts/test-compose.sh
```

Black-box test terhadap stack yang sudah hidup:

```sh
python3 -m pip install -r tests/requirements.txt
BASE_URL=http://localhost:8000 BANK_URL=http://localhost:5001 \
  python3 -m pytest -q tests/test_blackbox_smoke.py
```

## Maintenance commands

Command setara dengan `legacy/manage.py`:

| `manage.py` | Rust |
|---|---|
| `manage.py init-db` | `shopcore init-db` |
| `manage.py seed` | `shopcore seed` |
| `manage.py import-products <csv>` | `shopcore import-products <csv>` |
| `manage.py export-sales <file>` | `shopcore export-sales <file>` |
| `manage.py purge-sessions` | `shopcore purge-sessions` |
| - | `shopcore serve` |

Jalankan pada stack yang hidup:

```sh
docker compose exec -T app shopcore init-db
docker compose exec -T app shopcore seed
docker compose exec -T app shopcore purge-sessions

# File harus tersedia di dalam container.
docker compose cp seed/products.csv app:/tmp/products.csv
docker compose exec -T app shopcore import-products /tmp/products.csv

docker compose exec -T app shopcore export-sales /tmp/exports/sales.csv
docker compose cp app:/tmp/exports/sales.csv ./sales.csv
```

Tanpa database, `purge-sessions` hanya membersihkan session store proses command
tersebut; session server yang sedang berjalan tetap hidup. Ini sama dengan perilaku
`legacy/manage.py` yang juga berjalan sebagai proses terpisah.

## Konfigurasi

| Variable | Default compose | Keterangan |
|---|---|---|
| `DATABASE_URL` | `postgresql://shop:shop@db:5432/shop` | Koneksi PostgreSQL |
| `BANK_URL` | `http://bank:5001` | URL bank simulator |
| `BANK_SECRET` | `bank-shared-secret-2019` | Secret HMAC webhook |
| `ADMIN_USER` / `ADMIN_PASSWORD` | `admin` / `admin123` | Akun admin saat seed |
| `HOST` / `PORT` | `0.0.0.0` / `8000` | Bind server |
| `BIND` | - | Override `HOST:PORT` langsung |
| `EXPORT_DIR` | `/tmp/exports` | Direktori export/download |
| `APP_PORT` / `BANK_PORT` | `8000` / `5001` | Port host Compose |
| `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` | `shop` / `shop` / `shop` | Database Compose |
| `RUST_LOG` | `info` | Level logging |

Runtime tidak membutuhkan network eksternal setelah image selesai dibangun.

## Schema

Schema ada di `migrations/0001_init.sql` dan di-embed ke binary melalui
`sqlx::migrate!`. `shopcore init-db` menjalankan migrasi; `shopcore seed` menambahkan
admin, coupon, dan produk awal. Compose menjalankan keduanya otomatis sebelum server
start. Tidak ada langkah migrasi manual.

## Perubahan perilaku yang disengaja

- Error untuk route tidak dikenal, method salah, dan path ID tidak valid selalu JSON
  `{"error":"..."}`. Legacy bisa mengembalikan body kosong atau plain text.
- Admin route mengembalikan `401` untuk bearer hilang/invalid, sesuai OpenAPI. Legacy
  memiliki anonymous bypass pada `admin_required`.
- Webhook tanpa `X-Signature` ditolak `401`. Legacy hanya memeriksa signature bila
  header tersebut ada.
- `sort` produk memakai whitelist `id`, `name`, `price`, `stock`, `sku`; `sort=sku`
  didukung. Legacy menyisipkan nilai `sort` langsung ke SQL.
- Order creation, wallet payment/transfer, dan cancellation memakai transaksi database.
  Legacy tidak selalu atomik. Refund tetap mengikuti perilaku legacy non-transactional.
- `import-products` idempotent per SKU antar-run. Legacy hanya mendeduplikasi di dalam
  file yang sama.
- Export/download memakai basename file untuk mencegah path traversal.
- Webhook settlement yang memindahkan order `pending` ke `paid` juga mengisi
  `paid_amount=total`.
- Tanggal report invalid mengembalikan `400 {"error":"invalid date"}`.
- Setting `SECRET_KEY` yang tidak dipakai dihapus dari konfigurasi.

## Struktur

```text
.
├── src/                 # service Rust
├── migrations/          # schema PostgreSQL
├── bank_sim/            # bank simulator
├── tests/               # black-box HTTP suite
├── scripts/             # container test harness
├── Dockerfile           # build Rust + runtime image
├── docker-compose.yml   # app + PostgreSQL + bank
├── Cargo.toml
├── Cargo.lock
└── README.md
```

## Kontribusi perubahan

1. Verifikasi kontrak legacy/OpenAPI sebelum mengubah response atau status code.
2. Tambahkan regression/black-box test untuk setiap endpoint atau behavior.
3. Jalankan formatter, linter, test, dan build sebelum commit.
4. Jangan commit secret, dependency cache, build artifact, atau data runtime.

Lihat [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) untuk checklist lengkap.
