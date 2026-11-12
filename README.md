# ShopCore Migration

Migrasi service legacy ShopCore ke stack target. Source service dan kontrak API belum tersedia di initial repository ini.

## Status

Repository saat ini berisi fondasi project dan panduan migrasi. Setelah source legacy atau spesifikasi kontrak tersedia, tambahkan implementasi di `src/`, test di `tests/`, dan konfigurasi runtime sesuai target language.

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

Command berikut adalah placeholder sampai target language ditentukan:

```text
Install: <target-language install command>
Test:   <target-language test command>
Build:  <target-language build command>
Run:    <target-language run command>
```

Service production harus listen pada `0.0.0.0` dan port dari `PORT` (default `8080`) agar kompatibel dengan clean-container grader.

## Kontribusi perubahan

1. Verifikasi kontrak legacy sebelum mengubah response atau status code.
2. Tambahkan regression/black-box test untuk setiap endpoint.
3. Jalankan formatter, linter, test, dan build sebelum commit.
4. Jangan commit secret, dependency cache, build artifact, atau data runtime.

Lihat [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) untuk checklist lengkap.

