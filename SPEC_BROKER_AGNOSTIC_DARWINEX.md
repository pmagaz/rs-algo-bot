# SPEC: Broker-Agnostic Refactor + Darwinex Integration

**Scope:** `rs_algo_shared` + `rs-algo-bot` (workspace: `rs_algo_ws_server`, `rs_algo_bot`)  
**Goal:** Replace XTB with Darwinex, make broker layer fully agnostic, update to Rust 2024 edition, modernize dependencies.

---

## Table of Contents

1. [Context & Current State](#1-context--current-state)
2. [Core Architecture Principle](#2-core-architecture-principle)
3. [Darwinex API Overview](#3-darwinex-api-overview)
4. [Architecture: Before vs After](#4-architecture-before-vs-after)
5. [Step 1 — Rust 2024 Edition + Dependency Update](#step-1--rust-2024-edition--dependency-update)
6. [Step 2 — Clean broker/models.rs](#step-2--clean-brokermodelsrs)
7. [Step 3 — Extract BrokerStream Trait + Restructure xtb_stream.rs](#step-3--extract-brokerstream-trait--restructure-xtb_streamrs)
8. [Step 4 — Implement Darwinex Broker](#step-4--implement-darwinex-broker)
9. [Step 5 — Broker Factory + AnyBroker Enum](#step-5--broker-factory--anybroker-enum)
10. [Step 6 — Update rs_algo_ws_server](#step-6--update-rs_algo_ws_server)
11. [Step 7 — Fix message.rs InitSession](#step-7--fix-messagers-initsession)
12. [Step 8 — Remove XTB from Server + Gate in Shared Lib](#step-8--remove-xtb-from-server--gate-in-shared-lib)
13. [Environment Variables Reference](#environment-variables-reference)
14. [Files Affected Summary](#files-affected-summary)
15. [Multi-Client Server Architecture](#multi-client-server-architecture)
16. [Implementation Order](#implementation-order)

---

## 1. Context & Current State

### Problem Summary

| Issue | Location |
|---|---|
| `BrokerStream` trait and `Xtb` impl are in the same file | `rs_algo_shared/src/broker/xtb_stream.rs` |
| Trait exposes tungstenite internals via `get_stream()` | `rs_algo_shared/src/broker/xtb_stream.rs` |
| XTB uses TWO WebSocket connections (socket + stream) | `rs_algo_shared/src/broker/xtb_stream.rs` |
| `server.rs` hardcodes `Xtb::new().await` | `rs_algo_ws_server/src/server.rs:69` |
| `stream.rs` creates a second XTB broker for streaming, hardcodes `Xtb` type | `rs_algo_ws_server/src/handlers/stream.rs:19,25` |
| `stream.rs` calls XTB-specific methods (`subscribe_tick_prices`, `subscribe_trades`, `get_stream`, `parse_stream_data`) directly — these are XTB internal details | `rs_algo_ws_server/src/handlers/stream.rs:34-38,98,53` |
| `message.rs` `InitSession` reads raw JSON fields instead of typed `BotData` methods | `rs_algo_ws_server/src/message.rs:93-96` |
| `broker/models.rs` mixes generic models with XTB-specific structs | `rs_algo_shared/src/broker/models.rs` |
| Dependencies 2-3 years out of date | Both `Cargo.toml` files |

---

## 2. Core Architecture Principle

**The server (`rs_algo_ws_server`) must be completely broker-agnostic.** It must never import or call anything from `xtb_stream`, `xtb_models`, or any other broker-specific module.

The server sees ONLY the `BrokerStream` trait methods. All broker protocol details — WebSocket commands, subscription messages, authentication handshakes, message parsing, internal connections — are **encapsulated inside the broker struct**, invisible to the server.

### What belongs where

| Layer | What it knows | What it must NOT know |
|---|---|---|
| **Server** (`server.rs`, `message.rs`, `handlers/`) | `BrokerStream` trait methods only | Any broker struct, any broker protocol command, any internal WS type |
| **Broker trait** (`broker_trait.rs`) | Canonical operations: login, data, trade, stream | Protocol details, connection count, message format |
| **Broker impl** (`darwinex.rs`) | Everything internal to Darwinex: WS URL, subscribe JSON, token auth, quote parsing | Any server concern |

### Example: stream subscription is fully internal

```
Server calls:   broker.subscribe_stream("EURUSD", "my_strategy").await
Server gets:    UnboundedReceiver<String>  (pre-serialized ResponseBody JSON)
Server does:    forward each String to the bot WS — nothing else

Inside Darwinex::subscribe_stream() {
    // Darwinex-specific: sends {"op": "subscribe", "productNames": ["EURUSD"]}
    // Spawns tokio task: reads WS → parse Darwinex quote JSON → serialize ResponseBody
    // Server never sees any of this
}
```

The server does NOT call `subscribe_tick_prices()`, `subscribe_trades()`, `get_stream()`, or `parse_stream_data()`. Those are broker-internal concepts that do not belong in the interface.

---

## 3. Darwinex API Overview

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

### Darwinex vs XTB

| Capability | XTB | Darwinex |
|---|---|---|
| Direct Forex/CFD trading via API | Yes (WebSocket) | No (MT4/MT5 only) |
| DARWIN product trading | No | Yes (REST) |
| Real-time quote streaming | WebSocket (stream) | WebSocket (single) |
| Historical OHLC data | WebSocket (socket) | REST |
| Active positions | WebSocket (socket) | REST |
| Authentication | username + password WS command | Bearer token (OAuth2) |
| Connections needed | **Two** (socket + stream) | **One** (WebSocket + REST) |

### WebSocket Connection

```
URL:      wss://api.darwinex.com/quotewebsocket/1.0.0
Auth:     HTTP header: Authorization: Bearer <access_token>
Protocol: JSON over WebSocket
```

**Subscribe message (internal to Darwinex broker):**
```json
{ "op": "subscribe", "productNames": ["EURUSD"] }
```

**Incoming quote message (internal to Darwinex broker):**
```json
{ "productName": "EURUSD", "quote": 1.08432, "timestamp": 1715000000000 }
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

### REST endpoints (for data & trading)

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/investoraccountinfo/1.0/{account_id}/productportfolio` | Active positions |
| GET | `/investoraccountinfo/1.0/{account_id}/orders` | Order history |
| POST | `/darwintrading/1.0/{account_id}/portfolio/{darwin}` | Open trade |
| DELETE | `/darwintrading/1.0/{account_id}/portfolio/{darwin}/{order_id}` | Close trade |

---

## 4. Architecture: Before vs After

### Before (XTB, dual connection, tightly coupled)

```
[Bot]
  │ WebSocket (commands/responses)
[rs_algo_ws_server]
  │
  ├─── Xtb { socket: WebSocket }              ← command channel (login, data, trades)
  │
  └─── stream.rs creates a SECOND Xtb        ← stream channel (candles, ticks)
       │  initialize_broker_stream() { Xtb::new()... }
       │  broker_stream.subscribe_tick_prices(symbol)  ← XTB internal call in server!
       │  broker_stream.subscribe_trades(symbol)       ← XTB internal call in server!
       │  broker_stream.get_stream().next()            ← tungstenite type in server!
       └─  BK::parse_stream_data(msg, ...)             ← XTB parsing in server!
```

### After (broker-agnostic, single connection per broker)

```
[Bot]
  │ WebSocket (commands/responses)
[rs_algo_ws_server]
  │  server.rs: let mut broker = create_broker().await   ← broker-agnostic factory
  │
  └─── AnyBroker::Darwinex(Darwinex { ws, http, token })
         │
         │  stream.rs: broker.subscribe_stream(symbol, strategy)
         │                   └── returns UnboundedReceiver<String>
         │
         ├── ws: WebSocketClientStream     (quotes — async, single connection)
         └── http: reqwest::Client         (REST: data, positions, trades)

         Everything inside Darwinex is hidden from the server.
```

---

## Step 1 — Rust 2024 Edition + Dependency Update ✅ DONE

### Files Affected
- `rs_algo_shared/Cargo.toml` ✅
- `rs-algo-bot/rs_algo_ws_server/Cargo.toml` ✅
- `rs-algo-bot/rs_algo_bot/Cargo.toml` ✅
- `rs-algo-bot/Cargo.toml` (workspace) ✅

### Summary of changes
- `edition = "2021"` → `edition = "2024"` in all crates
- Removed `openssl`, replaced with `rustls-tls-native-roots` in tungstenite/tokio-tungstenite
- `tokio` 1.19 → 1.44, `tungstenite` / `tokio-tungstenite` 0.18 → 0.24
- `reqwest` 0.11 → 0.12 with rustls, `serde` 1.0.139 → 1.0.219
- `chrono` 0.4.19 → 0.4.40 (removed `wasmbind`/`js-sys` features)
- `thiserror` 1.0 → 2.0, `anyhow` 1.0.58 → 1.0.98, `bson` 2.2 → 2.13
- `async-trait` kept at 0.1.83 temporarily (removed when `AnyBroker` enum dispatch lands)
- Fixed `ws_client.rs`: `write_message()` → `send()`, `read_message()` → `read()` (tungstenite 0.20+ rename)

---

## Step 2 — Clean broker/models.rs ✅ DONE

### Files Affected
- `rs_algo_shared/src/broker/models.rs` ✅
- `rs_algo_shared/src/broker/xtb_models.rs` ✅ (new)

### What was done
- `models.rs` now contains only broker-agnostic types: `DOHLC`/`VEC_DOHLC`, `TransactionCommand/Action/State`, `TransactionDetails`, `TransactionStatusnResponse`
- All XTB protocol structs moved to `xtb_models.rs`: `LoginParams`, `Command<T>`, `CommandStreaming`, `CommandGetCandles`, `CommandTickStreamParams`, `CommandTradeStatusParams`, `TradeTransactionInfo`, `TransactionInfo`, `TransactionComments`, `Symbol`, etc.

---

## Step 3 — Extract BrokerStream Trait + Restructure xtb_stream.rs ✅ DONE

### Files Affected
- `rs_algo_shared/src/broker/broker_trait.rs` ✅ (new)
- `rs_algo_shared/src/broker/xtb_stream.rs` ✅
- `rs_algo_shared/src/broker/mod.rs` ✅
- `rs_algo_shared/src/ws/ws_stream_client.rs` ✅

### What was done
- Created `broker_trait.rs` with the clean `BrokerStream` trait
- Removed from the trait: `get_stream()`, `get_session_id()`, `subscribe_tick_prices()`, `subscribe_trades()`, `listen()`, `parse_stream_data()`, `open/close_trade_real/test()`, `read()`
- `subscribe_stream()` now returns `UnboundedReceiver<String>` — broker manages its own stream loop
- `ws_stream_client.rs`: `read` field changed to `Option<SplitStream>`, added `take_read()` method
- `xtb_stream.rs`: trait block replaced by `impl BrokerStream for Xtb`. `subscribe_stream` rewrites to:
  1. Send keepalive, candle, tick, and trade subscription WS commands (XTB-internal)
  2. `take_read()` from the stream WebSocket
  3. Spawn tokio task: read messages → `Xtb::parse_stream_data()` → send to channel
  4. Return `UnboundedReceiver<String>`
- `subscribe_tick_prices`, `subscribe_trades`, `parse_stream_data` moved to `impl Xtb` (internal helpers, not on the trait)
- `broker/mod.rs` updated: added `broker_trait`, exports `BrokerStream` from `broker_trait`, exports `Xtb` from `xtb_stream`

### BrokerStream trait

```rust
#[async_trait::async_trait]
pub trait BrokerStream: Send + Sync {
    async fn new() -> Self where Self: Sized;
    async fn login(&mut self, username: &str, password: &str) -> Result<&mut Self> where Self: Sized;
    async fn disconnect(&mut self) -> Result<()>;
    async fn keepalive_ping(&mut self) -> Result<()>;
    async fn get_instrument_data(&mut self, symbol: &str, period: usize, start: i64) -> Result<ResponseBody<InstrumentData<VEC_DOHLC>>>;
    async fn get_historic_data(&mut self, symbol: &str, period: usize, start: i64, end: i64) -> Result<ResponseBody<InstrumentData<VEC_DOHLC>>>;
    async fn get_instrument_tick(&mut self, symbol: &str) -> Result<ResponseBody<InstrumentTick>>;
    async fn get_instrument_swap(&mut self, symbol: &str) -> Result<ResponseBody<InstrumentSwap>>;
    async fn get_ask_bid(&mut self, symbol: &str) -> Result<(f64, f64)>;
    async fn get_symbols(&mut self) -> Result<ResponseBody<InstrumentData<VEC_DOHLC>>>;
    async fn get_market_hours(&mut self, symbol: &str) -> Result<ResponseBody<MarketHours>>;
    async fn is_market_open(&mut self, symbol: &str) -> Result<ResponseBody<bool>>;
    async fn is_market_available(&mut self, symbol: &str) -> bool;
    async fn open_trade(&mut self, trade: TradeData<TradeIn>, orders: Option<Vec<Order>>) -> Result<ResponseBody<TradeResponse<TradeIn>>>;
    async fn close_trade(&mut self, trade: TradeData<TradeOut>) -> Result<ResponseBody<TradeResponse<TradeOut>>>;
    async fn open_order(&mut self, trade: TradeData<TradeIn>, order: TradeData<Order>) -> Result<ResponseBody<TradeResponse<TradeIn>>>;
    async fn close_order(&mut self, trade: TradeData<TradeOut>, order: TradeData<Order>) -> Result<ResponseBody<TradeResponse<TradeOut>>>;
    async fn get_active_positions(&mut self, symbol: &str, strategy_name: &str) -> Result<ResponseBody<PositionResult>>;
    async fn get_transaction_details(&mut self, symbol: &str, strategy_name: &str, id: Option<usize>) -> Option<TransactionDetails>;
    async fn get_transactions_history(&mut self, symbol: &str, strategy_name: &str, id: Option<usize>) -> Option<TransactionDetails>;
    // KEY: broker manages its stream internally, server reads from channel
    async fn subscribe_stream(&mut self, symbol: &str, strategy_name: &str) -> Result<UnboundedReceiver<String>>;
}
```

---

## Step 4 — Implement Darwinex Broker ⬅ NEXT

### Files Affected
- `rs_algo_shared/src/broker/darwinex.rs` ← **CREATE**

### 4.1 Struct

```rust
pub struct Darwinex {
    ws: WebSocketClientStream,   // single async WS (quotes stream)
    http: reqwest::Client,       // REST client (data, positions, trades)
    access_token: String,
    symbol: String,
}
```

Replaces the two-field `Xtb { socket, stream }` with a single `ws` + HTTP client.

### 4.2 Connection Flow

```
1. Darwinex::new()
   └── http = reqwest::Client::new()
   └── ws  = WebSocketClientStream::connect(DARWINEX_WS_URL).await
   └── access_token = "".to_string()  (set during login)

2. login(username, password)
   └── POST https://api.darwinex.com/token (form-urlencoded)
   └── Store access_token
   └── WS does not require separate auth command (token passed in HTTP headers at connect time)
       NOTE: token must be in connect headers — reconnect via WebSocket::connect_with_headers()

3. subscribe_stream(symbol, strategy_name)
   └── Send: {"op": "subscribe", "productNames": [symbol]}
   └── ws.take_read() → move read half into tokio task
   └── Task: loop { WS msg → parse_stream_data() → tx.send() }
   └── Return: UnboundedReceiver<String>

4. get_instrument_data(symbol, period, from)
   └── GET REST endpoint with Bearer token
   └── Parse OHLC → VEC_DOHLC

5. open_trade / close_trade
   └── POST/DELETE REST endpoints with Bearer token

6. keepalive_ping()
   └── ws.ping(&[]).await  (standard WS ping frame)
```

### 4.3 subscribe_stream Implementation

All subscription logic is inside `Darwinex::subscribe_stream`. The server sees none of it.

```rust
async fn subscribe_stream(
    &mut self,
    symbol: &str,
    strategy_name: &str,
) -> Result<mpsc::UnboundedReceiver<String>> {
    // Internal Darwinex-specific subscribe command
    let subscribe_msg = serde_json::json!({
        "op": "subscribe",
        "productNames": [symbol]
    });
    self.ws.send(&subscribe_msg.to_string()).await?;

    let mut ws_read = self.ws.take_read();
    let symbol = symbol.to_owned();
    let strategy_name = strategy_name.to_owned();
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(msg_result) = ws_read.next().await {
            match msg_result {
                Ok(Message::Text(txt)) => {
                    if let Some(parsed) = Darwinex::parse_stream_data(&txt, &symbol, &strategy_name) {
                        if tx.send(parsed).is_err() { break; }
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    });

    Ok(rx)
}
```

### 4.4 parse_stream_data for Darwinex (internal fn, not on trait)

```rust
fn parse_stream_data(txt: &str, symbol: &str, _strategy_name: &str) -> Option<String> {
    let obj: Value = serde_json::from_str(txt).ok()?;

    // Darwinex quote: {"productName": "EURUSD", "quote": 1.08432, "timestamp": 1715000000000}
    let product = obj["productName"].as_str()?;
    let quote   = obj["quote"].as_f64()?;
    let ts      = obj["timestamp"].as_i64()?;

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
    serde_json::to_string(&response).ok()
}
```

**Note:** Darwinex delivers real-time quote (bid/ask) updates, not OHLC candles. Historical OHLC comes from REST. `subscribe_stream` emits tick events; the bot reconstructs candles from ticks.

---

## Step 5 — Broker Factory + AnyBroker Enum

### Files Affected
- `rs_algo_shared/src/broker/mod.rs`

### 5.1 AnyBroker enum dispatch (recommended over `Box<dyn>`)

`Box<dyn BrokerStream>` is not object-safe with native async traits. Use enum dispatch instead — zero-cost, no vtable.

```rust
// rs_algo_shared/src/broker/mod.rs

pub mod broker_trait;
pub mod darwinex;
pub mod models;
pub mod xtb_models;
#[cfg(feature = "xtb")]
pub mod xtb_stream;

pub use broker_trait::BrokerStream;
pub use darwinex::Darwinex;
pub use models::*;

pub enum AnyBroker {
    Darwinex(darwinex::Darwinex),
    #[cfg(feature = "xtb")]
    Xtb(xtb_stream::Xtb),
}

#[async_trait::async_trait]
impl BrokerStream for AnyBroker {
    async fn new() -> Self where Self: Sized {
        AnyBroker::Darwinex(Darwinex::new().await)
    }
    async fn login(&mut self, u: &str, p: &str) -> Result<&mut Self> where Self: Sized {
        match self {
            AnyBroker::Darwinex(b) => { b.login(u, p).await?; }
            #[cfg(feature = "xtb")] AnyBroker::Xtb(b) => { b.login(u, p).await?; }
        }
        Ok(self)
    }
    // ... delegate all trait methods via match
}

pub async fn create_broker() -> AnyBroker {
    match std::env::var("BROKER").as_deref() {
        #[cfg(feature = "xtb")] Ok("xtb") => AnyBroker::Xtb(xtb_stream::Xtb::new().await),
        _ => AnyBroker::Darwinex(darwinex::Darwinex::new().await),
    }
}
```

---

## Step 6 — Update rs_algo_ws_server

### Files Affected
- `rs_algo_ws_server/src/server.rs`
- `rs_algo_ws_server/src/handlers/stream.rs`

### 6.1 server.rs — Remove Hardcoded XTB

**Current:**
```rust
use rs_algo_shared::broker::xtb_stream::*;    // XTB-specific import
...
let mut broker = Xtb::new().await;             // hardcoded XTB type
broker.login(username, password).await.unwrap();
let broker = Arc::new(Mutex::new(broker));
```

**After:**
```rust
use rs_algo_shared::broker::{create_broker, AnyBroker};
...
let mut broker = create_broker().await;        // reads BROKER env var, returns AnyBroker
broker.login(&username, &password).await.unwrap();
let broker = Arc::new(Mutex::new(broker));
```

The type changes from `Arc<Mutex<Xtb>>` to `Arc<Mutex<AnyBroker>>`. All downstream code that takes `BK: BrokerStream` still compiles without changes.

### 6.2 stream.rs — Remove XTB Coupling, Use Channel

**Current problems:**
- Imports `rs_algo_shared::broker::xtb_stream::*` (XTB-specific)
- `initialize_broker_stream()` creates a SECOND broker connection (XTB-specific need, gone with Darwinex)
- Calls `broker_stream.subscribe_tick_prices()`, `.subscribe_trades()` — XTB internals in server
- Calls `broker_stream.get_stream()` — tungstenite type in server
- Calls `BK::parse_stream_data()` — XTB parsing in server

**After:** `stream.rs` is purely routing. The broker is the same instance created in `server.rs`. No second connection. No XTB-specific methods.

```rust
use rs_algo_shared::broker::BrokerStream;
use rs_algo_shared::ws::message::ReconnectOptions;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;
use tungstenite::Message;

pub fn listen<BK: BrokerStream + Send + 'static>(broker: Arc<Mutex<BK>>, session: Session) {
    tokio::spawn(async move {
        let keepalive_ms = env::var("KEEPALIVE_INTERVAL")
            .unwrap().parse::<u64>().unwrap();

        let symbol        = session.symbol.as_ref().to_string();
        let strategy_name = session.strategy.as_ref().to_string();

        // subscribe_stream() is the only broker call here.
        // All internal subscription commands are inside the broker.
        let mut stream_rx = {
            let mut guard = broker.lock().await;
            guard.subscribe_stream(&symbol, &strategy_name).await.unwrap()
        };

        let mut interval = time::interval(Duration::from_millis(keepalive_ms));

        loop {
            tokio::select! {
                msg = stream_rx.recv() => {
                    match msg {
                        Some(txt) => {
                            if message::send(&session, Message::Text(txt)).await.is_err() {
                                log::error!("Can't send stream data to {:?}", session.bot_name());
                                break;
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
- `initialize_broker_stream()` removed entirely
- `get_stream()` never called
- `subscribe_tick_prices()`, `subscribe_trades()` never called (broker-internal)
- `parse_stream_data()` never called (broker-internal, inside the spawned task)
- `stream.rs` has zero knowledge of any broker protocol

---

## Step 7 — Fix message.rs InitSession

### Files Affected
- `rs_algo_ws_server/src/message.rs`

**Current (lines 91-96) — fragile raw JSON field access:**
```rust
let bot: BotData = serde_json::from_value(data.clone()).unwrap();
let uuid = bot.uuid();
let symbol = data["symbol"].as_str().unwrap();            // redundant — bot has this
let time_frame = data["time_frame"].as_str().unwrap();    // redundant
let strategy_name = data["strategy_name"].as_str().unwrap(); // redundant
let id = data["_id"].as_str().unwrap();                   // redundant
```

**After — use BotData methods exclusively:**
```rust
let bot: BotData = serde_json::from_value(data.clone()).unwrap();
let uuid          = bot.uuid();
let symbol        = bot.symbol();
let time_frame    = bot.time_frame();
let strategy_name = bot.strategy_name();
```

Requires `BotData` to have accessor methods — check `rs_algo_shared/src/models/bot.rs` and add them if missing.

---

## Step 8 — Remove XTB from Server + Gate in Shared Lib

### Status: After Step 6 is complete, XTB is already removed from the server.

In `rs_algo_shared`, XTB code (`xtb.rs`, `xtb_stream.rs`, `xtb_models.rs`) can be:
- Gated behind `#[cfg(feature = "xtb")]` as a migration reference
- Eventually deleted when no longer needed

```toml
# rs_algo_shared/Cargo.toml
[features]
default = []
darwinex = ["tokio-tungstenite", "futures-util", "tokio", "reqwest"]
xtb = []   # feature flag gates xtb*.rs files (migration only)
```

```toml
# rs_algo_ws_server/Cargo.toml
rs_algo_shared = { path = "../../rs_algo_shared", features = ["darwinex"] }
# No "xtb" feature — server never references XTB code
```

---

## Environment Variables Reference

### Changes from XTB → Darwinex

| Old (XTB) | New (Darwinex) | Notes |
|---|---|---|
| `BROKER_URL` | `DARWINEX_WS_URL` | `wss://api.darwinex.com/quotewebsocket/1.0.0` |
| `BROKER_STREAM_URL` | *(removed)* | Single connection, no separate stream URL |
| `STREAM_SUBSCRIBE` | *(removed)* | Single connection, always streams |
| `BROKER_USERNAME` | `BROKER_USERNAME` | Keep — used for OAuth token request |
| `BROKER_PASSWORD` | `BROKER_PASSWORD` | Keep — used for OAuth token request |
| *(new)* | `BROKER` | `darwinex` (or `xtb` if xtb feature enabled) |
| *(new)* | `DARWINEX_API_BASE_URL` | `https://api.darwinex.com` |
| *(new)* | `DARWINEX_TOKEN_URL` | `https://api.darwinex.com/token` |
| *(new)* | `DARWINEX_ACCOUNT_ID` | Investor account ID for REST calls |

All other env vars (`ENV`, `SYMBOL`, `MARKET`, `STRATEGY_NAME`, `TIME_FRAME`, etc.) are unchanged — they belong to the bot layer, not the broker.

---

## Files Affected Summary

### rs_algo_shared

| File | Change Type | Status |
|---|---|---|
| `Cargo.toml` | MODIFY — edition 2024, updated deps | ✅ Done |
| `src/broker/mod.rs` | MODIFY — add factory, AnyBroker, update exports | 🔄 In progress |
| `src/broker/broker_trait.rs` | CREATE — clean BrokerStream trait | ✅ Done |
| `src/broker/darwinex.rs` | CREATE — Darwinex impl (single WS + REST) | ⬅ Next |
| `src/broker/models.rs` | MODIFY — broker-agnostic types only | ✅ Done |
| `src/broker/xtb_models.rs` | CREATE — XTB protocol structs | ✅ Done |
| `src/broker/xtb_stream.rs` | MODIFY — trait removed, Xtb impl restructured | ✅ Done |
| `src/broker/xtb.rs` | DELETE/GATE — gate with `#[cfg(feature = "xtb")]` | Pending |
| `src/ws/ws_stream_client.rs` | MODIFY — `Option<SplitStream>`, `take_read()` | ✅ Done |
| `src/ws/ws_client.rs` | GATE — `#[cfg(feature = "xtb")]` (sync WS only for XTB) | Pending |

### rs-algo-bot / rs_algo_ws_server

| File | Change Type | Status |
|---|---|---|
| `Cargo.toml` (workspace) | MODIFY — edition 2024 | ✅ Done |
| `rs_algo_ws_server/Cargo.toml` | MODIFY — updated deps, `darwinex` feature | ✅ Done |
| `rs_algo_ws_server/src/server.rs` | MODIFY — `create_broker()` replaces `Xtb::new()` | ⬅ Step 6 |
| `rs_algo_ws_server/src/handlers/stream.rs` | MODIFY — remove all XTB coupling, channel-based | ⬅ Step 6 |
| `rs_algo_ws_server/src/message.rs` | MODIFY — BotData methods in InitSession | ⬅ Step 7 |

---

## Multi-Client Server Architecture

The server already handles multiple independent WS clients correctly and this is preserved in the new design:

- `server.rs` calls `tokio::spawn` per incoming TCP connection → each bot runs in its own async task
- Each connection creates its own `broker` instance via `create_broker().await`
- `Sessions = Arc<Mutex<HashMap<SocketAddr, Session>>>` — shared registry, keyed by socket address
- **Old design**: each bot spawned TWO broker connections (command socket + separate stream socket via `initialize_broker_stream`)
- **New design**: one broker per bot; broker internally manages the stream channel — simpler, same isolation

No architectural change needed for multi-client support. The refactor reduces broker connections per bot from 2 → 1.

---

## Implementation Order

```
✅ 1. Cargo.toml files              → edition 2024 + dep update
✅ 2. broker/models.rs + xtb_models → split structs
✅ 3. broker_trait.rs + xtb_stream  → clean trait, subscribe_stream returns channel
   4. broker/darwinex.rs            → Darwinex impl (single WS + REST)  ⬅ NEXT
   5. broker/mod.rs                 → AnyBroker enum + create_broker() factory
   6. server.rs + handlers/stream   → remove all XTB coupling, use create_broker()
   7. message.rs                    → InitSession cleanup (BotData methods)
   8. Cargo features                → gate XTB behind feature flag, server uses darwinex only
   9. End-to-end test with Darwinex
  10. Delete XTB files if no longer needed
```
