# IBKR Client Portal Web API — Complete Reference

> Generated from official IBKR docs, ibkrguides.com release notes, ibind wiki, and API spec.
> Use this before touching any code so no re-fetching is needed.

---

## 1. Architecture Overview

IBKR offers multiple API products. We use the **Client Portal Web API (CPAPI v1)** because it is REST + WebSocket and does not require the TWS desktop application.

```
Trading Bot
    │
    │ WebSocket  /  HTTP REST
    ▼
IBKR Client Portal Gateway  (localhost:5000 — Java process)
    │
    └──▶ IBKR Servers
```

Two auth modes:

| Mode | When to use | Base URL |
|------|-------------|----------|
| **Gateway** (recommended) | Self-hosted bot, server | `https://localhost:5000` |
| **OAuth 1.0a** | Third-party apps, headless prod | `https://api.ibkr.com` |

For this integration we target the **Gateway mode** first.  
OAuth can be layered on top later (same endpoints, different base URL + auth header).

---

## 2. Authentication

### 2.1 Gateway Mode

1. Download and run [Client Portal Gateway](https://www.interactivebrokers.com/en/trading/ibkr-api.php) (Java).
2. Browse to `https://localhost:5000` and log in with IBKR credentials.
3. Gateway maintains the session; our code talks to `localhost:5000`.
4. Session expires daily → must re-authenticate via browser OR call `POST /iserver/reauthenticate`.

**Required startup call:**
```http
POST /v1/api/iserver/auth/ssodh/init?compete=true&publish=true
```
This initialises the brokerage session (trading access), not just the portfolio read session.

**Verify auth status:**
```http
POST /v1/api/iserver/auth/status
```
Response:
```json
{ "authenticated": true, "competing": false, "connected": true, "message": "...", "MAC": "..." }
```

### 2.2 Session Keepalive

**HTTP tickle** — must be called every 60 seconds to prevent session expiry:
```http
POST /v1/api/tickle
```
Response: `{ "session": "...", "ssoExpires": 3600, "collSession": "..." }`

**WebSocket heartbeat** — send every **10 seconds** to keep WS alive:
```
ech+hb
```

### 2.3 OAuth 1.0a (Advanced)

Three-step flow using RSA + Diffie-Hellman:
1. `POST /v1/api/oauth/live_session_token` — RSA-SHA256 signed, DH challenge/response → returns `live_session_token`
2. `GET /v1/api/portfolio/accounts` — HMAC-SHA256 signed with LST
3. `POST /v1/api/iserver/auth/ssodh/init` — HMAC-SHA256 signed

OAuth header format:
```
Authorization: OAuth realm="test_realm",
  oauth_consumer_key="...",
  oauth_nonce="...",
  oauth_timestamp="...",
  oauth_token="...",
  oauth_signature_method="HMAC-SHA256",
  oauth_signature="..."
```

Env vars needed for OAuth:
```
IBKR_CONSUMER_KEY=...
IBKR_ACCESS_TOKEN=...
IBKR_DH_PRIME=...
IBKR_PRIVATE_SIGNING_KEY_PEM=...   # RSA key for LST request
IBKR_PRIVATE_ENCRYPTION_KEY_PEM=... # DH encryption key
```

---

## 3. Base URLs & WebSocket Endpoint

| Environment | REST Base | WebSocket |
|-------------|-----------|-----------|
| Gateway (localhost) | `https://localhost:5000/v1/api` | `wss://localhost:5000/v1/api/ws` |
| OAuth (direct) | `https://api.ibkr.com/v1/api` | `wss://api.ibkr.com/v1/api/ws` |

> **TLS note**: Gateway uses a self-signed certificate.  
> Accept invalid certs for `localhost:5000` connections.  
> For `api.ibkr.com`, use normal TLS validation.

---

## 4. Rate Limits

| Scope | Limit |
|-------|-------|
| Global (OAuth) | 50 req/sec per user |
| Gateway | 10 req/sec |
| Market data snapshot | 10 req/sec |
| Scanner | 1 req/15 min |
| WebSocket concurrent market data | ~5 instruments |
| WS `smd` stream duration | **10 minutes** — must resubscribe |
| Penalty box | 10 min ban; repeat = permanent block |

---

## 5. WebSocket Protocol

### 5.1 Message Format

All messages follow the pattern:
```
TOPIC+ARGUMENT+{JSON}
```
- First char: `s` = subscribe, `u` = unsubscribe
- Heartbeat: `ech+hb`

### 5.2 Topics

| Subscribe | Unsubscribe | Purpose |
|-----------|-------------|---------|
| `smd+CONID+{"fields":[...]}` | `umd+CONID+{}` | Top-of-book market data ticks |
| `smh+CONID+{"period":"1d","bar":"5min"}` | `umh+CONID+{}` | Historical bars (streaming update) |
| `sor+{}` | `uor+{}` | Live order updates |
| `spl+{}` | `upl+{}` | P&L (profit/loss) updates |
| `sbd+ACCOUNT_ID+CONID` | `ubd+ACCOUNT_ID` | Book Trader L2 depth |
| `ech+hb` | — | Heartbeat (every 10 sec) |

### 5.3 Market Data Subscribe (`smd`)

Request:
```
smd+8314+{"fields":["31","84","85","86","87","88","7059","7068"]}
```

Response (newline-terminated JSON):
```json
{
  "topic": "smd",
  "server_id": "q0",
  "conid": 8314,
  "conidEx": "8314",
  "_updated": 1715000000000,
  "31": "1.08432",
  "84": "1.08430",
  "85": "1.08435",
  "86": "125000",
  "87": "1.08100",
  "88": "1.08400"
}
```

**Critical**: `smd` subscriptions expire after **10 minutes**. Resubscribe before expiry.

### 5.4 Market Data Field IDs

| Field ID | Name | Notes |
|----------|------|-------|
| `31` | Last price | |
| `70` | High (52-week) | |
| `71` | Low (52-week) | |
| `82` | Change | |
| `83` | Change % | |
| `84` | Bid | |
| `85` | Ask | |
| `86` | Volume | |
| `87` | Close / Prior close | |
| `88` | Open | |
| `7059` | Bid size | |
| `7068` | Ask size | |
| `7295` | Market cap | |
| `7296` | Company name | |
| `6509` | Market value | |

For ticks we primarily use: `["31","84","85","86","87","88","7059","7068"]`

### 5.5 Historical Bars Subscribe (`smh`)

Request:
```
smh+8314+{"period":"1d","bar":"5min","source":"t","format":"%o/%h/%l/%c/%v/%t"}
```

Parameters:
- `period`: `1d`, `1w`, `1m`, `3m`, `6m`, `1y`, `2y`, `5y`
- `bar`: `5min`, `15min`, `30min`, `1h`, `2h`, `4h`, `1d`, `1w`
- `source`: `t` (trades), `b` (bid), `a` (ask), `m` (midpoint)
- `format`: `%o/%h/%l/%c/%v/%t` (open/high/low/close/volume/time)
- `outsideRth`: `true`/`false`

Response:
```json
{
  "topic": "smh",
  "conid": 8314,
  "data": [
    { "t": 1715000000000, "o": 1.0843, "h": 1.0850, "l": 1.0840, "c": 1.0847, "v": 5000 }
  ],
  "points": 288,
  "mktDataDelay": 0
}
```

### 5.6 Order Updates (`sor`)

Request: `sor+{}`

Response (one object per order event):
```json
{
  "topic": "sor",
  "args": [
    {
      "orderId": 123456789,
      "account": "U12345678",
      "conid": 8314,
      "ticker": "IBM",
      "side": "BUY",
      "orderType": "MKT",
      "status": "Filled",
      "avgPrice": "100.50",
      "remainingQuantity": 0,
      "totalQuantity": 100,
      "filledQuantity": 100
    }
  ]
}
```

Order status values: `PreSubmitted`, `Submitted`, `Filled`, `Cancelled`, `Inactive`, `PendingSubmit`

### 5.7 P&L Updates (`spl`)

Request: `spl+{}`

Response:
```json
{
  "topic": "spl",
  "args": {
    "U12345678": {
      "rowType": 1,
      "dpl": -15.23,
      "nl": 101234.56,
      "upl": 234.78,
      "el": 101234.56,
      "mv": 50000.00
    }
  }
}
```

Fields: `dpl`=daily P&L, `nl`=net liquidation, `upl`=unrealized P&L, `el`=equity+loan, `mv`=market value

### 5.8 Heartbeat

Send every 10 seconds:
```
ech+hb
```
Response: `{"topic":"ech","hb":1715000000000}`

---

## 6. REST Endpoints

All paths relative to base URL (`/v1/api`).

### 6.1 Session

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/iserver/auth/status` | Check auth state |
| `POST` | `/iserver/auth/ssodh/init?compete=true&publish=true` | Init brokerage session |
| `POST` | `/iserver/reauthenticate` | Re-auth after session expire |
| `POST` | `/tickle` | Keep session alive (every 60s) |
| `GET`  | `/sso/validate` | Validate SSO session |
| `POST` | `/logout` | End session |

### 6.2 Accounts

| Method | Path | Purpose |
|--------|------|---------|
| `GET`  | `/iserver/accounts` | List trading accounts → `["U12345678"]` |
| `GET`  | `/portfolio/accounts` | Full account info |
| `GET`  | `/portfolio/subaccounts` | Sub-accounts (FA/IBroker) |
| `GET`  | `/portfolio/{acctId}/summary` | Equity, margin, cash |
| `GET`  | `/portfolio/{acctId}/ledger` | Cash balances per currency |

Account summary response (key fields):
```json
{
  "totalcashvalue": { "amount": 10000.00, "currency": "USD" },
  "netliquidation": { "amount": 50000.00 },
  "availablefunds":  { "amount": 9000.00 },
  "buyingpower":     { "amount": 36000.00 }
}
```

### 6.3 Contract / Symbol Lookup

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/iserver/secdef/search` | Search by symbol → get conid |
| `GET`  | `/iserver/contract/{conid}/info` | Contract details |
| `POST` | `/trsrv/secdef` | Batch security definitions |
| `GET`  | `/iserver/secdef/strikes` | Options strikes |
| `GET`  | `/trsrv/futures` | Futures contracts |
| `GET`  | `/iserver/currency/pairs?currency=EUR` | FX pairs |

**Symbol search request:**
```http
POST /v1/api/iserver/secdef/search
Content-Type: application/json

{"symbol": "EURUSD", "name": false, "secType": "CASH"}
```

secType values: `STK` (stocks), `CASH` (forex), `FUT` (futures), `OPT` (options), `BOND`, `CFD`

Response:
```json
[{
  "conid": 12087792,
  "companyHeader": "EUR.USD",
  "companyName": "EURO FX",
  "symbol": "EUR",
  "description": "EUR.USD",
  "restricted": null,
  "fop": null,
  "opt": null,
  "war": null,
  "sections": [{ "secType": "CASH", "months": null, "symbol": "EUR", "exchange": "IDEALPRO" }]
}]
```

> For forex pairs, `conid` is the EUR component; the pair is implied by the CASH secType.

**Known forex conids (IDEALPRO):**
| Pair | ConID |
|------|-------|
| EUR/USD | 12087792 |
| GBP/USD | 12087797 |
| USD/JPY | 15016062 |
| USD/CHF | 12087820 |
| AUD/USD | 12087791 |
| USD/CAD | 12087821 |

These are stable but **always verify via `/iserver/secdef/search`** before use.

### 6.4 Market Data

| Method | Path | Purpose |
|--------|------|---------|
| `GET`  | `/iserver/marketdata/snapshot` | Snapshot for 1+ conids |
| `GET`  | `/iserver/marketdata/history` | Historical OHLCV bars |
| `GET`  | `/iserver/marketdata/{conid}/unsubscribe` | Cancel snapshot stream |

**Snapshot request:**
```http
GET /v1/api/iserver/marketdata/snapshot?conids=12087792&fields=31,84,85,86,87,88
```

Note: First call returns cached/stale data; second call returns live data. Always call twice.

Response:
```json
[{
  "conid": 12087792,
  "31": "1.08432",
  "84": "1.08430",
  "85": "1.08435",
  "86": "125000",
  "87": "1.08100"
}]
```

**Historical bars request:**
```http
GET /v1/api/iserver/marketdata/history?conid=12087792&exchange=IDEALPRO&period=1m&bar=1h&outsideRth=false
```

Parameters:
| Param | Values | Notes |
|-------|--------|-------|
| `conid` | integer | From secdef lookup |
| `exchange` | `IDEALPRO`, `SMART`, etc. | IDEALPRO for forex |
| `period` | `1d`,`1w`,`1m`,`3m`,`6m`,`1y`,`2y`,`3y`,`5y` | How far back |
| `bar` | `1min`,`5min`,`10min`,`15min`,`30min`,`1h`,`2h`,`4h`,`8h`,`1d`,`1w`,`1m` | Bar size |
| `outsideRth` | `true`/`false` | Include extended hours |
| `startTime` | `YYYYMMDD-HH:MM:SS` | Optional start time |

Response:
```json
{
  "startTime": "20240101-09:30:00",
  "startTimeVal": 1704067800000,
  "endTime": "20240201-16:00:00",
  "endTimeVal": 1706832000000,
  "data": [
    { "t": 1704067800000, "o": 1.0843, "h": 1.0850, "l": 1.0838, "c": 1.0847, "v": 12500 }
  ],
  "points": 500,
  "mktDataDelay": 0,
  "symbol": "EUR.USD",
  "text": "EUR.USD"
}
```

**Period → bar size mapping** (our `period` in minutes):
| Period (min) | IBKR bar | IBKR period for 500 bars |
|--------------|----------|--------------------------|
| 1 | `1min` | `1d` |
| 5 | `5min` | `3d` |
| 15 | `15min` | `1w` |
| 30 | `30min` | `2w` |
| 60 | `1h` | `1m` |
| 240 | `4h` | `3m` |
| 1440 | `1d` | `2y` |
| 10080 | `1w` | `5y` |

### 6.5 Orders

| Method | Path | Purpose |
|--------|------|---------|
| `GET`  | `/iserver/account/orders` | All live orders |
| `GET`  | `/iserver/account/orders/{orderId}` | Single order status |
| `POST` | `/iserver/account/{acctId}/orders` | Place order(s) |
| `POST` | `/iserver/account/{acctId}/order/{orderId}` | Modify order |
| `DELETE`| `/iserver/account/{acctId}/order/{orderId}` | Cancel order |
| `POST` | `/iserver/reply/{replyId}` | Confirm order (handle warnings) |
| `GET`  | `/iserver/account/trades` | Recent fills |

**Place order request:**
```http
POST /v1/api/iserver/account/U12345678/orders
Content-Type: application/json

{
  "orders": [{
    "conid": 12087792,
    "secType": "12087792@CASH",
    "orderType": "MKT",
    "side": "BUY",
    "quantity": 20000,
    "tif": "GTC",
    "outsideRth": false,
    "price": null
  }]
}
```

Field values:
- `orderType`: `MKT`, `LMT`, `STP`, `STP LMT`, `MIDPRICE`, `TRAIL`, `TRAILLMT`, `PASSV`, `LIMIT_ON_CLOSE`
- `side`: `BUY`, `SELL`
- `tif` (time-in-force): `GTC`, `DAY`, `IOC`, `FOK`, `OPG`, `DTC`
- `secType`: format is `CONID@SECTYPE` (e.g., `12087792@CASH`)

**Limit order:**
```json
{
  "orders": [{
    "conid": 12087792,
    "secType": "12087792@CASH",
    "orderType": "LMT",
    "side": "BUY",
    "quantity": 20000,
    "price": 1.08200,
    "tif": "GTC"
  }]
}
```

**Stop order:**
```json
{
  "orders": [{
    "conid": 12087792,
    "secType": "12087792@CASH",
    "orderType": "STP",
    "side": "SELL",
    "quantity": 20000,
    "price": 1.07500,
    "tif": "GTC"
  }]
}
```

**Place order response (success, HTTP 200):**
```json
[{
  "order_id": "123456789",
  "local_order_id": "...",
  "order_status": "Submitted"
}]
```

**Place order response (requires confirmation, HTTP 200):**
```json
[{
  "id": "1a2b3c4d",
  "message": ["You are placing a forex order. ...", "Are you sure?"],
  "isSuppressed": false,
  "messageIds": ["o163"]
}]
```
→ Must call `POST /iserver/reply/{id}` with `{"confirmed": true}` to complete the order.

**Cancel order response (HTTP 200):**
```json
{ "msg": "Request was submitted", "order_id": 123456789, "conid": 12087792 }
```

### 6.6 Positions

| Method | Path | Purpose |
|--------|------|---------|
| `GET`  | `/portfolio/{acctId}/positions/0` | Open positions (page 0) |
| `GET`  | `/portfolio/{acctId}/positions/{pageId}` | Paginated positions |
| `GET`  | `/portfolio/{acctId}/position/{conid}` | Position for single conid |

Response:
```json
[{
  "acctId": "U12345678",
  "conid": 12087792,
  "contractDesc": "EUR.USD",
  "position": 20000.0,
  "mktPrice": 1.08432,
  "mktValue": 20000.0,
  "currency": "USD",
  "avgCost": 1.08200,
  "avgPrice": 1.08200,
  "realizedPnl": 0.0,
  "unrealizedPnl": 46.40,
  "exchs": null,
  "expiry": null,
  "putOrCall": null,
  "multiplier": 0.0,
  "strike": 0.0,
  "exerciseStyle": null,
  "conExchMap": [],
  "assetClass": "CASH",
  "undConid": 0
}]
```

---

## 7. Error Handling

| HTTP Code | Meaning |
|-----------|---------|
| 200 | Success (also used for order confirmation prompts) |
| 400 | Bad request / validation error |
| 401 | Not authenticated |
| 429 | Rate limit exceeded → back off and retry |
| 500 | Server error (also previously used for order rejection — now 200) |

Error response format:
```json
{ "error": "description of error" }
```

---

## 8. Key Gotchas & Known Issues

1. **Market data snapshot returns stale first call** — always call twice; first call "subscribes" internally, second returns live data.
2. **`smd` WebSocket stream expires after 10 minutes** — must track subscription time and resubscribe before expiry.
3. **Order confirmation flow** — placing an order may return a `message`/`id` instead of `order_id`. Must call `POST /iserver/reply/{id}` with `{"confirmed": true}`.
4. **Gateway self-signed cert** — `wss://localhost:5000` uses a self-signed TLS cert. Must disable cert verification for localhost connections.
5. **Brokerage session init required** — after Gateway login, must call `POST /iserver/auth/ssodh/init?compete=true&publish=true` before trading.
6. **Session expires daily** — Gateway session must be renewed daily (browser login or `POST /iserver/reauthenticate`).
7. **Conid lookup required** — all market data and order operations use integer contract IDs, not symbol strings. Must call `/iserver/secdef/search` first (or use a cache for known pairs).
8. **~5 concurrent WS market data streams** — IBKR enforces a low limit; do not subscribe to many instruments at once.
9. **Tickle must be called every 60s** — failure to tickle kills the session; do this in a background task.
10. **`secType` in order body** — use `CONID@SECTYPE` format e.g., `"12087792@CASH"` not just `"CASH"`.
11. **Forex quantity** — in lots (not units). 20000 = 20k units of base currency.

---

## 9. Required Environment Variables

```env
# Gateway URL (localhost for Gateway mode, api.ibkr.com for OAuth)
IBKR_GATEWAY_URL=https://localhost:5000

# Account ID (format: U12345678)
IBKR_ACCOUNT_ID=U12345678

# Accept self-signed cert (true for localhost:5000 gateway)
IBKR_ACCEPT_INVALID_CERT=true

# Environment: paper or live
IBKR_ENVIRONMENT=paper

# --- OAuth mode only (not needed for Gateway mode) ---
# IBKR_CONSUMER_KEY=...
# IBKR_ACCESS_TOKEN=...
# IBKR_DH_PRIME=...
# IBKR_PRIVATE_SIGNING_KEY_PEM=path/to/signing.pem
# IBKR_PRIVATE_ENCRYPTION_KEY_PEM=path/to/encryption.pem
```

---

## 10. Startup Sequence

```
1. Start Gateway → browse https://localhost:5000 → log in
2. POST /iserver/auth/ssodh/init?compete=true&publish=true
3. Verify: POST /iserver/auth/status → { "authenticated": true }
4. GET /iserver/accounts → get account_id
5. Start background tickle task: POST /tickle every 60s
6. Connect WebSocket: wss://localhost:5000/v1/api/ws
7. Start WS heartbeat: send "ech+hb" every 10s
8. For each symbol: POST /iserver/secdef/search → get conid
9. Send: smd+CONID+{"fields":["31","84","85","86","87","88"]}
10. Re-subscribe smd every 9 minutes (before 10-min expiry)
```

---

## 11. BrokerStream Trait → IBKR Mapping

| Trait method | IBKR implementation |
|---|---|
| `new()` | Read env vars, build reqwest client |
| `login()` | `POST /iserver/auth/ssodh/init` + verify auth status |
| `keepalive_ping()` | `POST /tickle` (HTTP) + `ech+hb` (WS) |
| `disconnect()` | `POST /logout` |
| `get_instrument_data()` | `GET /iserver/marketdata/history` |
| `get_historic_data()` | `GET /iserver/marketdata/history` with from/to |
| `get_instrument_tick()` | `GET /iserver/marketdata/snapshot` (called twice) |
| `get_ask_bid()` | `GET /iserver/marketdata/snapshot` fields 84+85 |
| `get_instrument_swap()` | Not available → return stub |
| `get_market_hours()` | `GET /trsrv/secdef` or stub with always-open |
| `is_market_open()` | `GET /iserver/marketdata/snapshot` field `6509` tradeable |
| `is_market_available()` | `true` unless auth check fails |
| `open_trade()` | `POST /iserver/account/{id}/orders` MKT order + handle reply |
| `close_trade()` | `POST /iserver/account/{id}/orders` SELL/BUY opposite + handle reply |
| `open_order()` | `POST /iserver/account/{id}/orders` LMT/STP order + handle reply |
| `close_order()` | `DELETE /iserver/account/{id}/order/{orderId}` |
| `get_active_positions()` | `GET /portfolio/{id}/positions/0` |
| `get_transaction_details()` | `GET /iserver/account/trades` |
| `subscribe_stream()` | WS connect → conid lookup → `smd+CONID+{...}` → spawn task |
