# Rewrite Task Plan: `justqiuv2` -> Vue + Rust

## Cara Menggunakan Dokumen Ini

Dokumen ini ditulis agar model AI yang lebih murah tetap bisa bekerja dengan benar.
Ikuti urutannya.
Jangan improvisasi besar tanpa alasan yang tertulis di sini.

Urutan kerja yang benar:

1. pahami `goals.md`
2. baca source of truth teknis yang disebut di bawah
3. implementasikan per phase
4. jangan lompat ke UI final sebelum kontrak backend stabil
5. jangan ubah perilaku bisnis tanpa update dokumentasi

---

## Source of Truth yang Wajib Dibaca Sebelum Implementasi

### Kontrak API upstream

- `docs/nexusggr-openapi-3.1.yaml`
- `docs/API Qris & VA V3.postman_collection.json`

### Kontrak API bridge saat ini

- `routes/api.php`
- `app/Http/Controllers/Api/WebhookTokoController.php`
- `app/Http/Controllers/Api/WebhookController.php`

### Flow uang

- `app/Filament/Pages/Withdrawal.php`
- `app/Filament/Pages/NexusggrTopup.php`
- `app/Jobs/ProcessQrisCallback.php`
- `app/Jobs/ProcessDisbursementCallback.php`
- `app/Jobs/SendTokoCallback.php`

### Model domain

- `app/Models/User.php`
- `app/Models/Toko.php`
- `app/Models/Bank.php`
- `app/Models/Balance.php`
- `app/Models/Player.php`
- `app/Models/Transaction.php`
- `app/Models/Income.php`

### Dashboard behavior

- `app/Filament/Pages/GameLog.php`
- `app/Filament/Pages/CallManagement.php`
- `app/Filament/Resources/*`
- `app/Filament/Pages/ApiDocumentation.php`

---

## Hard Rules untuk AI Implementer

1. Jangan lakukan direct HTTP dari frontend ke upstream.
2. Jangan kirim raw upstream payload ke toko.
3. Jangan bocorkan:
   - merchant uuid
   - merchant id rahasia
   - upstream secret
   - raw upstream error
4. Semua aksi uang wajib lewat transaction database.
5. Semua callback inbound wajib idempotent.
6. Semua callback outbound ke toko wajib retryable.
7. Semua route mutating wajib CSRF-protected untuk dashboard session.
8. Semua route mutating yang mewakili data user/toko wajib ownership-checked di backend.
9. Semua money value disimpan sebagai integer.
10. Semua write penting wajib menghasilkan transaction log atau audit trail.

---

## Rekomendasi Struktur Target Repository

Gunakan struktur seperti ini:

```text
apps/
  web/
    src/
      app/
      components/
      features/
      layouts/
      lib/
      pages/
      router/
      stores/
      styles/
    index.html
    package.json
    vite.config.ts

  api/
    src/
      main.rs
      app.rs
      config/
      http/
      middleware/
      auth/
      modules/
      services/
      repositories/
      workers/
      telemetry/
    Cargo.toml

  worker/
    src/
      main.rs
    Cargo.toml

crates/
  contracts/
  database/
  domain/
  auth/
  redis/
  nexusggr_client/
  qris_client/
  callback_client/
  errors/
  observability/

docs/
  ...
```

Jika tidak memakai workspace sebesar itu, minimal tetap pisahkan:

- frontend app
- backend API
- background worker
- shared contracts/types

---

## Target Frontend Stack

Gunakan:

- Vue 3
- TypeScript
- Vite
- Vue Router
- Pinia
- VueUse
- Axios
- VeeValidate
- Zod
- Tailwind CSS v4
- shadcn-vue
- lucide-vue-next

### Frontend Principles

1. gunakan route-based feature modules
2. form memakai VeeValidate + Zod schema
3. komponen UI dari shadcn-vue, bukan custom primitive berlebihan
4. state auth dan session di store terpisah
5. server state jangan dicampur dengan form state
6. seluruh request yang mutating harus otomatis mengirim `X-XSRF-TOKEN`
7. semua page harus menampilkan loading, empty state, error state

---

## Target Backend Stack

Gunakan:

- Axum
- Tokio
- SQLx
- Redis
- Reqwest
- Serde
- Jsonwebtoken
- Tracing
- Tower / tower-http

### Backend Principles

1. typed request / response DTO
2. service layer terpisah dari HTTP handlers
3. repository layer jelas untuk database access
4. upstream clients terpisah untuk NexusGGR dan QRIS
5. semua error dipetakan ke error publik yang aman
6. semua job async harus idempotent
7. semua write finansial transactional

---

## Data Model yang Harus Dipertahankan

Pertahankan konsep tabel/domain berikut:

### users

Field minimum:

- id
- username
- name
- email
- password_hash
- role
- is_active
- created_at
- updated_at

### tokos

Field minimum:

- id
- user_id
- name
- callback_url
- token
- is_active
- created_at
- updated_at
- deleted_at

### balances

Field minimum:

- id
- toko_id
- pending
- settle
- nexusggr
- created_at
- updated_at

### banks

Field minimum:

- id
- user_id
- bank_code
- bank_name
- account_number
- account_name
- created_at
- updated_at
- deleted_at

### players

Field minimum:

- id
- toko_id
- username
- ext_username
- created_at
- updated_at
- deleted_at

### transactions

Field minimum:

- id
- toko_id
- player
- external_player
- category
- type
- status
- amount
- code
- note
- created_at
- updated_at
- deleted_at

### incomes

Field minimum:

- id
- ggr
- fee_transaction
- fee_withdrawal
- amount
- created_at
- updated_at

---

## Money Rules yang Wajib Diikuti

### 1. Integer only

Semua perhitungan uang dilakukan dalam integer rupiah.

### 2. Ledger first

Mutasi saldo tanpa transaction record tidak boleh ada.

### 3. Safe status transitions

Status transaction minimal:

- `pending`
- `success`
- `failed`
- `expired`

Status hanya boleh berpindah dengan transisi yang valid.

### 4. Idempotency

Webhook inbound harus aman jika payload yang sama dikirim ulang.

Idempotency key minimum:

- QRIS callback: `trx_id`
- disbursement callback: `partner_ref_no`

### 5. Withdrawal safety

Saat withdraw settle:

1. inquiry tujuan rekening terlebih dahulu
2. hitung `amount + bank_fee + platform_fee`
3. cek settle cukup
4. potong settle saat request transfer berhasil diproses
5. jika callback gagal, refund settle
6. jika callback sukses, fee platform masuk income

### 6. NexusGGR agent balance

- deposit user mengurangi `nexusggr`
- withdraw user menambah `nexusggr`
- semua aksi harus mencatat transaction category `nexusggr`

---

## Redis Design

Gunakan Redis key schema yang jelas.

### Session

- `session:{sid}` -> JSON session metadata
- TTL sesuai session expiry

Metadata minimum:

- user_id
- role
- csrf_secret
- issued_at
- expires_at
- ip_hash
- ua_hash

### CSRF

Jika CSRF secret tidak digabung ke session blob:

- `csrf:{sid}` -> token secret / hash

### Captcha

- `captcha:{captcha_id}` -> hashed answer + metadata

TTL disarankan:

- 5 menit

### Rate limit

- `rl:login:{ip}`
- `rl:api:{token_or_toko_id}:{route}`
- `rl:webhook:{source}:{route}`

### Cache

- `cache:nexusggr:provider-list`
- `cache:nexusggr:game-list:{provider_code}`

TTL:

- provider list: 1 hari
- game list: 1 hari

### Idempotency

- `idempotency:webhook:qris:{trx_id}`
- `idempotency:webhook:disbursement:{partner_ref_no}`
- `idempotency:callback:{event_type}:{reference}`

---

## Auth, Session, dan CSRF Tasks

## Phase A1. Dashboard Session Auth

Implement:

1. login endpoint untuk dashboard
2. logout endpoint
3. current-user endpoint
4. password hashing dengan argon2id
5. role loading pada session

Output:

- cookie HttpOnly untuk session JWT
- cookie non-HttpOnly `XSRF-TOKEN`
- frontend dapat restore session pada page refresh

Acceptance criteria:

- login sukses membuat session Redis
- logout menghapus session Redis
- JWT tanpa session Redis dianggap invalid

## Phase A2. CSRF like Laravel

Implement:

1. endpoint untuk bootstrap CSRF cookie jika perlu
2. cookie `XSRF-TOKEN`
3. validasi header `X-XSRF-TOKEN`
4. rotate token saat login/logout

Acceptance criteria:

- request POST dashboard tanpa header CSRF gagal
- request dengan CSRF valid sukses
- axios frontend otomatis mengirim header

## Phase A3. Captcha

Implement:

1. endpoint generate captcha
2. endpoint refresh captcha
3. verifikasi captcha di login/register
4. simpan jawaban hashed di Redis

Captcha format:

- lebih baik SVG atau PNG
- response harus ringan untuk frontend

Acceptance criteria:

- captcha expired tidak dapat dipakai lagi
- captcha salah gagal
- captcha bisa direfresh tanpa reload page penuh

---

## Token API Toko Tasks

## Phase B1. Toko API Authentication

Implement authentication terpisah untuk `/api/v1`.

Jangan campur dengan dashboard session.

Pilihan implementasi:

- bearer token statik toko seperti versi sekarang
- atau hashed token storage yang compatible

Rekomendasi:

- simpan token toko dalam bentuk hash
- saat request `/api/v1`, lakukan lookup token hash

Acceptance criteria:

- semua `/api/v1/*` hanya bisa diakses token toko valid
- model auth result harus mewakili `toko`, bukan `user`
- ownership otomatis terscope ke toko yang terautentikasi

---

## Backend Module Breakdown

Implement module berikut.

## Module 1. Auth Module

Tanggung jawab:

- login dashboard
- register dashboard
- logout
- session validation
- csrf issuance
- captcha validation

## Module 2. User Module

Tanggung jawab:

- CRUD users dashboard
- role enforcement
- relation ke toko dan bank

## Module 3. Toko Module

Tanggung jawab:

- CRUD toko
- token toko
- callback_url
- active state
- toko visibility

## Module 4. Balance Module

Tanggung jawab:

- read balances
- safe mutate pending/settle/nexusggr
- helper invariants

Semua mutasi saldo harus melalui service ini.

## Module 5. Player Module

Tanggung jawab:

- create local player
- map username <-> ext_username
- visibility by toko
- lookup by local username
- lookup by ext_username

## Module 6. Transaction Module

Tanggung jawab:

- create ledger row
- status transition
- audit note serialization
- filtered listing
- detail view payload sanitization

## Module 7. Income Module

Tanggung jawab:

- load fee config
- update platform income
- support fee_transaction
- support fee_withdrawal
- support GGR conversion ratio

## Module 8. NexusGGR Client Module

Tanggung jawab:

- typed client to upstream NexusGGR
- request builder sesuai OpenAPI
- sanitize response
- map upstream errors ke internal error enum

Important:

- satu metode upstream = satu fungsi typed
- jangan langsung expose raw JSON ke handler publik

## Module 9. QRIS Client Module

Tanggung jawab:

- merchant active
- generate QRIS
- check status
- balance
- inquiry rekening
- submit transfer / disbursement

## Module 10. Webhook Inbound Module

Tanggung jawab:

- menerima callback qris
- menerima callback disbursement
- verifikasi source minimum
- idempotency
- enqueue processing

## Module 11. Callback Outbound Module

Tanggung jawab:

- push sanitized payload ke `callback_url` toko
- retry
- logging
- dedupe / idempotency

## Module 12. Dashboard Query Module

Tanggung jawab:

- provider list
- game list
- game log
- call players
- call history
- resource listing dashboard

---

## Public API Route Mapping Tasks

Implement route baru di Rust dengan parity terhadap route lama.

## NexusGGR bridge

### `POST /api/v1/user/create`

Behavior:

1. auth toko
2. normalisasi `username` lokal
3. generate `ext_username` unik
4. call upstream create user
5. simpan `players`
6. response ke toko hanya `username`

### `GET /api/v1/providers`

Behavior:

1. baca cache Redis
2. jika tidak ada, call upstream
3. sanitize response
4. cache 1 hari

### `POST /api/v1/games`

Behavior:

1. baca cache per provider
2. call upstream jika miss
3. sanitize response
4. cache 1 hari

### `POST /api/v1/games/v2`

Behavior:

1. call upstream localized game list
2. sanitize response
3. tidak wajib cache kalau bisnis tidak memerlukannya

### `POST /api/v1/game/launch`

Behavior:

1. resolve player lokal -> ext_username
2. call upstream
3. response publik hanya `launch_url`

### `POST /api/v1/money/info`

Behavior:

1. resolve toko balance lokal
2. optional resolve single player
3. call upstream
4. response tetap map ke username lokal

### `POST /api/v1/game/log`

Behavior:

1. resolve ext_username
2. pass `start` dan `end` presisi `Y-m-d H:i:s`
3. response publik hanya fields yang di-whitelist

### `POST /api/v1/user/deposit`

Behavior:

1. resolve player
2. cek `nexusggr` balance toko cukup
3. call upstream
4. dalam transaction DB:
   - decrement `nexusggr`
   - create transaction `category=nexusggr`, `type=deposit`
5. response map local username

### `POST /api/v1/user/withdraw`

Behavior:

1. resolve player
2. cek balance player di upstream cukup
3. call upstream
4. dalam transaction DB:
   - increment `nexusggr`
   - create transaction `category=nexusggr`, `type=withdrawal`
5. response map local username

### `POST /api/v1/user/withdraw-reset`

Behavior:

1. optional single player atau all users
2. call upstream
3. create transaction entries sesuai response reset
4. response map local username

### `POST /api/v1/transfer/status`

Behavior:

1. resolve player
2. call upstream by ext_username
3. sanitize response

### `GET /api/v1/call/players`

Behavior:

1. call upstream active players
2. filter ke player yang accessible ke toko
3. map ext_username -> username lokal

### `POST /api/v1/call/list`

Behavior:

1. call upstream
2. sanitize response

### `POST /api/v1/call/apply`

Behavior:

1. resolve player
2. call upstream with ext_username
3. sanitize response

### `POST /api/v1/call/history`

Behavior:

1. call upstream
2. filter only accessible players
3. map ext_username -> username lokal

### `POST /api/v1/call/cancel`

Behavior:

1. call upstream
2. sanitize response

### `POST /api/v1/control/rtp`

Behavior:

1. resolve player
2. call upstream with ext_username
3. sanitize response

### `POST /api/v1/control/users-rtp`

Behavior:

1. untuk dashboard bulk mode, backend langsung ambil seluruh player accessible
2. untuk API bridge toko, tetap dukung payload yang dibutuhkan kontrak saat ini
3. kirim stringified JSON jika upstream mengharuskannya

---

## QRIS / VA Route Mapping Tasks

### `POST /api/v1/merchant-active`

Behavior:

1. auth toko
2. return status store internal
3. return balance toko
4. jangan bocorkan secret upstream

### `POST /api/v1/generate`

Behavior:

1. auth toko
2. validate amount dan terminal/user reference
3. call upstream generate
4. create local transaction `qris + deposit + pending`
5. return sanitized QR payload

### `POST /api/v1/check-status`

Behavior:

1. auth toko
2. cari transaksi hanya milik toko itu
3. return status sanitized

### `GET /api/v1/balance`

Behavior:

1. auth toko
2. return `pending`, `settle`, `nexusggr`

---

## Webhook Tasks

## Phase W1. Inbound QRIS Webhook

Implement `POST /api/webhook/qris`.

Required behavior:

1. validate payload schema
2. verify upstream identity semampu kontrak upstream
3. dedupe by `trx_id`
4. enqueue async processing
5. return quick success response

Processing rules:

1. find pending transaction by `code = trx_id`, category `qris`, type `deposit`
2. if not found -> log warning, no crash
3. if already non-pending -> no-op
4. update transaction to success
5. branch by purpose:
   - regular deposit -> increment pending after fee deduction
   - nexusggr topup -> increment nexusggr by conversion formula
6. notify toko callback_url
7. optional internal notification to dashboard user

Important:

- callback ke toko hanya payload sanitized
- jangan kirim merchant secret

## Phase W2. Inbound Disbursement Webhook

Implement `POST /api/webhook/disbursement`.

Required behavior:

1. validate payload schema
2. dedupe by `partner_ref_no`
3. enqueue async processing

Processing rules:

1. find pending withdrawal transaction by code
2. update transaction status
3. if success:
   - platform fee masuk income
4. if failed:
   - refund settle
5. notify toko callback_url

---

## Callback Outbound Tasks

Implement service pengirim callback ke toko.

Requirements:

1. HTTP POST JSON
2. timeout ketat
3. retry dengan backoff
4. idempotency per event + reference
5. structured log
6. payload sanitized

Recommended headers:

- `Content-Type: application/json`
- `Accept: application/json`
- `X-Bridge-Event: qris|disbursement`
- `X-Bridge-Reference: <reference>`

Optional but recommended:

- signature HMAC internal untuk callback toko

Jika HMAC ditambahkan:

1. simpan callback secret per toko
2. kirim header signature
3. dokumentasikan verification flow untuk toko

---

## Dashboard Frontend Route Tasks

Implement page Vue untuk semua area ini.

## Auth Pages

- login
- register

## Dashboard Shell

- app layout
- top navigation
- sidebar navigation
- role-aware navigation

## Resource Pages

- users list/create/edit
- tokos list/create/edit
- banks list/create/edit
- players list
- transactions list/detail

## Tooling Pages

- providers
- games
- Game Log
- Call Management
- Withdrawal
- NexusGGR Topup
- API Docs

### UI Rules

1. tampilkan loading state
2. tampilkan unauthorized state
3. tampilkan not found state
4. tampilkan retry state untuk network errors
5. semua nominal uang harus diformat IDR
6. semua page sensitif harus jelas menunjukkan toko yang sedang dipakai

---

## Detailed Page Requirements

## Players Page

Behavior:

- `dev` / `superadmin`: lihat semua player
- `admin` / `user`: lihat semua player dari toko yang berelasi
- action money info per player
- jangan tampilkan ext_username ke role yang tidak perlu

## Transactions Page

Behavior:

- tampilkan transaksi `qris` dan `nexusggr`
- filter category, type, status, toko, date range, amount range
- detail modal tidak boleh error jika syntax highlighter tidak ada
- note payload harus bisa dilihat dengan aman

## Withdrawal Page

Behavior:

- pilih toko sesuai ownership
- pilih rekening tujuan sesuai ownership
- inquiry sebelum submit
- tampilkan estimasi:
  - amount
  - bank fee
  - platform fee
  - total deduction
  - remaining settle
- submit hanya jika saldo cukup

## NexusGGR Topup Page

Behavior:

- pilih toko sesuai ownership
- input nominal
- tampilkan estimasi balance NexusGGR yang akan didapat
- generate QRIS
- polling / refresh status

## Game Log Page

Behavior:

- player select searchable
- jangan tampilkan identifier yang tidak perlu
- `start` dan `end` selalu kirim `Y-m-d H:i:s`
- summary records / bet / win harus benar

## Call Management Page

Behavior:

- active players hanya yang accessible
- call history hanya yang accessible
- control RTP single user memakai player owned
- control users RTP bulk mengikuti aturan bisnis yang benar

## API Documentation Page

Behavior:

- tampilkan semua endpoint bridge
- tampilkan contoh request
- tampilkan contoh response
- tampilkan contoh callback yang dikirim ke toko
- jangan tampilkan secret upstream

---

## Security Tasks

## S1. Ownership Enforcement

Semua handler berikut harus re-check ownership di backend:

- bank actions
- toko actions
- withdrawal
- players lookup
- transactions query/detail
- game log
- call management
- qris check-status

## S2. Secret Redaction

Implement redaction layer untuk log dan response.

Jangan pernah expose:

- upstream client key
- upstream secret
- merchant uuid global
- token session
- JWT raw value

## S3. Rate Limiting

Implement rate limits:

- login
- register
- captcha refresh
- webhook inbound
- callback outbound retries
- `/api/v1/*`

## S4. Input Validation

Gunakan typed validators untuk:

- auth payload
- bridge API payload
- webhook payload
- dashboard forms

---

## Testing Tasks

Semua flow inti wajib memiliki automated tests.

## T1. Unit Tests

Minimal:

- money calculation helpers
- fee calculation
- GGR conversion
- JWT validation
- CSRF verification
- captcha verification
- username mapping

## T2. Integration Tests

Minimal:

- login/logout/session restore
- `/api/v1/*` endpoint parity
- webhook qris happy path
- webhook qris duplicate delivery
- webhook disbursement success
- webhook disbursement failure refund
- callback toko delivery success/failure
- player create / deposit / withdraw

## T3. Frontend Tests

Minimal:

- auth pages
- guarded routes
- withdrawal wizard
- NexusGGR topup page
- transactions filters
- players list visibility

## T4. Contract Tests

Buat test yang memastikan response shape route bridge tetap sama seperti versi lama.

Ini penting agar aplikasi toko tidak rusak.

## T5. Security Tests

Minimal:

- request tanpa CSRF ditolak
- request dengan session invalid ditolak
- admin/user tidak bisa akses toko orang lain
- secret upstream tidak muncul di callback payload

---

## Migration Strategy Tasks

Jangan lakukan big bang rewrite tanpa jaring pengaman.

Gunakan strategi bertahap:

## M1. Freeze contract

Sebelum rewrite massal:

1. dokumentasikan semua route
2. dokumentasikan semua response shape
3. dokumentasikan semua callback shape

## M2. Keep schema compatible

Tahap awal Rust backend sebaiknya tetap bisa membaca schema lama.

## M3. Implement parity tests

Buat suite parity terhadap Laravel lama.

## M4. Feature-by-feature cutover

Urutan cutover yang aman:

1. auth dashboard
2. read-only resources
3. players / transactions read-side
4. nexusggr bridge routes
5. qris generate/check-status
6. webhook inbound
7. withdrawal flow
8. callback outbound

## M5. Final switch

Setelah parity dan tests hijau:

1. arahkan traffic ke backend Rust
2. simpan Laravel sebagai fallback selama masa observasi

---

## Explicit Deliverables

Rewrite dinyatakan lengkap jika menghasilkan:

1. frontend Vue yang menggantikan dashboard Filament
2. backend Rust yang menggantikan controller/service Laravel
3. worker Rust untuk webhook/callback async
4. Redis session + cache + rate limit + captcha store
5. JWT cookie auth + CSRF like Laravel
6. captcha self-hosted
7. API bridge parity
8. webhook parity
9. callback toko parity
10. automated tests untuk flow uang
11. dokumentasi API baru

---

## Checklist Eksekusi Ringkas

Gunakan checklist ini untuk implementasi bertahap:

- [ ] baca `goals.md`
- [ ] petakan semua route lama
- [ ] petakan semua model lama
- [ ] definisikan schema DB target
- [ ] scaffold backend Rust
- [ ] scaffold frontend Vue
- [ ] implement auth + JWT + Redis session
- [ ] implement CSRF like Laravel
- [ ] implement captcha
- [ ] implement toko API auth
- [ ] implement players module
- [ ] implement transactions module
- [ ] implement balances module
- [ ] implement NexusGGR client
- [ ] implement QRIS client
- [ ] implement `/api/v1` routes
- [ ] implement webhook inbound
- [ ] implement callback outbound
- [ ] implement dashboard pages
- [ ] implement API docs page
- [ ] implement parity tests
- [ ] implement financial safety tests
- [ ] run migration / cutover rehearsal

---

## Final Reminder untuk AI Implementer

Jika bingung saat rewrite:

1. baca `goals.md`
2. jangan ubah kontrak lebih dulu
3. jangan bocorkan secret
4. jangan hilangkan callback toko
5. jangan longgarkan ownership
6. jangan gunakan float untuk uang
7. jangan expose ext_username ke tempat yang tidak perlu
8. jangan memindahkan logic finansial ke frontend

