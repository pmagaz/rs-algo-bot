# rs-algo-bot

Algorithmic trading bot system built in Rust. Connects to a broker via WebSocket and executes automated trading strategies.

## Workspace

```
rs-algo-bot/
├── rs_algo_ws_server/   WebSocket server — bridges the bot and the broker
└── rs_algo_bot/         Trading bot — runs strategies, sends commands to the server
```

## Architecture

```
[rs_algo_bot]
    │  WebSocket (commands / responses)
[rs_algo_ws_server]
    │
    └── AnyBroker (broker-agnostic, selected at runtime via BROKER env var)
          ├── Darwinex  — single WS (quotes) + REST (data, trades, positions)
          └── Xtb       — legacy dual-WS (deprecated)
```

The server is fully broker-agnostic. All broker protocol details (WebSocket commands, authentication, message parsing) are encapsulated inside the broker implementation in `rs_algo_shared`. The server interacts only via the `BrokerStream` trait.

## Shared Library

Broker logic, data models, and WebSocket utilities live in [`rs_algo_shared`](../rs_algo_shared).

## Broker: Darwinex

| Concern | Detail |
|---|---|
| Auth | OAuth2 password grant → Bearer token |
| Streaming | `wss://api.darwinex.com/quotewebsocket/1.0.0` |
| Data / Trading | REST `https://api.darwinex.com` |
| Active positions | `GET /investoraccountinfo/1.0/{account_id}/productportfolio` |
| Open trade | `POST /darwintrading/1.0/{account_id}/portfolio/{darwin}` |
| Close trade | `DELETE /darwintrading/1.0/{account_id}/portfolio/{darwin}/{order_id}` |

## Environment Variables

```env
# Broker selection
BROKER=darwinex

# Credentials (used for OAuth token request)
BROKER_USERNAME=your_username
BROKER_PASSWORD=your_password

# Darwinex
DARWINEX_WS_URL=wss://api.darwinex.com/quotewebsocket/1.0.0
DARWINEX_API_BASE_URL=https://api.darwinex.com
DARWINEX_TOKEN_URL=https://api.darwinex.com/token
DARWINEX_ACCOUNT_ID=your_account_id

# Server
SERVER_ADDR=127.0.0.1:7070
KEEPALIVE_INTERVAL=30000

# Bot
ENV=Development
SYMBOL=EURUSD
TIME_FRAME=M1
STRATEGY_NAME=my_strategy
NUM_BARS=500
EXECUTION_MODE=bot
```

## Running

```bash
# Server
cargo run -p rs_algo_ws_server

# Bot
cargo run -p rs_algo_bot
```

## Tech Stack

- Rust 2024 edition
- `tokio` async runtime
- `tokio-tungstenite` 0.24 (rustls, no OpenSSL)
- `reqwest` 0.12 (rustls)
- `mongodb` for session/bot state persistence
