# Phase 0 Checklist: Scaffold Monorepo

## Tujuan

Setup repository structure, toolchain, database, dan dependency sebelum coding dimulai.
Setelah checklist ini selesai, repo harus bisa:
- `cargo check` tanpa error
- `npm run dev` menampilkan halaman kosong Vue
- PostgreSQL schema ready (tabel inti)
- Redis reachable
- Worker binary compilable

---

## Prerequisite

- [ ] Rust toolchain (stable, ≥ 1.78) — `rustup show`
- [ ] Node.js ≥ 20 + npm — `node -v`
- [ ] PostgreSQL ≥ 15 running — `psql --version`
- [ ] Redis ≥ 7 running — `redis-cli ping`
- [ ] SQLx CLI installed — `cargo install sqlx-cli --no-default-features --features rustls,postgres`

---

## 0.1 — Init Cargo Workspace

- [ ] Create `Cargo.toml` di root sebagai workspace:
  ```toml
  [workspace]
  resolver = "2"
  members = [
    "apps/api",
    "apps/worker",
    "crates/domain",
    "crates/database",
    "crates/auth",
    "crates/redis_store",
    "crates/nexusggr_client",
    "crates/qris_client",
    "crates/callback_client",
    "crates/contracts",
    "crates/errors",
    "crates/observability",
  ]
  ```
- [ ] `cargo init apps/api --name justqiu-api`
- [ ] `cargo init apps/worker --name justqiu-worker`
- [ ] `cargo init crates/domain --lib --name justqiu-domain`
- [ ] `cargo init crates/database --lib --name justqiu-database`
- [ ] `cargo init crates/auth --lib --name justqiu-auth`
- [ ] `cargo init crates/redis_store --lib --name justqiu-redis`
- [ ] `cargo init crates/nexusggr_client --lib --name justqiu-nexusggr`
- [ ] `cargo init crates/qris_client --lib --name justqiu-qris`
- [ ] `cargo init crates/callback_client --lib --name justqiu-callback`
- [ ] `cargo init crates/contracts --lib --name justqiu-contracts`
- [ ] `cargo init crates/errors --lib --name justqiu-errors`
- [ ] `cargo init crates/observability --lib --name justqiu-observability`
- [ ] Verify: `cargo check` passes

---

## 0.2 — Workspace Dependencies

- [ ] Add shared dependencies di workspace `Cargo.toml`:
  ```toml
  [workspace.dependencies]
  axum = "0.8"
  tokio = { version = "1", features = ["full"] }
  sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "uuid"] }
  redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
  reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  jsonwebtoken = "9"
  argon2 = "0.5"
  sha2 = "0.10"
  tower = "0.5"
  tower-http = { version = "0.6", features = ["cors", "trace", "request-id", "set-header"] }
  tracing = "0.1"
  tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
  chrono = { version = "0.4", features = ["serde"] }
  uuid = { version = "1", features = ["v4", "serde"] }
  thiserror = "2"
  dotenvy = "0.15"
  ```
- [ ] Each crate references workspace deps: `axum.workspace = true`
- [ ] Verify: `cargo check` still passes

---

## 0.3 — Init Vue Frontend

- [ ] `npx -y create-vite@latest apps/web -- --template vue-ts`
  (If interactive, answer: Vue, TypeScript)
- [ ] `cd apps/web && npm install`
- [ ] Install core deps:
  ```bash
  npm install vue-router@4 pinia @vueuse/core axios
  npm install vee-validate zod @vee-validate/zod
  npm install -D tailwindcss@4 @tailwindcss/vite
  npm install lucide-vue-next
  ```
- [ ] Setup Tailwind v4 in `vite.config.ts` (plugin) and `src/styles/main.css` (`@import "tailwindcss"`)
- [ ] Setup shadcn-vue: `npx shadcn-vue@latest init` (follow prompts, use default config)
- [ ] Verify: `npm run dev` shows a page
- [ ] Create placeholder dirs: `src/pages/`, `src/stores/`, `src/layouts/`, `src/lib/`, `src/router/`

---

## 0.4 — Environment Configuration

- [ ] Create `.env.example` at repo root:
  ```env
  DATABASE_URL=postgresql://postgres:postgres@localhost:5432/justqiu
  REDIS_URL=redis://localhost:6379/0

  NEXUSGGR_API_URL=https://api.nexusggr.com
  NEXUSGGR_AGENT_CODE=
  NEXUSGGR_AGENT_TOKEN=

  QRIS_API_URL=https://rest.otomatis.vip/api
  QRIS_MERCHANT_UUID=

  JWT_SECRET=change-me-in-production
  JWT_EXPIRY_HOURS=8

  BIND_ADDRESS=0.0.0.0:8080
  RUST_LOG=info
  ```
- [ ] Create `.env` (gitignored) from `.env.example`
- [ ] Add `.env` to `.gitignore`
- [ ] Create `apps/api/src/config/mod.rs` — typed config struct that reads from env
  (struct definition only, no implementation logic yet)

---

## 0.5 — Database Setup

- [ ] Create PostgreSQL database: `createdb justqiu`
- [ ] Create `migrations/` directory at repo root
- [ ] Create initial migration:
  ```bash
  sqlx migrate add -r init_schema
  ```
- [ ] Write UP migration with these tables (retain Laravel schema exactly):
  - `users` — id, username, name, email, email_verified_at, password, role, is_active, remember_token, created_at, updated_at
  - `tokos` — id, user_id, name, callback_url, token, is_active, created_at, updated_at, deleted_at
  - `personal_access_tokens` — id, tokenable_type, tokenable_id, name, token(64), abilities, last_used_at, expires_at, created_at, updated_at
  - `balances` — id, toko_id (unique), pending, settle, nexusggr, created_at, updated_at
  - `banks` — id, user_id, bank_code, bank_name, account_number, account_name, created_at, updated_at, deleted_at
  - `players` — id, toko_id, username, ext_username (unique), created_at, updated_at, deleted_at + unique(toko_id, username)
  - `transactions` — id, toko_id, player, external_player, category, type, status, amount, code, note, created_at, updated_at, deleted_at
  - `incomes` — id, ggr, fee_transaction, fee_withdrawal, amount, created_at, updated_at
  - All money columns = `BIGINT NOT NULL DEFAULT 0`
- [ ] Write DOWN migration (drop all tables)
- [ ] Run: `sqlx migrate run`
- [ ] Verify: `psql justqiu -c '\dt'` shows all tables

---

## 0.6 — Domain Types Skeleton

- [ ] `crates/domain/src/models/` — define Rust structs for each entity:
  - `User { id, username, name, email, password, role, is_active, created_at, updated_at }`
  - `Toko { id, user_id, name, callback_url, token, is_active, created_at, updated_at, deleted_at }`
  - `Balance { id, toko_id, pending: i64, settle: i64, nexusggr: i64, created_at, updated_at }`
  - `Bank { id, user_id, bank_code, bank_name, account_number, account_name, created_at, updated_at, deleted_at }`
  - `Player { id, toko_id, username, ext_username, created_at, updated_at, deleted_at }`
  - `Transaction { id, toko_id, player, external_player, category, r#type, status, amount: i64, code, note, created_at, updated_at, deleted_at }`
  - `Income { id, ggr: i64, fee_transaction: i64, fee_withdrawal: i64, amount: i64, created_at, updated_at }`
- [ ] `crates/domain/src/enums.rs`:
  - `Role { Dev, Superadmin, Admin, User }`
  - `TransactionCategory { Qris, Nexusggr }`
  - `TransactionType { Deposit, Withdrawal }`
  - `TransactionStatus { Pending, Success, Failed, Expired }`
- [ ] All structs derive `sqlx::FromRow`, `Serialize`, `Deserialize`
- [ ] All money fields = `i64` — **no floats**
- [ ] Verify: `cargo check`

---

## 0.7 — Error Types Skeleton

- [ ] `crates/errors/src/lib.rs`:
  - `AppError` enum with variants: `NotFound`, `Unauthorized`, `Forbidden`, `BadRequest(String)`, `InternalError(String)`, `UpstreamError(String)`
  - `IntoResponse` impl for Axum — returns `{ "success": false, "message": "..." }`
  - NEVER return raw upstream details
- [ ] Verify: `cargo check`

---

## 0.8 — Observability Skeleton

- [ ] `crates/observability/src/lib.rs`:
  - `init_tracing()` function — sets up `tracing-subscriber` with env filter + JSON format
- [ ] Verify: `cargo check`

---

## 0.9 — API Server Skeleton

- [ ] `apps/api/src/main.rs`:
  - Load `.env` via dotenvy
  - Init tracing
  - Create Axum router (empty, just `/health` returning 200)
  - Bind to `BIND_ADDRESS`
- [ ] `apps/api/src/app.rs`:
  - `AppState` struct holding: `PgPool`, `redis::Client`, config
  - Router assembly function (placeholder)
- [ ] Verify: `cargo run -p justqiu-api` starts and `/health` returns 200

---

## 0.10 — Worker Skeleton

- [ ] `apps/worker/src/main.rs`:
  - Load `.env`
  - Init tracing
  - Connect to Redis + PostgreSQL
  - Empty job loop placeholder (log "worker started")
- [ ] Verify: `cargo run -p justqiu-worker` starts without error

---

## 0.11 — Project Files

- [ ] `.gitignore` — include: `target/`, `node_modules/`, `.env`, `apps/web/dist/`
- [ ] Copy `docs/nexusggr-openapi-3.1.yaml` from justqiuv2
- [ ] Copy `docs/API Qris & VA V3.postman_collection.json` from justqiuv2
- [ ] Copy `goals.md` (already present)
- [ ] Copy `task.md` (already present)

---

## Acceptance Criteria

Phase 0 dianggap selesai jika:

1. `cargo check` — zero errors untuk semua workspace members
2. `cargo run -p justqiu-api` — server starts, `/health` returns 200
3. `cargo run -p justqiu-worker` — worker starts, logs "worker started"
4. `npm run dev` (di `apps/web`) — Vue dev server starts
5. `psql justqiu -c '\dt'` — semua 8 tabel ada
6. `redis-cli ping` — PONG
7. Domain types ada semua 7 models + 4 enums
8. Error types compile
9. No float types untuk money fields
