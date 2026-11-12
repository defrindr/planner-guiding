# ShopCore Migration

Migrasi service legacy ShopCore ke Rust. Implementasi membaca kontrak dan behavior
legacy, tetapi image/runtime final tidak bergantung pada folder `legacy/`.

## Status

Service Rust sekarang menyediakan API HTTP, persistence PostgreSQL, bank webhook,
maintenance commands, dan bank simulator. Detail handoff dan keputusan kompatibilitas
ada di [docs/MIGRATION_PLAN.md](docs/MIGRATION_PLAN.md).

## Struktur

```text
.
├── src/                 # application source
├── tests/               # unit, integration, dan black-box contract tests
├── data/                # local runtime data; tidak di-commit
├── .env.example         # konfigurasi lokal non-rahasia
├── MIGRATION_GUIDE.md   # checklist dan best practice migrasi
└── README.md
```

## Development commands

Command utama (dijalankan di container Rust):

```text
cargo test
docker compose build
docker compose up
```

Service production harus listen pada `0.0.0.0:8000`. Compose menyalakan PostgreSQL,
bank simulator, lalu menjalankan `shopcore init-db`, `shopcore seed`, dan
`shopcore serve`. Bank simulator tersedia di `http://localhost:5001`.

## Container smoke test

Dengan Docker dan Compose tersedia:

```sh
./scripts/test-compose.sh
```

Smoke test HTTP dapat dijalankan terhadap deployment yang sudah hidup dengan:

```sh
python3 -m pip install -r tests/requirements.txt
BASE_URL=http://localhost:8000 BANK_URL=http://localhost:5001 \
  python3 -m pytest -q tests/test_blackbox_smoke.py
```

Konfigurasi utama dapat dioverride lewat environment: `DATABASE_URL`, `BANK_URL`,
`BANK_SECRET`, `ADMIN_USER`, `ADMIN_PASSWORD`, `APP_PORT`, `BANK_PORT`, dan
`POSTGRES_*`. Runtime tidak membutuhkan network eksternal setelah image selesai
dibangun.

## Infrastructure contract

`Dockerfile` mengasumsikan package Rust menghasilkan binary bernama `shopcore` dan
mendukung subcommand `init-db`, `seed`, serta `serve`. `migrations/` harus tersedia
di root saat build dan binary harus membaca `DATABASE_URL`, `BANK_URL`, `BANK_SECRET`,
`HOST`, `PORT`, dan `EXPORT_DIR`. Ini adalah integration requirement untuk agent
core; infrastructure tidak mengubah `src/` atau migration.

## Kontribusi perubahan

1. Verifikasi kontrak legacy sebelum mengubah response atau status code.
2. Tambahkan regression/black-box test untuk setiap endpoint.
3. Jalankan formatter, linter, test, dan build sebelum commit.
4. Jangan commit secret, dependency cache, build artifact, atau data runtime.

Lihat [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) untuk checklist lengkap.

