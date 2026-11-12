# ShopCore Migration — Best-Practice Guide

Panduan ini dipakai untuk migrasi service legacy ke stack target ketika grader menguji perilaku service sebagai black box.

## 1. Tetapkan kontrak sebelum menulis ulang

- Inventaris semua endpoint, method HTTP, path, query, header, body, dan status code.
- Catat format JSON sukses dan error secara persis: nama field, tipe data, `null` versus field yang hilang, serta format tanggal dan angka.
- Catat validasi, default value, pagination, sorting, filtering, dan perilaku resource yang tidak ditemukan.
- Identifikasi side effect, idempotency, urutan operasi, dan apakah data bertahan antar-request.
- Simpan contoh request/response legacy sebagai fixture.

## 2. Prioritaskan kompatibilitas black-box

Pertahankan URL/method, status code, header relevan, bentuk response body, aturan urutan hasil, state antar-request, serta perilaku input invalid, missing, kosong, dan duplikat. Hindari memperbaiki perilaku legacy yang aneh bila grader mungkin bergantung padanya.

## 3. Struktur implementasi

Pisahkan lapisan berikut:

- **Transport**: routing, parsing request, status code, dan serialisasi response.
- **Application/service**: use case dan aturan bisnis.
- **Repository**: akses persistence atau adapter storage.
- **Model/schema**: validasi dan pemetaan data.
- **Configuration**: environment variable, port, dan lokasi storage.

Jangan menaruh aturan bisnis di handler HTTP. Gunakan dependency injection sederhana agar storage mudah diganti.

## 4. Persistence dan determinisme

- Data harus bertahan selama proses hidup jika legacy bersifat stateful.
- Gunakan seed deterministik; jangan membuat data awal secara acak.
- Gunakan ID dan timestamp yang konsisten dengan kontrak legacy.
- Gunakan timezone eksplisit, bukan timezone lokal mesin grader.
- Buat direktori storage saat startup dan tangani file yang belum ada.
- Sort hasil secara eksplisit bila kontrak membutuhkan urutan tertentu.

## 5. Error handling

Buat format error yang sama dengan legacy, misalnya:

```json
{"error":"descriptive message"}
```

Tangani terpisah JSON malformed, field wajib hilang, tipe data salah, resource tidak ditemukan, konflik/duplikasi, route tidak didukung, dan error internal. Jangan membocorkan stack trace atau path filesystem.

## 6. Testing

Buat test unit untuk validasi dan aturan bisnis, integration test untuk repository/persistence, serta contract test yang menjalankan service lalu mengirim request HTTP nyata.

Untuk setiap endpoint, uji happy path, body kosong, JSON malformed, field wajib hilang, tipe salah, ID valid/tidak ada/salah format, list kosong, list banyak item, request berulang, state, dan idempotency.

Jika service legacy masih bisa dijalankan, buat harness pembanding: kirim fixture yang sama ke legacy dan implementasi baru, lalu bandingkan status, header relevan, dan JSON yang dinormalisasi hanya untuk bagian nondeterministik.

## 7. Runtime dan container

- Sediakan satu command startup dan dokumentasikan port.
- Bind server ke `0.0.0.0`, bukan hanya `127.0.0.1`.
- Ambil port dari environment variable bila platform grader menyediakannya.
- Proses utama harus menerima signal dan berhenti dengan bersih.
- Jangan membutuhkan service eksternal yang tidak tersedia di clean container.
- Lock dependency dan gunakan build repeatable.
- Gunakan `.dockerignore` untuk cache, secret, build output, dan dependency lokal.

## 8. Keamanan dan kualitas minimum

- Validasi ukuran body dan input sebelum diproses.
- Jangan commit secret, credential, atau data produksi.
- Hindari shell command dari input user.
- Gunakan logging tanpa mencetak token atau data sensitif.
- Jalankan formatter, linter, test, dan build dalam kondisi clean.

## 9. Checklist submission

- [ ] Repository berisi source, manifest dependency, dan konfigurasi runtime.
- [ ] README menjelaskan command install, test, build, dan start.
- [ ] Service dapat dibangun dari direktori kosong di clean container.
- [ ] Server listen pada host/port yang benar.
- [ ] Semua route dan status code sudah dicocokkan.
- [ ] Error response dan edge case sudah diuji.
- [ ] Tidak ada dependency pada file/service lokal yang tidak ikut di-commit.
- [ ] `git status` bersih dan commit lengkap.
- [ ] URL repository dan hash commit yang disubmit dapat diakses grader.

## README minimum

```text
Install: <command>
Test:   <command>
Build:  <command>
Run:    <command>
Port:   <default port / environment variable>
```

Tambahkan daftar endpoint dan contoh request hanya setelah kontraknya terverifikasi dari legacy atau spesifikasi resmi.
