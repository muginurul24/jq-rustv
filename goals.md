# Rewrite Goals: `justqiuv2` -> Vue + Rust

## Tujuan Dokumen

Dokumen ini adalah **source of truth** untuk target akhir rewrite project `justqiuv2`.
Semua keputusan implementasi harus tunduk ke dokumen ini.

Project ini **bukan** sekadar dashboard admin. Project ini adalah:

1. bridge antara aplikasi toko dan upstream **NexusGGR**
2. bridge antara aplikasi toko dan upstream **QRIS / VA / Disbursement**
3. dashboard operasional yang mengelola **uang milik toko**
4. sistem callback yang meneruskan event ke `callback_url` toko

Karena project ini mengelola uang, rewrite **harus memprioritaskan keamanan, integritas data, dan kompatibilitas kontrak** di atas kecepatan coding.

---

## Goal Utama

Rewrite seluruh project ke stack berikut:

1. **Frontend**: Vue 3 + Vite
2. **Styling**: Tailwind CSS v4 + shadcn-vue
3. **Form handling**: VeeValidate
4. **Vue ecosystem**: boleh memakai Vue Router, Pinia, VueUse, dan dependency Vue lain yang relevan
5. **Backend**: Rust
6. **Cache & session registry**: Redis
7. **Auth**: JWT session + CSRF token model seperti Laravel
8. **Captcha**: self-hosted captcha seperti pengalaman `mews/captcha`
9. **Database**: tetap PostgreSQL

Rewrite harus menghasilkan sistem yang:

1. mempertahankan **perilaku bisnis inti** project saat ini
2. mempertahankan **kontrak API bridge** yang sudah dipakai aplikasi toko
3. mempertahankan **kontrak upstream** terhadap:
   - `docs/nexusggr-openapi-3.1.yaml`
   - `docs/API Qris & VA V3.postman_collection.json`
4. mempertahankan **dashboard operasional** dengan permission yang setara atau lebih ketat
5. meningkatkan **keamanan, observability, maintainability, testability, dan performa**

---

## Non-Negotiable Invariants

Hal-hal berikut **wajib tetap benar** setelah rewrite:

### 1. Project tetap menjadi bridge

Frontend toko atau dashboard **tidak boleh** memanggil upstream NexusGGR atau QRIS langsung.
Semua integrasi harus melewati backend project ini.

### 2. Kontrak API publik `/api/v1` harus tetap kompatibel

Minimal route berikut harus tetap ada dan tetap melayani tujuan yang sama:

- `POST /api/v1/test`
- `POST /api/v1/user/create`
- `GET /api/v1/providers`
- `POST /api/v1/games`
- `POST /api/v1/games/v2`
- `POST /api/v1/user/deposit`
- `POST /api/v1/user/withdraw`
- `POST /api/v1/game/launch`
- `POST /api/v1/money/info`
- `POST /api/v1/game/log`
- `POST /api/v1/user/withdraw-reset`
- `POST /api/v1/transfer/status`
- `GET /api/v1/call/players`
- `POST /api/v1/call/list`
- `POST /api/v1/call/apply`
- `POST /api/v1/call/history`
- `POST /api/v1/call/cancel`
- `POST /api/v1/control/rtp`
- `POST /api/v1/control/users-rtp`
- `POST /api/v1/merchant-active`
- `POST /api/v1/generate`
- `POST /api/v1/check-status`
- `GET /api/v1/balance`

Kalau ada perubahan response shape, perubahan itu **harus disengaja, terdokumentasi, dan di-versioning**.
Target default rewrite adalah **mempertahankan response contract lama**.

### 3. Webhook upstream harus tetap ada

Route inbound dari provider harus tetap tersedia:

- `POST /api/webhook/qris`
- `POST /api/webhook/disbursement`

### 4. Callback ke toko harus tetap bekerja

Jika `callback_url` ada pada toko, maka setelah event tertentu diproses lokal, sistem wajib mengirim callback keluar ke `callback_url` toko.

Minimal event:

- deposit QRIS sukses
- status disbursement / withdrawal berubah

### 5. Rahasia upstream tidak boleh bocor

Semua data berikut **tidak boleh** bocor ke aplikasi toko atau callback toko kecuali memang bagian dari kontrak publik:

- global merchant uuid
- merchant id rahasia perusahaan
- secret/key upstream
- raw upstream error detail
- raw upstream payload yang tidak di-whitelist
- token internal session

Contoh aturan eksplisit:

- server toko **jangan pernah menerima** `global-merchant-uuid`
- user / toko **jangan pernah menerima** raw response upstream jika ada field rahasia

### 6. Uang harus konsisten

Semua operasi uang wajib mengikuti aturan berikut:

- semua nilai uang disimpan sebagai **integer** rupiah
- tidak boleh memakai float untuk penyimpanan
- transaksi harus menjadi ledger yang bisa diaudit
- perubahan saldo harus bisa ditelusuri ke transaksi yang jelas
- callback inbound harus idempotent
- retry job tidak boleh menyebabkan double credit / double refund / double debit

### 7. Ownership dan visibility harus ketat

Role saat ini:

- `dev`
- `superadmin`
- `admin`
- `user`

Aturan akses minimum:

- `dev` dan `superadmin` boleh melihat semua data
- `admin` dan `user` hanya boleh melihat toko miliknya atau toko yang berelasi dengannya
- player, bank, balance, transaction, game log, call management, dan withdrawal harus selalu di-scope ke toko yang memang accessible

### 8. Mapping player harus dipertahankan

Konsep berikut wajib tetap ada:

- `username` = username lokal yang dilihat toko
- `ext_username` = username yang dikirim ke NexusGGR upstream

Aturannya:

- `username` **boleh sama** di toko yang berbeda
- `ext_username` harus unik secara global
- semua request ke NexusGGR memakai `ext_username`
- response ke toko tetap memakai `username`

### 9. Saldo tetap memakai tiga domain utama

Balance toko tetap memiliki tiga bucket inti:

- `pending`
- `settle`
- `nexusggr`

Makna bisnis:

- `pending`: dana deposit player yang belum settle
- `settle`: dana toko yang bisa di-withdraw
- `nexusggr`: saldo agent yang dipakai untuk deposit/withdraw user di upstream NexusGGR

### 10. Dashboard harus tetap operasional

Semua area operasional penting sekarang wajib tersedia ulang dalam frontend Vue:

- auth login / register
- users
- tokos
- banks
- players
- transactions
- providers
- games
- Game Log
- Call Management
- Withdrawal
- NexusGGR Topup
- API Documentation

---

## Goal Arsitektur Target

## A. Frontend Goal

Frontend harus berupa SPA modern berbasis Vue 3 + TypeScript.

Stack yang direkomendasikan:

- Vue 3
- TypeScript
- Vite
- Vue Router
- Pinia
- VueUse
- Axios
- Tailwind CSS v4
- shadcn-vue
- VeeValidate
- Zod
- Lucide icons

Goal frontend:

1. menggantikan seluruh halaman Filament dengan UI Vue
2. tetap mempertahankan alur kerja operasional yang sama
3. memisahkan server state dan client state dengan rapi
4. menggunakan komponen UI yang konsisten
5. menghindari tampilan “admin panel generik yang membingungkan”
6. aman terhadap XSS, CSRF, dan session confusion

## B. Backend Goal

Backend harus ditulis ulang di Rust.

Stack backend yang direkomendasikan:

- Axum untuk HTTP server
- Tokio untuk async runtime
- SQLx untuk query PostgreSQL
- Redis client untuk cache, session registry, rate limiting, captcha store
- Serde untuk JSON serialization
- JSON Web Token library untuk JWT
- Tower / tower-http untuk middleware
- Tracing untuk logging dan observability
- Reqwest untuk outbound HTTP ke upstream / callback toko

Goal backend:

1. menggantikan seluruh controller Laravel dengan service Rust yang typed
2. menjaga seluruh kontrak request / response tetap stabil
3. memisahkan layer:
   - HTTP layer
   - auth/session layer
   - domain layer
   - repository layer
   - upstream client layer
   - worker / async job layer
4. semua endpoint finansial harus transactional dan idempotent
5. semua ownership check harus berada di server, bukan hanya UI

## C. Session & Auth Goal

Auth target bukan token liar di localStorage.
Gunakan **JWT session via cookie** yang tetap memiliki kontrol server-side melalui Redis.

Goal auth:

1. session login dashboard menggunakan cookie HttpOnly
2. JWT memuat klaim minimum:
   - `sub`
   - `role`
   - `sid`
   - `exp`
   - `iat`
3. Redis menyimpan session registry by `sid`
4. logout harus bisa invalidasi session sebelum JWT expired
5. semua request state-changing wajib melewati verifikasi CSRF

## D. CSRF Goal

Implementasi harus terasa seperti Laravel:

- server mengirim cookie `XSRF-TOKEN`
- frontend membaca cookie ini dan mengirim `X-XSRF-TOKEN`
- request mutating tanpa header valid harus ditolak

CSRF wajib dipakai minimal untuk:

- login
- register
- logout
- semua endpoint dashboard internal
- semua aksi finansial admin / user

## E. Captcha Goal

Captcha harus self-hosted dan sederhana dipakai.

Goal captcha:

1. ada endpoint generate captcha
2. ada id / token captcha
3. jawaban disimpan hashed di Redis dengan TTL
4. bisa refresh captcha tanpa reload penuh
5. dipakai minimal pada:
   - login
   - register
   - password reset jika ada
   - endpoint sensitif yang rawan abuse bila diperlukan

## F. Redis Goal

Redis bukan hanya cache biasa.
Redis wajib dipakai untuk:

1. session registry JWT
2. CSRF secret atau session-linked csrf state
3. captcha challenge state
4. rate limiting
5. queue / job coordination bila worker memakainya
6. provider list cache
7. game list cache
8. idempotency / dedupe keys untuk webhook dan callback

## G. Security Goal

Sistem rewrite harus lebih aman dari versi sekarang.

Wajib:

1. semua request ownership diverifikasi di backend
2. upstream secrets tidak pernah dikirim ke client
3. callback toko di-whitelist payload-nya
4. webhook upstream tervalidasi
5. semua aksi uang memiliki audit trail
6. rate limit diterapkan untuk auth, webhook, dan API publik
7. logging sensitif harus di-redact
8. tidak ada secret di frontend build output

## H. Observability Goal

Minimal harus ada:

1. structured logs
2. correlation / request id
3. job logs
4. webhook delivery logs
5. callback delivery logs
6. upstream latency metrics
7. audit log untuk transaksi finansial penting

---

## Business Behavior That Must Be Preserved

## 1. Toko dan callback

Setiap toko memiliki:

- owner user
- token API
- `callback_url`
- balance

Jika event selesai diproses secara lokal dan `callback_url` valid, sistem harus push callback ke toko.

## 2. Deposit QRIS reguler

Perilaku saat ini yang harus dipertahankan:

1. toko generate QRIS via bridge
2. bridge membuat transaksi `qris + deposit + pending`
3. upstream mengirim callback `qris`
4. bridge update transaksi menjadi `success`
5. bridge menambah `pending` toko setelah dikurangi fee transaction
6. fee transaction masuk ke income platform
7. callback diteruskan ke `callback_url` toko

## 3. NexusGGR topup via QRIS

Perilaku saat ini yang harus dipertahankan:

1. dashboard generate QRIS khusus topup NexusGGR
2. transaksi tetap `qris + deposit + pending` dengan note purpose khusus
3. saat callback sukses, dana tidak masuk `pending`
4. dana dikonversi menjadi `nexusggr`
5. conversion ratio mengikuti `income.ggr`

## 4. Withdrawal settle balance

Perilaku saat ini yang harus dipertahankan:

1. user memilih toko
2. user memilih rekening tujuan
3. sistem inquiry rekening ke upstream QRIS/disbursement
4. sistem menghitung:
   - amount
   - bank fee
   - platform fee
   - total deduction
5. jika submit sukses:
   - transaksi withdrawal dibuat pending
   - `settle` dipotong total deduction
6. saat callback disbursement:
   - jika success: platform fee masuk income
   - jika gagal: settle dikembalikan
7. callback status withdrawal diteruskan ke toko

## 5. NexusGGR deposit / withdraw player

Perilaku saat ini yang harus dipertahankan:

1. deposit player memotong `nexusggr` toko
2. withdraw player menambah `nexusggr` toko
3. semua aksi membuat ledger transaction category `nexusggr`
4. response ke toko tetap memakai `username` lokal
5. upstream tetap memakai `ext_username`

## 6. Call management dan game log

Perilaku saat ini yang harus dipertahankan:

1. data call dan game log hanya terlihat untuk player yang accessible
2. admin/user tidak boleh melihat player toko lain
3. display memakai username lokal
4. request upstream tetap memakai ext identity

---

## Compatibility Goals

## 1. API Compatibility

Goal default:

- path tetap sama
- method tetap sama
- request body field tetap sama
- response field publik tetap sama
- error semantik utama tetap sama

Jika ada perubahan yang diperlukan, maka:

1. harus dibuat backward-compatible lebih dulu
2. perubahan harus didokumentasikan
3. perubahan harus memiliki migration plan

## 2. Database Compatibility

Goal default:

- tetap memakai PostgreSQL
- sedapat mungkin mempertahankan tabel inti:
  - `users`
  - `tokos`
  - `banks`
  - `balances`
  - `players`
  - `transactions`
  - `incomes`
- schema boleh dirapikan atau ditambah, tetapi data lama harus bisa dimigrasi lossless

## 3. Upstream Compatibility

NexusGGR client baru harus tetap kompatibel dengan `docs/nexusggr-openapi-3.1.yaml`.
QRIS/VA client baru harus tetap kompatibel dengan `docs/API Qris & VA V3.postman_collection.json`.

---

## Suggested Target Topology

## Frontend

- Vue SPA untuk dashboard backoffice
- route base tetap bisa memakai `/backoffice` agar transisi tidak memecah kebiasaan user

## Backend

- Rust HTTP API
- Rust worker untuk job async

## Infra

- PostgreSQL
- Redis

Frontend dan backend boleh berada dalam satu repository monorepo.

---

## Quality Goals

Rewrite dianggap berhasil jika:

1. semua route bridge yang dipakai toko tetap bisa dipanggil
2. semua flow uang memiliki automated tests
3. semua webhook inbound idempotent
4. semua callback outbound retryable dan observable
5. dashboard memiliki UX setara atau lebih baik dari versi sekarang
6. tidak ada direct dependency ke Laravel, Livewire, atau Filament lagi
7. tidak ada direct call dari browser ke upstream provider

---

## Definition of Done

Rewrite belum selesai sebelum semua poin ini terpenuhi:

1. login dashboard berjalan dengan JWT cookie + CSRF
2. token API toko berjalan untuk `/api/v1`
3. semua route `/api/v1` parity dengan project lama
4. semua webhook inbound parity dengan project lama
5. semua callback toko parity dengan project lama
6. semua balance mutation punya test
7. semua transaction ledger mutation punya test
8. role-based visibility parity
9. player local/ext mapping parity
10. secret upstream tidak bocor ke toko atau UI
11. API docs baru tersedia dan akurat
12. cutover dari Laravel ke Rust bisa dilakukan tanpa kehilangan data

---

## Explicit Anti-Goals

Hal-hal berikut **tidak boleh** dilakukan saat rewrite:

1. jangan ubah project menjadi client yang memanggil upstream langsung dari frontend
2. jangan simpan access token di `localStorage`
3. jangan expose raw upstream response ke toko
4. jangan hilangkan ledger transaksi demi “kesederhanaan”
5. jangan hilangkan callback toko
6. jangan mengubah `username` lokal menjadi satu-satunya identifier upstream
7. jangan pindahkan semua logika uang ke frontend
8. jangan gunakan float untuk arithmetic uang
9. jangan memaksa user toko melihat data toko lain

---

## Prinsip Implementasi

Jika ada keraguan saat rewrite:

1. utamakan keamanan uang
2. utamakan kompatibilitas kontrak
3. utamakan idempotency
4. utamakan auditability
5. utamakan server-side ownership checks
6. jangan mengembalikan raw upstream payload

