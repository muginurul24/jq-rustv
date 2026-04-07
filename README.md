# jq-rustv

Rewrite of `justqiuv2` into a Rust + Vue stack.

## Stack

- Rust workspace
  - `apps/api`: Axum HTTP API
  - `apps/worker`: background job consumers
  - `crates/*`: auth, Redis, upstream clients, domain, errors
- Vue 3 + Vite
  - `apps/web`: dashboard SPA
- PostgreSQL
- Redis

## Main Surfaces

- `/api/v1/*`
  - Toko-authenticated bridge API
  - NexusGGR and QRIS integration routes
- `/api/webhook/*`
  - inbound QRIS and disbursement webhooks
- `/backoffice/api/*`
  - dashboard session auth and backoffice API

## Local Setup

1. Copy env and adjust values as needed.
2. Ensure PostgreSQL and Redis are running.
3. Run migrations.
4. Run the full verification script.

```bash
cp .env.example .env
sqlx migrate run
./scripts/verify-all.sh
```

`./scripts/verify-all.sh` runs:

- `cargo test --workspace -- --nocapture`
- `npm --prefix apps/web run build`

## Required Environment

See [`.env.example`](./.env.example).

Minimum local variables:

```env
DATABASE_URL=postgresql://postgres:postgres@localhost:5432/justqiu
REDIS_URL=redis://localhost:6379/0
JWT_SECRET=change-me-in-production
```

Bridge runtime variables:

```env
NEXUSGGR_API_URL=
NEXUSGGR_AGENT_CODE=
NEXUSGGR_AGENT_TOKEN=

QRIS_API_URL=
QRIS_CLIENT=
QRIS_CLIENT_KEY=
QRIS_MERCHANT_UUID=
```

## Frontend

Install and run the dashboard locally:

```bash
npm --prefix apps/web ci
npm --prefix apps/web run dev
```

## API / Worker

Run the API:

```bash
cargo run -p justqiu-api
```

Run the worker:

```bash
cargo run -p justqiu-worker
```

## CI

GitHub Actions workflow lives in [`.github/workflows/ci.yml`](./.github/workflows/ci.yml).

It provisions PostgreSQL and Redis, runs migrations, then executes:

```bash
./scripts/verify-all.sh
```

## Reference Docs

- [`docs/architecture.md`](./docs/architecture.md)
- [`docs/contracts.md`](./docs/contracts.md)
- [`docs/phase-1-auth-plan.md`](./docs/phase-1-auth-plan.md)
- [`task.md`](./task.md)
