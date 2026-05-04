# SPEC: Broker-Agnostic Refactor + Darwinex Integration

**Scope:** `rs_algo_shared` + `rs-algo-bot` (workspace: `rs_algo_ws_server`, `rs_algo_bot`)  
**Goal:** Replace XTB with Darwinex, make broker layer fully agnostic, update to Rust 2024 edition, modernize dependencies.

---

## Table of Contents

1. [Context & Current State](#1-context--current-state)
2. [Darwinex API Overview](#2-darwinex-api-overview)
3. [Architecture: Before vs After](#3-architecture-before-vs-after)
4. [Step 1 — Rust 2024 Edition + Dependency Update](#step-1--rust-2024-edition--dependency-update)
5. [Step 2 — Clean broker/models.rs](#step-2--clean-brokermodelsrs)
6. [Step 3 — Extract BrokerStream Trait](#step-3--extract-brokerstream-trait)
7. [Step 4 — Implement Darwinex Broker](#step-4--implement-darwinex-broker)
8. [Step 5 — Broker Factory](#step-5--broker-factory)
9. [Step 6 — Update WebSocket Layer (single connection)](#step-6--update-websocket-layer-single-connection)
10. [Step 7 — Update rs_algo_ws_server](#step-7--update-rs_algo_ws_server)
11. [Step 8 — Deprecate XTB](#step-8--deprecate-xtb)
12. [Environment Variables Reference](#environment-variables-reference)
13. [Files Affected Summary](#files-affected-summary)

---

## 1. Context & Current State

### Problem Summary

| Issue | Location |
|---|---|
| `BrokerStream` trait and `Xtb` impl are in the same file | `rs_algo_shared/src/broker/xtb_stream.rs` |
| Trait exposes tungstenite internals via `get_stream()` | `rs_algo_shared/src/broker/xtb_stream.rs:139` |
| XTB uses TWO WebSocket connections (socket + stream) | `rs_algo_shared/src/broker/xtb_stream.rs:148-156` |
| `server.rs` hardcodes `Xtb::new().await` | `rs_algo_ws_server/src/server.rs:69` |
| `stream.rs` hardcodes `Xtb` struct type | `rs_algo_ws_server/src/handlers/stream.rs:19,25` |
| `message.rs` `InitSession` reads raw JSON fields instead of typed struct | `rs_algo_ws_server/src/message.rs:93-96` |
| `broker/models.rs` mixes generic models with XTB-specific structs | `rs_algo_shared/src/broker/models.rs` |
| `async-trait` crate used — replaced by native async traits in Rust 1.75+ | All files using `#[async_trait]` |
| Dependencies 2-3 years out of date | Both `Cargo.toml` files |
| Lots of `camelCase` field names in structs (XTB protocol artifact) | `broker/models.rs` |
| Dead code: `xtb.rs` basic `Broker` trait never used by server | `rs_algo_shared/src/broker/xtb.rs` |

### Current Module Structure

```
rs_algo_shared/src/broker/
├── mod.rs           exports Broker, BrokerStream, models
├── models.rs        DOHLC types + XTB-specific command structs (mixed)
├── xtb.rs           basic Broker trait (unused by server)
└── xtb_stream.rs    BrokerStream trait + Xtb struct impl (mixed together)

rs-algo-bot/rs_algo_ws_server/src/
├── server.rs        hardcodes: use xtb_stream::*; Xtb::new()
├── message.rs       raw JSON field access in InitSession
└── handlers/
    └── stream.rs    hardcodes: Xtb type; creates 2nd broker connection
```

---

## 2. Darwinex API Overview

> **Reference:** https://darwinex.github.io/darwin-api-tutorials/ | https://help.darwinex.com/api-walkthrough

### API Surface

Darwinex exposes five APIs. Only two are relevant for this bot:

| API | Type | Purpose |
|---|---|---|
| **Product Websockets API** | WebSocket | Real-time quote streaming for DARWIN products |
| **Investor Account Info API** | REST | Account info, open/closed positions, orders |
| Darwin Trading API | REST | Buy/sell DARWINs (investment operations) |
| Darwin Info API | REST | Historical DARWIN quotes, scores |
| Product Quotes API | REST | Snapshot quotes |

### ⚠️ Important: Darwinex vs XTB

| Capability | XTB | Darwinex |
|---|---|---|
| Direct Forex/CFD trading via API | Yes (WebSocket) | No (MT4/MT5 only) |
| DARWIN product trading | No | Yes (REST) |
| Real-time quote streaming | WebSocket (stream) | WebSocket (single) |
| Historical OHLC data | WebSocket (socket) | REST |
| Active positions | WebSocket (socket) | REST |
| Authentication | username + password WS command | Bearer token (OAuth2 or direct) |
| Connections needed | **Two** (socket + stream) | **One** (WebSocket + REST) |

Darwinex's broker platform (real-money forex trading) uses **MetaTrader 4/5** — there is no public WebSocket API for direct forex order placement. The trading operations in this bot will use Darwinex's REST API for account/position management and their WebSocket for quote streaming.

### WebSocket Connection

```
URL:      wss://api.darwinex.com/quotewebsocket/1.0.0
Auth:     HTTP header: Authorization: Bearer <access_token>
Protocol: JSON over WebSocket
```

**Subscribe message:**
```json
{
  "op": "subscribe",
  "productNames": ["EURUSD", "GBPUSD"]
}
```

**Incoming quote message:**
```json
{
  "productName": "EURUSD",
  "quote": 1.08432,
  "timestamp": 1715000000000
}
```

**Keepalive:** Standard WebSocket ping/pong frames.

### Authentication

```
POST https://api.darwinex.com/token
Content-Type: application/x-www-form-urlencoded

grant_type=password&username=<user>&password=<pass>&scope=openid
```

Response:
```json
{
  "access_token": "...",
  "refresh_token": "...",
  "expires_in": 3600,
  "token_type": "Bearer"
}
```

Direct tokens (no expiry) are available from the Darwinex API Store for the data APIs.

---

## 3. Architecture: Before vs After

### Before (XTB, dual connection)

```
[Bot]
  │ WebSocket (commands/responses)
[rs_algo_ws_server]
  │
  ├─── Xtb { socket: WebSocket }       ← command channel (login, data, trades)
  └─── Xtb { stream: WebSocketClientStream }  ← stream channel (candles, ticks)
         (second Xtb instance created in stream.rs)
```

### After (broker-agnostic, single connection)

```
[Bot]
  │ WebSocket (commands/responses)
[rs_algo_ws_server]
  │
  └─── Box<dyn BrokerStream>           ← runtime-selected broker
         │
         ├── DarwinexBroker            ← single WS + REST client
         │     ws: WebSocket           (quotes stream)
         │     http: reqwest::Client   (data, trades, positions)
         │     stream_tx: Sender<Msg>  (internal channel → server reads)
         │
         └── (Xtb kept for migration, deprecated)
```

---

## Step 1 — Rust 2024 Edition + Dependency Update

### Files Affected
- `rs_algo_shared/Cargo.toml`
- `rs-algo-bot/rs_algo_ws_server/Cargo.toml`
- `rs-algo-bot/rs_algo_bot/Cargo.toml`
- `rs-algo-bot/Cargo.toml` (workspace)

### 1.1 rs_algo_shared/Cargo.toml

Change `edition = "2021"` → `edition = "2024"`.

**Dependency changes:**

| Crate | Current | Target | Notes |
|---|---|---|---|
| `async-trait` | `0.1.52` | **REMOVE** | Use native async traits (stable since Rust 1.75) |
| `tokio` | `1.19.1` | `1.44` | |
| `tokio-tungstenite` | `0.18.0` | `0.26` | Change feature: `native-tls` → `rustls-tls-native-roots` |
| `tungstenite` | `0.18.0` | `0.26` | Change feature: `native-tls` → `rustls-tls-native-roots` |
| `openssl` | `0.10.38` | **REMOVE** | Replaced by rustls |
| `serde` | `1.0.139` | `1.0.219` | |
| `serde_json` | `1.0.82` | `1.0.140` | |
| `chrono` | `0.4.19` | `0.4.40` | Remove `wasmbind`, `js-sys` features (not needed) |
| `reqwest` | `0.11.22` | `0.12` | Change to rustls: `features = ["json", "rustls-tls"]` |
| `bson` | `2.2.0` | `2.13` | |
| `thiserror` | `1.0.31` | `2.0` | |
| `anyhow` | `1.0.58` | `1.0.98` | |
| `env_logger` | `0.10.0` | `0.11` | |
| `futures-util` | `0.3.17` | `0.3.31` | |

**Feature flags — rename for broker agnosticism:**

```toml
[features]
default = []
chart = ["plotters"]
darwinex = ["tokio-tungstenite", "futures-util", "tokio", "reqwest"]
xtb = ["tungstenite", "tokio-tungstenite", "futures-util", "tokio"]
websocket = ["tungstenite", "tokio", "futures-util"]
```

Remove the generic `broker` feature — it implied XTB. Each broker has its own feature.

### 1.2 rs_algo_ws_server/Cargo.toml

Change `edition = "2021"` → `edition = "2024"`.

| Crate | Current | Target | Notes |
|---|---|---|---|
| `async-trait` | `0.1.73` | **REMOVE** | Native async traits |
| `tokio` | `1.35.1` | `1.44` | |
| `tokio-tungstenite` | `0.18.0` | `0.26` | |
| `tungstenite` | `0.18.0` | `0.26` | |
| `serde` | `1.0.188` | `1.0.219` | |
| `serde_json` | `1.0.105` | `1.0.140` | |
| `chrono` | `0.4.19` | `0.4.40` | |
| `futures-channel` | `0.3` | `0.3.31` | |
| `futures-util` | `0.3.28` | `0.3.31` | |
| `mongodb` | `2.2.2` | `3.x` | Breaking changes in client API |
| `bson` | `2.3.0` | `2.13` | |
| `thiserror` | `1.0.47` | `2.0` | |
| `anyhow` | `1.0.75` | `1.0.98` | |
| `env_logger` | `0.10.0` | `0.11` | |
| `rs_algo_shared` | git rev `29a6c5b` | local path (dev) / new rev (prod) | Change feature from `broker` to `darwinex` |

### 1.3 Rust 2024 Code Adjustments

**Remove `#[async_trait]` everywhere.** Replace:
```rust
// Before
#[async_trait::async_trait]
pub trait BrokerStream {
    async fn login(&mut self, ...) -> Result<...>;
}

// After (Rust 1.75+ native async traits)
pub trait BrokerStream {
    async fn login(&mut self, ...) -> Result<...>;
}
```

**`gen` is a reserved keyword in Rust 2024** — search all files for variables named `gen` and rename them (e.g., `generator`, `gen_val`).

**Stricter `unsafe` requirements** — any `unsafe` blocks need to be audited (search: `unsafe`).

---

## Step 2 — Clean broker/models.rs

### Files Affected
- `rs_algo_shared/src/broker/models.rs`
- `rs_algo_shared/src/broker/xtb_models.rs` ← **NEW**

### 2.1 What to Keep in models.rs (broker-agnostic)

Keep only types that have meaning regardless of broker:

```rust
// rs_algo_shared/src/broker/models.rs

pub type DOHLC = (DateTime<Local>, f64, f64, f64, f64, f64);
pub type VEC_DOHLC = Vec<DOHLC>;

// Generic order direction
pub enum TransactionCommand { BuyMarket, SellMarket, BuyLimit, SellLimit, BuyStop, SellStop, Balance, Credit }
pub enum TransactionAction  { Open, Pending, Close, Modify, Delete }
pub enum TransactionState   { Error, Pending, Accepted, Rejected }

// Generic trade metadata embedded in broker comments/tags
pub struct TransactionDetails  { pub id: usize, pub open_price: f64, pub close_price: f64, pub profit: f64 }
pub struct TransactionComments { pub strategy_name: String, pub index_in: usize, ... }
pub struct TransactionStatusResponse { pub order: u64, pub ask: f64, pub bid: f64, pub status: TransactionState, ... }
```

### 2.2 What to Move to xtb_models.rs (XTB-specific)

These are XTB protocol structs — they use `camelCase` field names mirroring XTB's JSON API. Moving them isolates protocol details from the shared layer:

```rust
// rs_algo_shared/src/broker/xtb_models.rs  (NEW FILE)

// XTB login protocol
pub struct LoginParams        { pub userId: String, pub password: String, pub appName: String }
pub struct LoginResponse      { pub status: bool, pub streamSessionId: String }

// XTB command wrappers
pub struct Command<T>         { pub command: String, pub arguments: T }
pub struct CommandStreaming    { pub command: String, pub streamSessionId: String }
pub struct CommandGetCandles   { pub command: String, pub streamSessionId: String, pub symbol: String }
pub struct CommandTickStreamParams { ... }
pub struct CommandTradeStatusParams { ... }
pub struct CommandAllSymbols  { pub command: String }
pub struct Ping               { pub command: String }
pub struct SymbolArg          { pub symbol: String }

// XTB instrument request structs
pub struct Instrument         { pub info: InstrumentCandles }
pub struct InstrumentCandles  { pub period: usize, pub start: i64, pub symbol: String }
pub struct HistoricInstrument { pub info: HistoricInstrumentCandles }
pub struct HistoricInstrumentCandles { pub period: usize, pub start: i64, pub end: i64, pub ticks: i64, pub symbol: String }

// XTB trade execution
pub struct TradeTransactionInfo { pub cmd: isize, pub customComment: String, ... }
pub struct TransactionInfo    { pub tradeTransInfo: TradeTransactionInfo }
pub struct TransactionStatus  { pub order: u64 }
pub struct GetTrades          { pub openedOnly: bool }
pub struct GetTradesHistory   { pub start: i64, pub end: i64 }

// XTB instrument tick (camelCase fields = XTB protocol)
pub struct SymbolInstrumentTick { pub symbol: String, pub ask: f64, pub bid: f64, pub spreadRaw: f64, ... }
```

**Also fix the `camelCase` field names in structs that ARE kept in models.rs** — use `#[serde(rename)]`:

```rust
// Before (XTB artifact)
pub struct SymbolInstrumentTick {
    pub spreadRaw: f64,
    pub contractSize: isize,
}

// After (snake_case + serde rename)
pub struct SymbolInstrumentTick {
    pub spread_raw: f64,
    #[serde(rename = "contractSize")]
    pub contract_size: isize,
}
```

### 2.3 Update mod.rs exports

```rust
// rs_algo_shared/src/broker/mod.rs (updated)
pub mod models;
pub mod xtb_models;     // XTB protocol structs
pub mod broker_trait;   // BrokerStream trait (Step 3)
pub mod darwinex;       // Darwinex impl (Step 4)
pub mod xtb;            // deprecated, kept for migration

pub use broker_trait::BrokerStream;
pub use models::*;
```

---

## Step 3 — Extract BrokerStream Trait

### Files Affected
- `rs_algo_shared/src/broker/broker_trait.rs` ← **NEW FILE** (extracted from `xtb_stream.rs`)
- `rs_algo_shared/src/broker/xtb_stream.rs` ← trait removed, only `Xtb` impl remains

### 3.1 Problem with Current Trait

The current `BrokerStream` trait (at `xtb_stream.rs:36-146`) is **not** broker-agnostic:

1. **`get_stream()` returns tungstenite internals** (line 139):
   ```rust
   async fn get_stream(&mut self) -> &mut SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;
   ```
   This hard-couples all brokers to tungstenite's split stream type. Darwinex (single connection) cannot satisfy this as written.

2. **`get_session_id()`** — XTB-specific concept (XTB stream session IDs). No other broker has this.

3. **`open_trade_real()` / `open_trade_test()`** — implementation details leaked into the interface. Only `open_trade()` belongs in the trait.

4. **`listen()`** — unused empty function in the XTB impl (never called). Remove it.

### 3.2 New Trait Design

The key architectural change: instead of exposing raw stream internals, the broker manages its stream internally and delivers parsed messages over a **tokio channel**.

```rust
// rs_algo_shared/src/broker/broker_trait.rs

pub trait BrokerStream: Send + Sync {
    // Lifecycle
    async fn new() -> Self where Self: Sized;
    async fn login(&mut self, username: &str, password: &str) -> Result<&mut Self>
        where Self: Sized;
    async fn disconnect(&mut self) -> Result<()>;
    async fn keepalive_ping(&mut self) -> Result<()>;

    // Market data
    async fn get_instrument_data(
        &mut self, symbol: &str, period: usize, start: i64,
    ) -> Result<ResponseBody<InstrumentData<VEC_DOHLC>>>;

    async fn get_historic_data(
        &mut self, symbol: &str, period: usize, start: i64, end: i64,
    ) -> Result<ResponseBody<InstrumentData<VEC_DOHLC>>>;

    async fn get_instrument_tick(&mut self, symbol: &str) -> Result<ResponseBody<InstrumentTick>>;
    async fn get_instrument_swap(&mut self, symbol: &str) -> Result<ResponseBody<InstrumentSwap>>;
    async fn get_ask_bid(&mut self, symbol: &str) -> Result<(f64, f64)>;
    async fn get_symbols(&mut self) -> Result<ResponseBody<InstrumentData<VEC_DOHLC>>>;

    // Market status
    async fn get_market_hours(&mut self, symbol: &str) -> Result<ResponseBody<MarketHours>>;
    async fn is_market_open(&mut self, symbol: &str) -> Result<ResponseBody<bool>>;
    async fn is_market_available(&mut self, symbol: &str) -> bool;

    // Trading
    async fn open_trade(
        &mut self, trade: TradeData<TradeIn>, orders: Option<Vec<Order>>,
    ) -> Result<ResponseBody<TradeResponse<TradeIn>>>;

    async fn close_trade(
        &mut self, trade: TradeData<TradeOut>,
    ) -> Result<ResponseBody<TradeResponse<TradeOut>>>;

    async fn open_order(
        &mut self, trade: TradeData<TradeIn>, order: TradeData<Order>,
    ) -> Result<ResponseBody<TradeResponse<TradeIn>>>;

    async fn close_order(
        &mut self, trade: TradeData<TradeOut>, order: TradeData<Order>,
    ) -> Result<ResponseBody<TradeResponse<TradeOut>>>;

    // Positions
    async fn get_active_positions(
        &mut self, symbol: &str, strategy_name: &str,
    ) -> Result<ResponseBody<PositionResult>>;

    async fn get_transaction_details(
        &mut self, symbol: &str, strategy_name: &str, id: Option<usize>,
    ) -> Option<TransactionDetails>;

    async fn get_transactions_history(
        &mut self, symbol: &str, strategy_name: &str, id: Option<usize>,
    ) -> Option<TransactionDetails>;

    // ── Streaming (KEY CHANGE) ────────────────────────────────────────────
    // Returns a channel receiver. The broker impl internally reads from
    // its WebSocket and forwards serialized ResponseBody JSON strings.
    // The server reads from this receiver — no tungstenite types exposed.
    async fn subscribe_stream(
        &mut self, symbol: &str,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<String>>;

    // Static: parse a raw broker message string into a ResponseBody JSON string.
    // Each broker impl knows its own message format.
    async fn parse_stream_data(msg: &str, symbol: &str, strategy_name: &str) -> Option<String>
        where Self: Sized;
}
```

**What was removed from the trait:**
- `get_stream()` — tungstenite internal type, not broker-agnostic
- `get_session_id()` — XTB-specific concept
- `open_trade_real()` / `open_trade_test()` — impl details (moved inside broker structs)
- `close_trade_real()` / `close_trade_test()` — same
- `open_order_test()` / `close_order_test()` — same
- `get_instrument_tick_test()` — test-mode detail
- `subscribe_tick_prices()` — merged into `subscribe_stream()` (broker decides internally)
- `subscribe_trades()` — same
- `read()` — internal helper, not interface
- `listen()` — unused, empty in XTB impl

---

## Step 4 — Implement Darwinex Broker

### Files Affected
- `rs_algo_shared/src/broker/darwinex.rs` ← **NEW FILE**

### 4.1 Struct

```rust
// rs_algo_shared/src/broker/darwinex.rs

pub struct Darwinex {
    ws: WebSocket,                              // single WS connection (quotes stream)
    http: reqwest::Client,                      // REST client (data, trading, positions)
    access_token: String,
    symbol: String,
}
```

This replaces the two-field `Xtb { socket, stream }` with a single `ws` + an HTTP client.

### 4.2 Connection Flow

```
1. Darwinex::new()
   └── create reqwest::Client with default headers
   └── ws = WebSocket::connect("wss://api.darwinex.com/quotewebsocket/1.0.0")

2. login(username, password)
   └── POST https://api.darwinex.com/token → access_token
   └── Store token, set Authorization header on http client
   └── Send WS auth handshake (if required by broker)

3. subscribe_stream(symbol)
   └── Send: {"op": "subscribe", "productNames": [symbol]}
   └── Spawn tokio task: loop { read WS msg → parse → send to channel }
   └── Return: UnboundedReceiver<String>

4. get_instrument_data(symbol, period, from)
   └── GET https://api.darwinex.com/... (REST, with Bearer token)
   └── Parse OHLC response → VEC_DOHLC

5. open_trade / close_trade
   └── POST/DELETE https://api.darwinex.com/... (Darwin Trading API REST)

6. keepalive_ping()
   └── Send WS ping frame
```

### 4.3 subscribe_stream Implementation Pattern

```rust
// Broker spawns its own read loop, server holds the receiver
async fn subscribe_stream(
    &mut self, symbol: &str,
) -> Result<tokio::sync::mpsc::UnboundedReceiver<String>> {
    let subscribe_msg = json!({
        "op": "subscribe",
        "productNames": [symbol]
    });
    self.ws.send(&subscribe_msg.to_string()).await?;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut ws_stream = self.ws.get_stream(); // internal split

    tokio::spawn(async move {
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Text(txt)) => {
                    if let Some(parsed) = Self::parse_stream_data(&txt, symbol, "").await {
                        let _ = tx.send(parsed);
                    }
                }
                Ok(Message::Ping(_)) => { /* broker handles pong internally */ }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    });

    Ok(rx)
}
```

### 4.4 parse_stream_data for Darwinex

```rust
async fn parse_stream_data(msg: &str, symbol: &str, strategy_name: &str) -> Option<String> {
    let obj: Value = serde_json::from_str(msg).ok()?;

    // Darwinex quote update: {"productName": "EURUSD", "quote": 1.08432, "timestamp": ...}
    if let (Some(product), Some(quote), Some(ts)) = (
        obj["productName"].as_str(),
        obj["quote"].as_f64(),
        obj["timestamp"].as_i64(),
    ) {
        if product != symbol { return None; }

        let tick = InstrumentTick::new()
            .symbol(product.to_string())
            .ask(quote)
            .bid(quote)
            .time(ts / 1000)
            .build()
            .ok()?;

        let response = ResponseBody {
            response: ResponseType::SubscribeTickPrices,
            payload: Some(tick),
        };
        return serde_json::to_string(&response).ok();
    }

    None
}
```

**Note:** Darwinex's WS delivers product quote updates. OHLC candles come from REST. The `subscribe_stream` implementation constructs candle closures or tick events from quote updates depending on the configured time frame.

---

## Step 5 — Broker Factory

### Files Affected
- `rs_algo_shared/src/broker/mod.rs`

### 5.1 Runtime Broker Selection

The server selects the broker at startup via an environment variable `BROKER=darwinex` (or `xtb`).

Because the two broker types are different concrete types, the factory returns `Box<dyn BrokerStream>`:

```rust
// rs_algo_shared/src/broker/mod.rs

pub mod broker_trait;
pub mod darwinex;
pub mod models;
pub mod xtb_models;
#[cfg(feature = "xtb")]
pub mod xtb_stream;  // deprecated

pub use broker_trait::BrokerStream;
pub use models::*;

pub enum BrokerKind {
    Darwinex,
    #[cfg(feature = "xtb")]
    Xtb,
}

impl BrokerKind {
    pub fn from_env() -> Self {
        match std::env::var("BROKER").as_deref() {
            Ok("darwinex") => BrokerKind::Darwinex,
            #[cfg(feature = "xtb")]
            Ok("xtb") => BrokerKind::Xtb,
            _ => BrokerKind::Darwinex, // default
        }
    }
}

pub async fn create_broker() -> Box<dyn BrokerStream> {
    match BrokerKind::from_env() {
        BrokerKind::Darwinex => Box::new(darwinex::Darwinex::new().await),
        #[cfg(feature = "xtb")]
        BrokerKind::Xtb => Box::new(xtb_stream::Xtb::new().await),
    }
}
```

**Note on `Box<dyn BrokerStream>`:** Since `BrokerStream` uses async methods, it must be object-safe. This requires the `async_trait` pattern or using `Box<dyn Future>` returns. With native async traits and the `async-trait` crate removed, use `#[trait_variant::make(BrokerStreamSend: Send)]` pattern (Rust 1.78+) OR keep a separate object-safe wrapper trait. This is documented in the implementation notes below.

**Alternative — if object safety is a concern**, use an enum dispatch pattern:

```rust
pub enum AnyBroker {
    Darwinex(Darwinex),
    #[cfg(feature = "xtb")]
    Xtb(Xtb),
}

impl BrokerStream for AnyBroker {
    async fn login(...) -> ... {
        match self {
            AnyBroker::Darwinex(b) => b.login(...).await,
            AnyBroker::Xtb(b) => b.login(...).await,
        }
    }
    // ... all other methods
}
```

This avoids `Box<dyn>` entirely and is zero-cost. **Recommended approach.**

---

## Step 6 — Update WebSocket Layer (single connection)

### Files Affected
- `rs_algo_shared/src/ws/ws_client.rs`
- `rs_algo_shared/src/ws/ws_stream_client.rs`

### 6.1 Problem

`ws_client.rs` uses synchronous/blocking tungstenite. `ws_stream_client.rs` uses async tokio-tungstenite with a split stream. For a single-connection broker:

- Commands and stream data share the same WebSocket
- The broker impl needs to multiplex internally (read task → channel, write via sender)

### 6.2 New ws_client.rs — Unified Async WebSocket

Replace the two separate clients with one unified async client. The Darwinex broker uses `ws_stream_client.rs` (async) only. `ws_client.rs` (sync) can be kept only for the deprecated XTB feature:

```rust
// Darwinex uses ws_stream_client.rs for its single async connection
// The broker struct holds the write half, spawns a read task with the read half
```

No structural changes needed to the WS client files for Darwinex — `ws_stream_client.rs` already supports splitting:
```rust
// ws_stream_client.rs already has:
pub async fn get_stream(&mut self) -> &mut SplitStream<...>
```

The Darwinex impl will call `connect()` then split internally, keeping the write half for commands and spawning a task with the read half.

**Update `ws_stream_client.rs`** for the new tungstenite 0.26 API (minor breaking changes in message type naming).

---

## Step 7 — Update rs_algo_ws_server

### Files Affected
- `rs_algo_ws_server/src/server.rs`
- `rs_algo_ws_server/src/message.rs`
- `rs_algo_ws_server/src/handlers/stream.rs`

### 7.1 server.rs — Remove Hardcoded XTB

**Current (lines 8, 67-70):**
```rust
use rs_algo_shared::broker::xtb_stream::*;
...
let mut broker = Xtb::new().await;
broker.login(username, password).await.unwrap();
```

**After:**
```rust
use rs_algo_shared::broker::{create_broker, AnyBroker};
...
let username = env::var("BROKER_USERNAME")?;
let password = env::var("BROKER_PASSWORD")?;
let mut broker = create_broker().await;  // reads BROKER env var
broker.login(&username, &password).await?;
```

The type of `broker` changes from `Xtb` to `AnyBroker` (or `Box<dyn BrokerStream>`). Because `broker` is passed into `Arc<Mutex<_>>`, the type just needs to be `Send + Sync`.

### 7.2 message.rs — Fix InitSession Raw JSON Access

**Current (lines 91-96) — fragile raw field access:**
```rust
let bot: BotData = serde_json::from_value(data.clone()).unwrap();
let uuid = bot.uuid();
let symbol = data["symbol"].as_str().unwrap();       // redundant — bot has this
let time_frame = data["time_frame"].as_str().unwrap(); // redundant
let strategy_name = data["strategy_name"].as_str().unwrap(); // redundant
let id = data["_id"].as_str().unwrap();              // redundant
```

**After — use bot methods exclusively:**
```rust
let bot: BotData = serde_json::from_value(data.clone())?;
let uuid = bot.uuid();
let symbol = bot.symbol();          // BotData accessor
let time_frame = bot.time_frame();  // BotData accessor
let strategy_name = bot.strategy_name(); // BotData accessor
```

This requires **`BotData` to expose accessor methods** for `symbol`, `time_frame`, `strategy_name`. Check `rs_algo_shared/src/models/bot.rs` — if these methods don't exist, add them.

**Also replace `unwrap()` with `?`** throughout `message.rs` handler arms.

### 7.3 stream.rs — Remove Hardcoded Xtb + Use Channel

**Current (lines 6, 19, 25):**
```rust
use rs_algo_shared::{broker::xtb_stream::*, models::environment};
...
async fn initialize_broker_stream(symbol: &str) -> Result<Xtb, RsAlgoErrorKind> {
    let mut broker_stream = Xtb::new().await;
    ...
    Ok(broker_stream)
}
```
And then:
```rust
stream = broker_stream.get_stream().await.next() => { ... }
```

**After — broker returns a channel:**

```rust
use rs_algo_shared::broker::{create_broker, BrokerStream};

pub fn listen<BK>(broker: Arc<Mutex<BK>>, session: Session)
where
    BK: BrokerStream + Send + 'static,
{
    tokio::spawn(async move {
        let symbol = session.symbol.as_ref();

        // Broker handles its own internal connection for streaming.
        // subscribe_stream() returns a channel receiver.
        let mut stream_rx = {
            let mut guard = broker.lock().await;
            guard.subscribe_stream(symbol).await.unwrap()
        };

        let keepalive_ms = env::var("KEEPALIVE_INTERVAL")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let mut interval = time::interval(Duration::from_millis(keepalive_ms));

        loop {
            tokio::select! {
                msg = stream_rx.recv() => {
                    match msg {
                        Some(txt) => {
                            match message::send(&session, Message::Text(txt)).await {
                                Ok(_) => (),
                                Err(_) => {
                                    log::error!("Can't send to {:?}", session.bot_name());
                                    break;
                                }
                            }
                        }
                        None => {
                            // Channel closed — broker stream ended
                            message::send_reconnect(&session, ReconnectOptions { clean_data: true }).await;
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    broker.lock().await.keepalive_ping().await.unwrap();
                }
            }
        }
    });
}
```

**Key changes:**
- No second broker connection created in `stream.rs`
- `initialize_broker_stream()` is removed entirely
- `get_stream()` is not called
- `BK::parse_stream_data()` is called inside the broker's `subscribe_stream()` task, not in the server
- `stream.rs` is purely routing — broker-agnostic

---

## Step 8 — Deprecate XTB

### Files Affected
- `rs_algo_shared/src/broker/xtb.rs`
- `rs_algo_shared/src/broker/xtb_stream.rs`

Once Darwinex is working and tested:

1. Move `xtb.rs` and `xtb_stream.rs` behind `#[cfg(feature = "xtb")]`
2. Remove the `xtb` feature from the default features in `rs_algo_ws_server/Cargo.toml`
3. Eventually delete both files when no longer needed

```toml
# rs_algo_shared/Cargo.toml
[features]
default = []
darwinex = ["tokio-tungstenite", "futures-util", "tokio", "reqwest"]
xtb = ["tungstenite", "tokio-tungstenite", "futures-util", "tokio"]  # migration only
```

```toml
# rs_algo_ws_server/Cargo.toml
rs_algo_shared = { ..., features = ["darwinex", "websocket"] }
# Previously: features = ["broker", "websocket"]
```

---

## Environment Variables Reference

### Changes from XTB → Darwinex

| Old (XTB) | New (Darwinex) | Notes |
|---|---|---|
| `BROKER_URL` | `DARWINEX_WS_URL` | `wss://api.darwinex.com/quotewebsocket/1.0.0` |
| `BROKER_STREAM_URL` | *(removed)* | Single connection, no stream URL |
| `STREAM_SUBSCRIBE` | *(removed)* | Single connection, always stream |
| `BROKER_USERNAME` | `BROKER_USERNAME` | Keep — used for OAuth token request |
| `BROKER_PASSWORD` | `BROKER_PASSWORD` | Keep — used for OAuth token request |
| *(new)* | `BROKER` | `darwinex` or `xtb` — selects broker at runtime |
| *(new)* | `DARWINEX_API_BASE_URL` | `https://api.darwinex.com` |
| *(new)* | `DARWINEX_TOKEN_URL` | `https://api.darwinex.com/token` |

All other env vars (`ENV`, `SYMBOL`, `MARKET`, `STRATEGY_NAME`, `TIME_FRAME`, etc.) are unchanged — they belong to the bot layer, not the broker.

---

## Files Affected Summary

### rs_algo_shared

| File | Change Type | Description |
|---|---|---|
| `Cargo.toml` | **MODIFY** | Edition 2024, update all deps, rename features |
| `src/broker/mod.rs` | **MODIFY** | Add factory, AnyBroker enum, update exports |
| `src/broker/broker_trait.rs` | **CREATE** | Clean BrokerStream trait (extracted from xtb_stream.rs) |
| `src/broker/darwinex.rs` | **CREATE** | Darwinex broker impl (single WS + REST) |
| `src/broker/models.rs` | **MODIFY** | Keep generic types, remove XTB-specific structs |
| `src/broker/xtb_models.rs` | **CREATE** | XTB-specific structs moved from models.rs |
| `src/broker/xtb_stream.rs` | **MODIFY** | Remove trait (moved to broker_trait.rs), keep Xtb impl only; gate with `#[cfg(feature = "xtb")]` |
| `src/broker/xtb.rs` | **DELETE/GATE** | Unused basic Broker trait — delete or gate with `#[cfg(feature = "xtb")]` |
| `src/ws/ws_stream_client.rs` | **MODIFY** | Update for tungstenite 0.26 API changes |
| `src/ws/ws_client.rs` | **MODIFY** | Gate with `#[cfg(feature = "xtb")]` (sync WS only used by XTB) |
| `src/models/bot.rs` | **MODIFY** | Add `symbol()`, `time_frame()`, `strategy_name()` accessor methods if missing |

### rs-algo-bot / rs_algo_ws_server

| File | Change Type | Description |
|---|---|---|
| `Cargo.toml` (workspace) | **MODIFY** | Edition 2024 |
| `rs_algo_ws_server/Cargo.toml` | **MODIFY** | Update all deps, change rs_algo_shared feature `broker` → `darwinex` |
| `rs_algo_ws_server/src/server.rs` | **MODIFY** | Replace `Xtb::new()` with `create_broker()` |
| `rs_algo_ws_server/src/message.rs` | **MODIFY** | Fix InitSession: remove raw JSON field access, use BotData methods |
| `rs_algo_ws_server/src/handlers/stream.rs` | **MODIFY** | Remove Xtb hardcoding, use channel from `subscribe_stream()` |

### rs-algo-bot / rs_algo_bot

| File | Change Type | Description |
|---|---|---|
| `rs_algo_bot/Cargo.toml` | **MODIFY** | Edition 2024, update rs_algo_shared rev |

---

## Multi-Client Server Architecture

The server already handles multiple independent WS clients correctly and this is preserved in the new design:

- `server.rs` calls `tokio::spawn` per incoming TCP connection → each bot runs in its own async task
- Each connection creates its own `broker` instance (currently `Xtb::new().await`, will become `create_broker().await`)
- `Sessions = Arc<Mutex<HashMap<SocketAddr, Session>>>` — shared registry, keyed by socket address
- **Old design**: each bot spawned TWO broker connections (command socket + separate stream socket via `initialize_broker_stream`)
- **New design**: one broker per bot; broker internally manages the stream channel — simpler, same isolation

No architectural change needed for multi-client support. The refactor reduces broker connections per bot from 2 → 1.

---

## Implementation Order

Execute in this sequence to minimize compilation breakage:

```
✅ 1. rs_algo_shared/Cargo.toml          → edition 2024 + dep update (all 3 Cargo files)
✅ 2. rs_algo_shared/src/broker/models.rs + xtb_models.rs   → split structs
🔄 3. broker_trait.rs (created) + xtb_stream.rs (in progress) → extract trait, remove get_stream
4. rs_algo_shared/src/broker/mod.rs                       → AnyBroker + factory
5. rs_algo_shared/src/broker/darwinex.rs                  → Darwinex impl
6. rs_algo_ws_server/src/server.rs                        → broker factory (remove hardcoded Xtb)
7. rs_algo_ws_server/src/handlers/stream.rs               → channel-based streaming
8. rs_algo_ws_server/src/message.rs                       → InitSession cleanup
9. Test Darwinex integration end-to-end
10. Gate XTB behind feature flag / remove
```
