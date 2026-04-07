# Architecture: justqiuv2 Rewrite — Vue + Rust

## Tujuan Dokumen

Dokumen ini menjelaskan arsitektur teknis target rewrite.
Setiap keputusan arsitektur di sini harus konsisten dengan `goals.md` dan `task.md`.

---

## System Topology

```
┌─────────────────────────────────────────────────────────────────┐
│                         INTERNET                                │
│                                                                 │
│  ┌──────────────┐      ┌──────────────┐     ┌──────────────┐   │
│  │  Toko Client │      │  NexusGGR    │     │  QRIS/VA     │   │
│  │  (downstream)│      │  (upstream)  │     │  (upstream)  │   │
│  └──────┬───────┘      └──────┬───────┘     └──────┬───────┘   │
│         │                     │                    │            │
└─────────┼─────────────────────┼────────────────────┼────────────┘
          │                     │                    │
          │ Bearer token        │ POST /             │ Various endpoints
          │ /api/v1/*           │ {method: ...}      │
          ▼                     ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                    RUST BACKEND (Axum)                           │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    HTTP Layer                             │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐               │   │
│  │  │ /api/v1  │  │ /api/    │  │ /backoff │               │   │
│  │  │ (bridge) │  │ webhook  │  │ ice/api  │               │   │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘               │   │
│  │       │              │             │                      │   │
│  │  ┌────▼─────┐  ┌────▼─────┐  ┌────▼─────┐               │   │
│  │  │Toko Auth │  │Rate Limit│  │JWT+CSRF  │               │   │
│  │  │Middleware │  │Middleware│  │Middleware │               │   │
│  │  └──────────┘  └──────────┘  └──────────┘               │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                   Service Layer                           │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐ │   │
│  │  │ Balance  │  │ Player   │  │Transaction│  │ Income   │ │   │
│  │  │ Service  │  │ Service  │  │ Service   │  │ Service  │ │   │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘ │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                Upstream Client Layer                       │   │
│  │  ┌───────────────────┐  ┌───────────────────┐            │   │
│  │  │ NexusGGR Client   │  │ QRIS Client       │            │   │
│  │  │ (single POST /)   │  │ (multiple endpts) │            │   │
│  │  └───────────────────┘  └───────────────────┘            │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                 Repository Layer (SQLx)                    │   │
│  │  users │ tokos │ banks │ balances │ players │ transactions│   │
│  │  incomes │ personal_access_tokens                         │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────┬───────────────────────┬───────────────────────────────────┘
      │                       │
      ▼                       ▼
┌──────────┐           ┌──────────┐
│PostgreSQL│           │  Redis   │
└──────────┘           └──────────┘
      ▲                       ▲
      │                       │
┌─────┴───────────────────────┴───────────────────────────────────┐
│                    RUST WORKER                                   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Job Consumers                                            │   │
│  │  ┌──────────────────┐  ┌──────────────────┐              │   │
│  │  │ProcessQrisCallback│  │ProcessDisbursement│              │   │
│  │  └──────────────────┘  └──────────────────┘              │   │
│  │  ┌──────────────────┐  ┌──────────────────┐              │   │
│  │  │SendTokoCallback  │  │ExpirePendingTx   │              │   │
│  │  └──────────────────┘  └──────────────────┘              │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    VUE 3 SPA (Vite)                              │
│                                                                 │
│  Browser ──HTTP──▶ Rust Backend /backoffice/api/*                │
│                    Cookie: session_jwt (HttpOnly)                │
│                    Header: X-XSRF-TOKEN                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## Repository Structure

```
rustv/
├── apps/
│   ├── api/                          # Axum HTTP server
│   │   ├── src/
│   │   │   ├── main.rs               # Entry point, server bootstrap
│   │   │   ├── app.rs                # Router assembly, state injection
│   │   │   ├── config/
│   │   │   │   └── mod.rs            # Typed config from env
│   │   │   ├── http/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── bridge/           # /api/v1/* handlers (toko auth)
│   │   │   │   │   ├── mod.rs
│   │   │   │   │   ├── nexusggr.rs   # 15 NexusGGR bridge endpoints
│   │   │   │   │   └── qris.rs       # 4 QRIS bridge endpoints
│   │   │   │   ├── webhook/          # /api/webhook/* handlers
│   │   │   │   │   ├── mod.rs
│   │   │   │   │   ├── qris.rs
│   │   │   │   │   └── disbursement.rs
│   │   │   │   └── dashboard/        # /backoffice/api/* handlers (JWT auth)
│   │   │   │       ├── mod.rs
│   │   │   │       ├── auth.rs
│   │   │   │       ├── users.rs
│   │   │   │       ├── tokos.rs
│   │   │   │       ├── banks.rs
│   │   │   │       ├── players.rs
│   │   │   │       ├── transactions.rs
│   │   │   │       ├── withdrawal.rs
│   │   │   │       ├── topup.rs
│   │   │   │       ├── providers.rs
│   │   │   │       ├── games.rs
│   │   │   │       ├── game_log.rs
│   │   │   │       └── call_management.rs
│   │   │   ├── middleware/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── toko_auth.rs      # Bearer token → Toko principal
│   │   │   │   ├── session_auth.rs   # JWT cookie → User principal
│   │   │   │   ├── csrf.rs           # X-XSRF-TOKEN validation
│   │   │   │   ├── rate_limit.rs     # Redis-based rate limiting
│   │   │   │   └── request_id.rs     # Correlation ID
│   │   │   └── extractors/
│   │   │       ├── mod.rs
│   │   │       ├── authenticated_toko.rs
│   │   │       └── authenticated_user.rs
│   │   └── Cargo.toml
│   │
│   ├── worker/                       # Background job processor
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   └── jobs/
│   │   │       ├── mod.rs
│   │   │       ├── process_qris.rs
│   │   │       ├── process_disbursement.rs
│   │   │       ├── send_toko_callback.rs
│   │   │       └── expire_pending.rs
│   │   └── Cargo.toml
│   │
│   └── web/                          # Vue 3 SPA
│       ├── src/
│       │   ├── main.ts
│       │   ├── App.vue
│       │   ├── router/
│       │   │   └── index.ts
│       │   ├── stores/
│       │   │   ├── auth.ts           # Pinia auth store
│       │   │   └── csrf.ts
│       │   ├── lib/
│       │   │   ├── axios.ts          # Axios instance with CSRF
│       │   │   └── format.ts         # IDR formatter, etc.
│       │   ├── layouts/
│       │   │   ├── DashboardLayout.vue
│       │   │   └── AuthLayout.vue
│       │   ├── pages/
│       │   │   ├── auth/
│       │   │   ├── users/
│       │   │   ├── tokos/
│       │   │   ├── banks/
│       │   │   ├── players/
│       │   │   ├── transactions/
│       │   │   ├── withdrawal/
│       │   │   ├── nexusggr-topup/
│       │   │   ├── providers/
│       │   │   ├── games/
│       │   │   ├── game-log/
│       │   │   ├── call-management/
│       │   │   └── api-docs/
│       │   └── components/
│       │       └── ui/               # shadcn-vue components
│       ├── index.html
│       ├── package.json
│       ├── vite.config.ts
│       └── tailwind.config.ts
│
├── crates/
│   ├── domain/                       # Domain models, enums, value objects
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── models/
│   │   │   │   ├── user.rs
│   │   │   │   ├── toko.rs
│   │   │   │   ├── balance.rs
│   │   │   │   ├── bank.rs
│   │   │   │   ├── player.rs
│   │   │   │   ├── transaction.rs
│   │   │   │   └── income.rs
│   │   │   └── enums.rs              # Role, Category, TxType, TxStatus
│   │   └── Cargo.toml
│   │
│   ├── database/                     # SQLx repositories
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── pool.rs
│   │   │   └── repositories/
│   │   │       ├── user_repo.rs
│   │   │       ├── toko_repo.rs
│   │   │       ├── balance_repo.rs
│   │   │       ├── bank_repo.rs
│   │   │       ├── player_repo.rs
│   │   │       ├── transaction_repo.rs
│   │   │       └── income_repo.rs
│   │   └── Cargo.toml
│   │
│   ├── auth/                         # JWT, CSRF, Captcha, session
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── jwt.rs
│   │   │   ├── csrf.rs
│   │   │   ├── captcha.rs
│   │   │   ├── session.rs            # Redis session registry
│   │   │   ├── password.rs           # argon2id hash/verify
│   │   │   └── toko_token.rs         # Sanctum-compatible SHA256 lookup
│   │   └── Cargo.toml
│   │
│   ├── redis_store/                  # Redis abstractions
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── session.rs
│   │   │   ├── csrf.rs
│   │   │   ├── captcha.rs
│   │   │   ├── rate_limit.rs
│   │   │   ├── cache.rs
│   │   │   ├── idempotency.rs
│   │   │   └── queue.rs             # Job queue primitives
│   │   └── Cargo.toml
│   │
│   ├── nexusggr_client/              # Typed HTTP client for NexusGGR
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs            # reqwest-based, single POST /
│   │   │   ├── requests.rs          # Typed request builders per method
│   │   │   ├── responses.rs         # Typed response per method
│   │   │   └── errors.rs            # Upstream error → internal enum
│   │   └── Cargo.toml
│   │
│   ├── qris_client/                  # Typed HTTP client for QRIS/VA
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs
│   │   │   ├── requests.rs
│   │   │   ├── responses.rs
│   │   │   └── errors.rs
│   │   └── Cargo.toml
│   │
│   ├── callback_client/              # Outbound callback to toko
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── sender.rs            # HTTP POST + retry + backoff
│   │   │   ├── sanitizer.rs         # Payload whitelist enforcer
│   │   │   └── types.rs
│   │   └── Cargo.toml
│   │
│   ├── contracts/                    # Shared request/response DTOs
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── bridge/              # /api/v1 request/response types
│   │   │   ├── dashboard/           # Dashboard API types
│   │   │   ├── webhook/             # Inbound webhook types
│   │   │   └── callback/            # Outbound callback types
│   │   └── Cargo.toml
│   │
│   ├── errors/                       # Unified error types
│   │   └── ...
│   │
│   └── observability/                # Tracing setup, structured logging
│       └── ...
│
├── migrations/                       # SQLx migration files
│   └── ...
│
├── docs/
│   ├── nexusggr-openapi-3.1.yaml     # Upstream spec (read-only)
│   ├── API Qris & VA V3.postman_collection.json  # Upstream spec (read-only)
│   ├── architecture.md               # This file
│   ├── contracts.md                  # API contract freeze
│   ├── phase-0-checklist.md
│   └── phase-1-auth-plan.md
│
├── goals.md                          # Source of truth — business goals
├── task.md                           # Source of truth — implementation tasks
├── Cargo.toml                        # Workspace root
└── .env.example
```

---

## Layer Responsibilities

### HTTP Layer (`apps/api/src/http/`)

- Route registration
- Request deserialization → typed DTO (from `crates/contracts`)
- Call service layer
- Serialize response DTO
- **Tidak** mengandung business logic

### Middleware Layer (`apps/api/src/middleware/`)

| Middleware | Scope | Detail |
|---|---|---|
| `toko_auth` | `/api/v1/*` | Bearer token → SHA256 → lookup `personal_access_tokens` → resolve `Toko`. Auth result **selalu Toko**, bukan User. |
| `session_auth` | `/backoffice/api/*` | Cookie `session_jwt` → JWT decode → Redis `session:{sid}` → resolve `User` + role |
| `csrf` | `/backoffice/api/*` (mutating only) | Read `X-XSRF-TOKEN` header → validate against session-bound secret |
| `rate_limit` | All routes | Redis-based sliding window. Key patterns per `task.md` Redis Design. |
| `request_id` | All routes | Generate/propagate `X-Request-Id` for correlation |

### Service Layer (`crates/` services)

- **BalanceService**: Semua mutasi `pending`, `settle`, `nexusggr` wajib lewat sini. Lock + ledger.
- **PlayerService**: Username ↔ ext_username mapping. Visibility scoping.
- **TransactionService**: Create ledger rows. Status transitions. Sanitize note payload.
- **IncomeService**: Load fee config. Update platform income.
- **AuthService**: Login/register/logout. Session management.

### Repository Layer (`crates/database/`)

- Pure data access — SELECT, INSERT, UPDATE
- SQLx compile-time checked queries
- Tidak mengandung business logic
- Semua query yang involve uang pakai `SELECT ... FOR UPDATE`

### Upstream Client Layer (`crates/nexusggr_client/`, `crates/qris_client/`)

**NexusGGR Client**:
- Single endpoint: `POST https://api.nexusggr.com/`
- Discriminator: `method` field di JSON body
- Credentials: `agent_code` + `agent_token` (dari config, **tidak pernah ke client**)
- Response: check `status == 1` untuk success. Error bisa di `msg` atau `error`.

**QRIS Client**:
- Multiple endpoints: `merchant_active`, `generate`, `checkstatus/v2/{trx_id}`, `inquiry`, `transfer`, `balance`
- Base URL: `https://rest.otomatis.vip/api/`
- Auth: `uuid` (merchant UUID, dari config, **tidak pernah ke client**)

### Worker Layer (`apps/worker/`)

- Consume jobs dari Redis queue
- Setiap job = struct + handler
- Retry dengan exponential backoff
- Idempotency enforced via Redis key sebelum processing

---

## Auth Architecture

### Dashboard Session (JWT Cookie)

```
Login flow:
  1. POST /backoffice/api/auth/login
     Body: { username, password, captcha_id, captcha_answer }
  2. Verify captcha (Redis lookup → compare hash)
  3. Verify password (argon2id)
  4. Generate session ID (UUID v4)
  5. Store session in Redis:
     Key: session:{sid}
     Value: { user_id, role, csrf_secret, issued_at, expires_at, ip_hash, ua_hash }
     TTL: session expiry (e.g. 8 hours)
  6. Sign JWT:
     Claims: { sub: user_id, role, sid, exp, iat }
  7. Set cookies:
     - session_jwt (HttpOnly, Secure, SameSite=Lax, Path=/)
     - XSRF-TOKEN (NOT HttpOnly, Secure, SameSite=Lax, Path=/)
  8. Return { user: { id, username, name, role } }
```

### Toko API Auth (Bearer Token)

```
Request flow:
  1. Toko sends: Authorization: Bearer {plaintext_token}
  2. Middleware:
     a. Extract token from header
     b. Split token by "|" → ID part (before |) is token_id; remainder is plaintext_key
        Note: Sanctum format = "{id}|{40_char_plaintext}"
     c. SHA256 hash the plaintext_key
     d. Lookup personal_access_tokens WHERE id = token_id AND token = sha256_hash
     e. Resolve tokenable_type = "App\Models\Toko", tokenable_id → load Toko
  3. Auth result = Toko struct (NOT User)
  4. All subsequent handlers receive Toko context
```

> Sanctum stores tokens as `SHA256(plaintext)` in the `token` column of `personal_access_tokens`.
> The bearer value sent by clients is `{id}|{plaintext}`.
> We replicate this exact logic in Rust for backward compatibility.

### CSRF Flow

```
1. Server sets cookie: XSRF-TOKEN = random_token (derived from csrf_secret in session)
2. Frontend (Axios) reads XSRF-TOKEN cookie, sends as: X-XSRF-TOKEN header
3. Server validates: header token matches session's csrf_secret
4. Applied to: all POST/PUT/PATCH/DELETE on /backoffice/api/*
5. NOT applied to: /api/v1/* (uses bearer token auth instead)
6. NOT applied to: /api/webhook/* (upstream has its own auth)
```

---

## Data Flow Architecture

### Financial Transaction Flow

Setiap operasi uang mengikuti pattern yang sama:

```
1. Validate input
2. Check balances (with lock)
3. Call upstream if needed
4. BEGIN DB TRANSACTION
   a. Mutate balance (SELECT FOR UPDATE → increment/decrement)
   b. Create Transaction ledger row
   c. Update Income if applicable
5. COMMIT
6. Enqueue callback to toko (if callback_url exists)
7. Log structured event
```

### Webhook Processing Flow

```
Upstream → POST /api/webhook/{qris|disbursement}
  │
  ├── 1. Validate payload schema
  ├── 2. Check idempotency key in Redis
  │      Key: idempotency:webhook:{type}:{ref}
  │      If exists → return 200 OK immediately
  ├── 3. Set idempotency key (TTL 24h)
  ├── 4. Enqueue job to Redis queue
  └── 5. Return { status: true, message: "OK" }

Worker picks up job:
  │
  ├── 1. Strip merchant_id from payload
  ├── 2. Find pending transaction by code
  │      If not found → log warning, return
  │      If not pending → no-op, return
  ├── 3. BEGIN DB TRANSACTION
  │      ├── Update transaction status
  │      ├── Mutate balance (varies by type)
  │      └── Update income
  ├── 4. COMMIT
  ├── 5. If toko.callback_url filled:
  │      Enqueue SendTokoCallback job
  └── 6. Log structured event
```

### Callback Outbound Flow

```
SendTokoCallback job:
  │
  ├── 1. Validate callback_url (not empty, valid URL)
  ├── 2. Check idempotency: idempotency:callback:{event_type}:{reference}
  ├── 3. Sanitize payload:
  │      ├── REMOVE: merchant_id
  │      ├── REMOVE: any field not in whitelist
  │      ├── NEVER include: upstream secrets, raw errors
  │      └── See contracts.md for exact whitelist
  ├── 4. HTTP POST to callback_url
  │      ├── Content-Type: application/json
  │      ├── Accept: application/json
  │      ├── X-Bridge-Event: {qris|disbursement}
  │      ├── X-Bridge-Reference: {reference}
  │      └── Timeout: 10 seconds
  ├── 5. If success → log, done
  └── 6. If failure → retry with backoff [10, 30, 60]s (4 attempts total)
```

---

## Redis Key Schema

Exact key patterns (from `task.md`):

| Purpose | Key Pattern | TTL | Value |
|---|---|---|---|
| Session | `session:{sid}` | Session expiry (8h) | JSON: `{user_id, role, csrf_secret, issued_at, expires_at, ip_hash, ua_hash}` |
| CSRF | Embedded in session blob | — | Part of session JSON |
| Captcha | `captcha:{captcha_id}` | 5 minutes | Hashed answer + metadata |
| Rate limit - login | `rl:login:{ip}` | Window-based | Counter |
| Rate limit - API | `rl:api:{toko_id}:{route}` | Window-based | Counter |
| Rate limit - webhook | `rl:webhook:{source}:{route}` | Window-based | Counter |
| Cache - providers | `cache:nexusggr:provider-list` | 1 day | JSON provider list |
| Cache - games | `cache:nexusggr:game-list:{provider_code}` | 1 day | JSON game list |
| Idempotency - QRIS | `idempotency:webhook:qris:{trx_id}` | 24 hours | `1` |
| Idempotency - disbursement | `idempotency:webhook:disbursement:{partner_ref_no}` | 24 hours | `1` |
| Idempotency - callback | `idempotency:callback:{event_type}:{reference}` | 24 hours | `1` |
| Job queue | `queue:jobs:{queue_name}` | — | Job payloads (RPUSH/BLPOP) |

---

## Database Schema

Retain PostgreSQL schema from Laravel. Key tables:

### users
```sql
id              BIGSERIAL PRIMARY KEY
username        VARCHAR NOT NULL UNIQUE
name            VARCHAR NOT NULL
email           VARCHAR NOT NULL UNIQUE
email_verified_at TIMESTAMPTZ
password        VARCHAR NOT NULL          -- argon2id hash
role            VARCHAR NOT NULL DEFAULT 'user'  -- dev|superadmin|admin|user
is_active       BOOLEAN NOT NULL DEFAULT true
remember_token  VARCHAR
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
```

### tokos
```sql
id              BIGSERIAL PRIMARY KEY
user_id         BIGINT NOT NULL REFERENCES users(id)
name            VARCHAR NOT NULL
callback_url    VARCHAR
token           VARCHAR                   -- legacy; actual token in personal_access_tokens
is_active       BOOLEAN NOT NULL DEFAULT true
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
deleted_at      TIMESTAMPTZ
```

### personal_access_tokens (Sanctum compatibility)
```sql
id              BIGSERIAL PRIMARY KEY
tokenable_type  VARCHAR NOT NULL          -- "App\Models\Toko"
tokenable_id    BIGINT NOT NULL           -- toko.id
name            VARCHAR NOT NULL
token           VARCHAR(64) NOT NULL UNIQUE  -- SHA256 of plaintext
abilities       TEXT
last_used_at    TIMESTAMPTZ
expires_at      TIMESTAMPTZ
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
```

### balances
```sql
id              BIGSERIAL PRIMARY KEY
toko_id         BIGINT NOT NULL UNIQUE REFERENCES tokos(id)
pending         BIGINT NOT NULL DEFAULT 0
settle          BIGINT NOT NULL DEFAULT 0
nexusggr        BIGINT NOT NULL DEFAULT 0
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
```

### banks
```sql
id              BIGSERIAL PRIMARY KEY
user_id         BIGINT NOT NULL REFERENCES users(id)
bank_code       VARCHAR NOT NULL
bank_name       VARCHAR NOT NULL
account_number  VARCHAR NOT NULL
account_name    VARCHAR NOT NULL
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
deleted_at      TIMESTAMPTZ
```

### players
```sql
id              BIGSERIAL PRIMARY KEY
toko_id         BIGINT NOT NULL REFERENCES tokos(id)
username        VARCHAR NOT NULL
ext_username    VARCHAR NOT NULL UNIQUE    -- ULID, globally unique
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
deleted_at      TIMESTAMPTZ

UNIQUE (toko_id, username)                -- username unique per toko
```

### transactions
```sql
id              BIGSERIAL PRIMARY KEY
toko_id         BIGINT NOT NULL REFERENCES tokos(id)
player          VARCHAR                   -- local username or terminal_id
external_player VARCHAR                   -- ext_username (internal only)
category        VARCHAR NOT NULL          -- qris | nexusggr
type            VARCHAR NOT NULL          -- deposit | withdrawal
status          VARCHAR NOT NULL          -- pending | success | failed | expired
amount          BIGINT NOT NULL           -- integer rupiah
code            VARCHAR                   -- trx_id or partner_ref_no
note            TEXT                      -- JSON blob
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
deleted_at      TIMESTAMPTZ
```

### incomes
```sql
id              BIGSERIAL PRIMARY KEY
ggr             BIGINT NOT NULL DEFAULT 0   -- GGR conversion ratio
fee_transaction BIGINT NOT NULL DEFAULT 0   -- percentage for QRIS deposit fee
fee_withdrawal  BIGINT NOT NULL DEFAULT 0   -- percentage for withdrawal fee
amount          BIGINT NOT NULL DEFAULT 0   -- accumulated platform income
created_at      TIMESTAMPTZ
updated_at      TIMESTAMPTZ
```

---

## Error Architecture

```
Upstream error (raw)
  → crates/nexusggr_client/errors.rs or crates/qris_client/errors.rs
  → Internal error enum (e.g. NexusggrError::InvalidUser, QrisError::InsufficientBalance)
  → crates/errors/ → public error DTO
  → HTTP handler → JSON response with sanitized message only

NEVER: forward raw upstream JSON, error messages, or status codes to client
```

Example error mapping:
```
NexusGGR upstream: { status: 0, msg: "INSUFFICIENT_AGENT_FUNDS" }
  → NexusggrError::InsufficientAgentFunds
  → Public: { success: false, message: "Insufficient balance" }

QRIS upstream: { status: false, error: "Toko not valid" }
  → QrisError::InvalidMerchant
  → Public: { success: false, message: "Failed to generate QRIS from upstream provider" }
```

---

## Configuration

All config via environment variables, loaded into typed struct at startup:

```
# Database
DATABASE_URL=postgresql://user:pass@host:5432/dbname

# Redis
REDIS_URL=redis://host:6379/0

# NexusGGR upstream
NEXUSGGR_API_URL=https://api.nexusggr.com
NEXUSGGR_AGENT_CODE=***
NEXUSGGR_AGENT_TOKEN=***

# QRIS upstream
QRIS_API_URL=https://rest.otomatis.vip/api
QRIS_MERCHANT_UUID=***

# JWT
JWT_SECRET=***
JWT_EXPIRY_HOURS=8

# Server
BIND_ADDRESS=0.0.0.0:8080
RUST_LOG=info
```

> [!CAUTION]
> `NEXUSGGR_AGENT_CODE`, `NEXUSGGR_AGENT_TOKEN`, `QRIS_MERCHANT_UUID`, and `JWT_SECRET` are secrets.
> They must NEVER appear in frontend build output, API responses, callback payloads, or logs.
