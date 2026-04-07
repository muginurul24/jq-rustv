# API Contract Freeze: justqiuv2 Rewrite

## Tujuan Dokumen

Membekukan semua kontrak API publik. Perubahan hanya dengan approval eksplisit + versioning.

---

## 1. Route Publik `/api/v1` — Auth & Conventions

- Auth: `Authorization: Bearer {token}` — principal = **Toko** (bukan User)
- Token format: Sanctum `{id}|{plaintext}` — backward compatible
- Success: `{ "success": true, ... }`
- Failure: `{ "success": false, "message": "..." }`

---

## 2. NexusGGR Bridge Routes

### `POST /api/v1/user/create`
- Request: `{ "username": string }`
- Response: `{ "success": true, "username": "playername" }`

### `GET /api/v1/providers`
- Response: `{ "success": true, "providers": [{ "code", "name", "status" }] }`
- Cached 1 day

### `POST /api/v1/games`
- Request: `{ "provider_code": string }`
- Response: `{ "success": true, "provider_code", "games": [{ "id", "game_code", "game_name", "banner", "status" }] }`
- Cached 1 day per provider

### `POST /api/v1/games/v2`
- Request: `{ "provider_code": string }`
- Response: `{ "success": true, "provider_code", "games": [{ "id", "game_code", "game_name": {localized} }] }`

### `POST /api/v1/game/launch`
- Request: `{ "username", "provider_code", "game_code"|null, "lang" }`
- Response: `{ "success": true, "launch_url": "https://..." }`

### `POST /api/v1/money/info`
- Request: `{ "username": string|null, "all_users": bool|null }`
- Response (user): `{ "success": true, "agent": { "code", "balance" }, "user": { "username", "balance" } }`
- Response (all): adds `"user_list": [{ "username", "balance" }]`
- `agent.balance` = toko's nexusggr. `agent.code` = toko.name

### `POST /api/v1/user/deposit`
- Request: `{ "username", "amount": int, "agent_sign": string|null }`
- Response: `{ "success": true, "agent": { "code", "balance" }, "user": { "username", "balance" } }`
- Side effect: nexusggr -= amount, Transaction(nexusggr, deposit, success)

### `POST /api/v1/user/withdraw`
- Request: `{ "username", "amount": int, "agent_sign": string|null }`
- Response: same shape as deposit
- Side effect: nexusggr += amount, Transaction(nexusggr, withdrawal, success)

### `POST /api/v1/user/withdraw-reset`
- Request: `{ "username": string|null, "all_users": bool|null }`
- Response (single): `{ "success": true, "agent": {...}, "user": { "username", "withdraw_amount", "balance" } }`
- Response (all): adds `"user_list": [{ "username", "withdraw_amount", "balance" }]`

### `POST /api/v1/transfer/status`
- Request: `{ "username", "agent_sign" }`
- Response: `{ "success": true, "amount", "type", "agent": { "code", "balance" }, "user": { "username", "balance" } }`

### `GET /api/v1/call/players`
- Response: `{ "success": true, "data": [{ "username", "provider_code", "game_code", "bet", "balance", "total_debit", "total_credit", "target_rtp", "real_rtp" }] }`
- Filtered to accessible players only

### `POST /api/v1/call/list`
- Request: `{ "provider_code", "game_code" }`
- Response: `{ "success": true, "calls": [{ "rtp", "call_type" }] }`

### `POST /api/v1/call/apply`
- Request: `{ "provider_code", "game_code", "username", "call_rtp": int, "call_type": int }`
- Response: `{ "success": true, "called_money" }`

### `POST /api/v1/call/history`
- Request: `{ "offset": int|null, "limit": int|null }`
- Response: `{ "success": true, "data": [{ "id", "username", "provider_code", "game_code", "bet", "user_prev", "user_after", "agent_prev", "agent_after", "expect", "missed", "real", "rtp", "type", "status", "created_at", "updated_at" }] }`

### `POST /api/v1/call/cancel`
- Request: `{ "call_id": int }`
- Response: `{ "success": true, "canceled_money" }`

### `POST /api/v1/control/rtp`
- Request: `{ "provider_code", "username", "rtp": float }`
- Response: `{ "success": true, "changed_rtp" }`

### `POST /api/v1/control/users-rtp`
- Request: `{ "user_codes": [string], "rtp": float }`
- Response: `{ "success": true, "changed_rtp" }`
- `user_codes` = local usernames, resolved to ext_usernames internally

---

## 3. QRIS Bridge Routes

### `POST /api/v1/merchant-active`
- Response: `{ "success": true, "store": { "name", "callback_url", "token" }, "balance": { "nexusggr", "pending", "settle" } }`

### `POST /api/v1/generate`
- Request: `{ "username", "amount": int, "expire": int|null, "custom_ref": string|null }`
- Response: `{ "success": true, "data": "QR_STRING", "trx_id": "..." }`
- Side effect: Transaction(qris, deposit, pending, code=trx_id)

### `POST /api/v1/check-status`
- Request: `{ "trx_id": string }`
- Response: `{ "success": true, "trx_id", "status": "pending|success|failed|expired" }`
- Scoped to authenticated toko

### `GET /api/v1/balance`
- Response: `{ "success": true, "pending_balance", "settle_balance", "nexusggr_balance" }`

---

## 4. Webhook Inbound

### `POST /api/webhook/qris`
- Inbound: `{ amount, terminal_id, merchant_id, trx_id, rrn?, custom_ref?, vendor?, status, created_at?, finish_at? }`
- Response: `{ "status": true, "message": "OK" }`
- Idempotency: `trx_id`
- Processing: strip merchant_id → find pending tx → branch by purpose → update balance/income → callback toko

### `POST /api/webhook/disbursement`
- Inbound: `{ amount, partner_ref_no, status, transaction_date?, merchant_id }`
- Response: `{ "status": true, "message": "OK" }`
- Idempotency: `partner_ref_no`
- Processing: strip merchant_id → find pending withdrawal → success: income += fee / failed: refund settle → callback toko

---

## 5. Callback Outbound ke Toko

### QRIS Event
```
POST {callback_url}  |  X-Bridge-Event: qris  |  X-Bridge-Reference: {trx_id}
Payload: { amount, terminal_id, trx_id, rrn, custom_ref, vendor, status, created_at, finish_at }
```

### Disbursement Event
```
POST {callback_url}  |  X-Bridge-Event: disbursement  |  X-Bridge-Reference: {partner_ref_no}
Payload: { amount, partner_ref_no, status, transaction_date }
```

Delivery: timeout 10s, inline retry [250ms, 750ms], job retry 4x backoff [10, 30, 60]s

---

## 6. Field Blacklist — DILARANG Bocor

| Field | Reason |
|---|---|
| `merchant_id` | QRIS upstream secret |
| `QRIS_MERCHANT_UUID` / `uuid` | Global merchant UUID |
| `agent_code`, `agent_token` | NexusGGR credentials |
| `ext_username` | Internal upstream identity — toko hanya lihat `username` |
| `JWT_SECRET`, `session_jwt`, `csrf_secret` | Auth internals |
| Raw upstream error/response | Strip fields not in whitelist |
| `password`, `password_hash`, `remember_token` | User secrets |
| Internal `note` fields (`purpose`, `inquiry_id`, `platform_fee`, `fee`, `bank_id`, `qris_data`) | Internal metadata — jangan forward ke callback |

Rules:
1. `merchant_id` → `unset` sebelum processing AND sebelum callback
2. Upstream errors → generic message: `"Failed to {action} on upstream platform"`
3. `ext_username` boleh di DB (`Transaction.external_player`) tapi jangan di response
4. Callback payload = whitelisted fields only (section 5)

---

## 7. Versioning Rules

1. Semua kontrak = **v1**, berlaku sejak rewrite
2. Breaking change (remove/rename field) → approval + `/api/v2/...`
3. Adding optional fields → allowed tanpa version bump
