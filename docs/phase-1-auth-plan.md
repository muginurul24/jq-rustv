# Phase 1 Auth Plan: JWT Cookie Session, Redis, CSRF, Captcha

## Tujuan

Mengimplementasikan infrastruktur autentikasi dashboard dan autentikasi token API toko.
Setelah phase ini selesai:
1. User bisa login/logout dashboard via browser
2. Session di-persist di Redis, invalidate kapan saja
3. CSRF protection aktif untuk semua request mutating dashboard
4. Captcha self-hosted untuk login/register
5. Token toko authenticate `/api/v1` sebagai Toko principal

---

## A1. Dashboard Session Auth

### Login Endpoint

**Route**: `POST /backoffice/api/auth/login`

**Request**:
```json
{
  "username": "admin",
  "password": "secret",
  "captcha_id": "uuid-captcha",
  "captcha_answer": "xK7p"
}
```

**Flow**:
1. Rate limit: `rl:login:{client_ip}` — max 10/minute, 30/hour
2. Validate captcha:
   - Redis GET `captcha:{captcha_id}`
   - Compare hashed answer
   - Delete key (one-time use)
   - If invalid → 422 `{ "success": false, "message": "Captcha is invalid or expired" }`
3. Find user by `username` (case-insensitive)
   - If not found → 401 `{ "success": false, "message": "Invalid credentials" }`
4. Check `user.is_active`
   - If false → 403 `{ "success": false, "message": "Account is not active" }`
5. Verify password with argon2id:
   - `argon2::verify_encoded(&user.password, password_bytes)`
   - If mismatch → 401 `{ "success": false, "message": "Invalid credentials" }`
6. Generate session:
   - `sid` = UUID v4
   - `csrf_secret` = 32 random bytes, hex-encoded
7. Store session in Redis:
   ```
   Key: session:{sid}
   TTL: JWT_EXPIRY_HOURS (default 8 hours)
   Value (JSON):
   {
     "user_id": 1,
     "role": "admin",
     "csrf_secret": "a1b2c3...",
     "issued_at": 1712000000,
     "expires_at": 1712028800,
     "ip_hash": "sha256(client_ip)[:16]",
     "ua_hash": "sha256(user_agent)[:16]"
   }
   ```
8. Sign JWT:
   ```json
   {
     "sub": "1",
     "role": "admin",
     "sid": "uuid-session-id",
     "iat": 1712000000,
     "exp": 1712028800
   }
   ```
   Signed with `JWT_SECRET` (HS256)
9. Set cookies:
   - `session_jwt={jwt_value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={expiry_seconds}`
   - `XSRF-TOKEN={csrf_token}; Secure; SameSite=Lax; Path=/; Max-Age={expiry_seconds}`
   - `csrf_token` derivation: `HMAC-SHA256(csrf_secret, sid)` → hex → first 64 chars
10. Response:
```json
{
  "success": true,
  "user": {
    "id": 1,
    "username": "admin",
    "name": "Admin User",
    "role": "admin"
  }
}
```

### Logout Endpoint

**Route**: `POST /backoffice/api/auth/logout`

**Flow**:
1. Extract JWT from `session_jwt` cookie
2. Decode JWT, get `sid`
3. Delete Redis key `session:{sid}`
4. Clear cookies: `session_jwt` and `XSRF-TOKEN` (set Max-Age=0)
5. Response: `{ "success": true }`

### Current User Endpoint

**Route**: `GET /backoffice/api/auth/me`

**Flow**:
1. Session auth middleware (see below) validates JWT + Redis
2. Return user info from database (loaded via `user_id` from session)
3. Response:
```json
{
  "success": true,
  "user": {
    "id": 1,
    "username": "admin",
    "name": "Admin User",
    "role": "admin"
  }
}
```

### Session Auth Middleware

Applied to all `/backoffice/api/*` routes.

**Flow**:
1. Extract `session_jwt` cookie
   - Missing → 401
2. Decode JWT with `JWT_SECRET`
   - Invalid/expired → 401
3. Extract `sid` from claims
4. Redis GET `session:{sid}`
   - Not found → 401 (session invalidated or expired)
5. Parse session JSON, extract `user_id` and `role`
6. Inject `AuthenticatedUser { user_id, role, sid }` into request extensions
7. Continue to handler

**Critical rule**: JWT alone is NOT sufficient. Redis lookup is mandatory.
A user who logged out (Redis key deleted) must NOT be able to access with a still-valid JWT.

### Password Hashing

- Algorithm: argon2id
- Laravel's default hasher is bcrypt, but the project may use argon2.
- **Migration note**: check existing `users.password` format.
  - If bcrypt (`$2y$...`): Rust verifies with `bcrypt::verify`
  - If argon2id (`$argon2id$...`): Rust verifies with `argon2::verify_encoded`
  - New passwords created by Rust always use argon2id
- Implementation: `crates/auth/src/password.rs`
  - `verify_password(hash: &str, plain: &str) -> bool` — auto-detect format
  - `hash_password(plain: &str) -> String` — always argon2id

---

## A2. CSRF Like Laravel

### Goal

Replicate Laravel's CSRF cookie pattern so Axios works out-of-the-box.

### Design

1. Server sets `XSRF-TOKEN` cookie (NOT HttpOnly — frontend must read it)
2. Axios automatically reads `XSRF-TOKEN` cookie, sends as `X-XSRF-TOKEN` header
3. Server validates header against session's csrf_secret

### CSRF Token Derivation

```
csrf_secret = random 32 bytes (stored in Redis session)
csrf_token  = HMAC-SHA256(key=csrf_secret, data=sid) → hex string → first 64 chars
```

### CSRF Middleware

Applied to: `POST`, `PUT`, `PATCH`, `DELETE` on `/backoffice/api/*`
NOT applied to: `GET`, `HEAD`, `OPTIONS`
NOT applied to: `/api/v1/*` (uses bearer token auth)
NOT applied to: `/api/webhook/*` (upstream auth)

**Flow**:
1. Extract `X-XSRF-TOKEN` header from request
   - Missing → 403 `{ "success": false, "message": "CSRF token missing" }`
2. Get `csrf_secret` and `sid` from session (already loaded by session_auth middleware)
3. Recompute expected token: `HMAC-SHA256(csrf_secret, sid)`
4. Constant-time compare header value vs expected
   - Mismatch → 403 `{ "success": false, "message": "CSRF token mismatch" }`
5. Continue to handler

### Token Rotation

- New `csrf_secret` generated on login
- On logout: secret destroyed with session
- Optional: rotate on each request (trade-off: breaks back/forward navigation)
  - **Decision**: rotate only on login/logout for simplicity, like Laravel default

### Axios Config (Frontend)

```typescript
// lib/axios.ts
import axios from 'axios'

const api = axios.create({
  baseURL: '/backoffice/api',
  withCredentials: true,   // sends cookies
  xsrfCookieName: 'XSRF-TOKEN',
  xsrfHeaderName: 'X-XSRF-TOKEN',
})
```

Axios natively reads `XSRF-TOKEN` cookie and sends `X-XSRF-TOKEN` header — no extra code needed.

---

## A3. Captcha

### Goal

Self-hosted captcha, no external service. Simple image/SVG challenge stored in Redis.

### Generate Endpoint

**Route**: `GET /backoffice/api/auth/captcha`

**Flow**:
1. Generate random alphanumeric string (4-6 chars, avoid confusing chars like 0/O, 1/l/I)
2. Render as SVG with noise (lines, dots, slight rotation per char)
3. Generate `captcha_id` = UUID v4
4. Hash answer: `SHA256(answer_lowercase)`
5. Store in Redis:
   ```
   Key: captcha:{captcha_id}
   TTL: 300 seconds (5 minutes)
   Value: { "hash": "sha256...", "created_at": timestamp }
   ```
6. Response:
```json
{
  "captcha_id": "uuid...",
  "image": "<svg>...</svg>"
}
```

**Content**: SVG string (not base64, not external URL — inline SVG for simplicity).

### Refresh Endpoint

Same as generate — frontend calls `GET /backoffice/api/auth/captcha` again to get a new one. Old captcha key auto-expires via TTL.

### Verification (used by login/register)

Not a separate endpoint. Called internally during login/register processing:

```rust
fn verify_captcha(redis: &Redis, captcha_id: &str, answer: &str) -> Result<(), AppError> {
    let key = format!("captcha:{}", captcha_id);
    let stored = redis.get_del(&key)?;  // get and delete atomically
    if stored.is_none() {
        return Err(AppError::BadRequest("Captcha is invalid or expired"));
    }
    let expected_hash = stored.hash;
    let answer_hash = sha256(answer.to_lowercase());
    if !constant_time_eq(expected_hash, answer_hash) {
        return Err(AppError::BadRequest("Captcha answer is incorrect"));
    }
    Ok(())
}
```

Key points:
- One-time use: `GET_DEL` atomically reads and removes
- Case-insensitive: compare lowercase
- Expired captcha: Redis TTL handles it (returns None)
- Constant-time comparison for hash

### SVG Generation

Use simple procedural SVG — no external image library needed:
- Canvas: 150×50
- Each character: random slight rotation (-15° to +15°), random x-offset
- Background: 3-5 random lines (noise)
- 10-20 random dots
- Font: monospace, size 28-32px
- Colors: random dark-on-light

Implementation: `crates/auth/src/captcha.rs`
- `generate_captcha() -> (captcha_id, answer, svg_string)`
- `store_captcha(redis, captcha_id, answer_hash)`
- `verify_captcha(redis, captcha_id, answer) -> Result<()>`

---

## B1. Toko API Authentication

### Goal

Backward-compatible token auth for `/api/v1/*`. Auth result = Toko (not User).

### Sanctum Token Format

Laravel Sanctum stores tokens as:
- `personal_access_tokens.token` = `SHA256(plaintext_part)`
- Client sends: `Authorization: Bearer {id}|{plaintext_part}`
  - `{id}` = `personal_access_tokens.id`
  - `{plaintext_part}` = 40-char random string

### Toko Auth Middleware

Applied to: all `/api/v1/*` routes

**Flow**:
1. Extract `Authorization` header
   - Missing → 401 `{ "success": false, "message": "Unauthenticated" }`
2. Strip `Bearer ` prefix
3. Split by `|`:
   - Parts must be exactly 2: `[token_id_str, plaintext]`
   - Parse `token_id_str` as i64
4. Compute `SHA256(plaintext)` → hex string
5. Query DB:
   ```sql
   SELECT pat.id, pat.tokenable_id, pat.abilities
   FROM personal_access_tokens pat
   WHERE pat.id = $1
     AND pat.token = $2
     AND pat.tokenable_type = 'App\Models\Toko'
   ```
6. If not found → 401
7. Load Toko by `tokenable_id`:
   ```sql
   SELECT * FROM tokos WHERE id = $1 AND is_active = true AND deleted_at IS NULL
   ```
8. If toko not found or inactive → 401
9. Optionally update `last_used_at` on personal_access_tokens (non-blocking)
10. Inject `AuthenticatedToko(toko)` into request extensions
11. Continue

**Critical**: Auth result is `AuthenticatedToko`, NOT `AuthenticatedUser`.
All subsequent handlers operate in toko context.

### Implementation

`crates/auth/src/toko_token.rs`:
- `verify_toko_token(pool: &PgPool, bearer: &str) -> Result<Toko, AppError>`

`apps/api/src/middleware/toko_auth.rs`:
- Axum middleware that calls `verify_toko_token` and injects result

---

## Rate Limiting

### Redis-based Sliding Window

Implementation: `crates/redis_store/src/rate_limit.rs`

```
Key: rl:{scope}:{identifier}
Value: counter
TTL: window duration
```

### Rate Limit Config

| Scope | Identifier | Limit | Window |
|---|---|---|---|
| `login` | client IP | 10 | 1 minute |
| `login` | client IP | 30 | 1 hour |
| `register` | client IP | 5 | 1 hour |
| `captcha` | client IP | 30 | 1 minute |
| `api` | toko_id | 60 | 1 minute |
| `webhook` | source IP | 60 | 1 minute |

Response when exceeded: `429 Too Many Requests`
```json
{ "success": false, "message": "Rate limit exceeded. Try again later." }
```

---

## Deliverables

After Phase 1 completion:

| What | Verification |
|---|---|
| Login | POST /backoffice/api/auth/login → JWT cookie + XSRF-TOKEN cookie |
| Logout | POST /backoffice/api/auth/logout → Redis session deleted, cookies cleared |
| Session restore | GET /backoffice/api/auth/me → returns user from JWT + Redis |
| CSRF protection | POST without X-XSRF-TOKEN → 403 |
| Captcha generate | GET /backoffice/api/auth/captcha → SVG + captcha_id |
| Captcha validate | Login with wrong captcha → 422 |
| Captcha expire | Login with 5min old captcha_id → 422 |
| Toko auth | Bearer token → AuthenticatedToko |
| Sanctum compat | Existing Sanctum tokens work without regeneration |
| Rate limit | 11th login in 1 min → 429 |
| JWT without Redis | Login, delete Redis key, call /me → 401 |
| Vue login page | Login form with captcha, CSRF auto-sent by Axios |

---

## Files to Create/Modify

### Rust Backend

| File | Content |
|---|---|
| `crates/auth/src/lib.rs` | Module exports |
| `crates/auth/src/jwt.rs` | `sign_jwt()`, `decode_jwt()`, `Claims` struct |
| `crates/auth/src/csrf.rs` | `generate_csrf_secret()`, `derive_csrf_token()`, `verify_csrf_token()` |
| `crates/auth/src/captcha.rs` | `generate_captcha()`, `store_captcha()`, `verify_captcha()`, SVG renderer |
| `crates/auth/src/session.rs` | `create_session()`, `get_session()`, `delete_session()`, `SessionData` struct |
| `crates/auth/src/password.rs` | `hash_password()`, `verify_password()` (argon2id + bcrypt compat) |
| `crates/auth/src/toko_token.rs` | `verify_toko_token()` (Sanctum SHA256 compat) |
| `crates/redis_store/src/session.rs` | Redis session CRUD |
| `crates/redis_store/src/captcha.rs` | Redis captcha CRUD |
| `crates/redis_store/src/rate_limit.rs` | Sliding window rate limiter |
| `apps/api/src/middleware/session_auth.rs` | JWT cookie → Redis session → AuthenticatedUser |
| `apps/api/src/middleware/csrf.rs` | X-XSRF-TOKEN validation |
| `apps/api/src/middleware/toko_auth.rs` | Bearer token → SHA256 → Toko lookup |
| `apps/api/src/middleware/rate_limit.rs` | Rate limit middleware |
| `apps/api/src/http/dashboard/auth.rs` | Login/logout/me/captcha handlers |

### Vue Frontend

| File | Content |
|---|---|
| `apps/web/src/lib/axios.ts` | Axios instance with `withCredentials`, XSRF config |
| `apps/web/src/stores/auth.ts` | Pinia store: login(), logout(), fetchUser(), user ref |
| `apps/web/src/router/index.ts` | Route guards: redirect to login if unauthenticated |
| `apps/web/src/pages/auth/LoginPage.vue` | Login form with captcha image + refresh |
| `apps/web/src/pages/auth/RegisterPage.vue` | Register form (if needed) |
| `apps/web/src/layouts/AuthLayout.vue` | Centered layout for auth pages |
