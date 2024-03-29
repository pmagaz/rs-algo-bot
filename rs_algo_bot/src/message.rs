use rs_algo_shared::broker::{DOHLC, VEC_DOHLC};
use rs_algo_shared::helpers::date::DateTime;
use rs_algo_shared::models::bot::BotData;
use rs_algo_shared::models::market::MarketHours;
use rs_algo_shared::models::tick::InstrumentTick;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::ws::message::*;

use serde_json::Value;
use std::str::FromStr;

pub fn get_type(msg: &str) -> MessageType {
    if !msg.is_empty() {
        let parsed: Value = serde_json::from_str(msg).expect("Can't parse to JSON");
        let response = parsed["response"].as_str();
        let payload = &parsed["payload"];
        let data = &parsed["payload"]["data"];
        let symbol = parsed["payload"]["symbol"].as_str().unwrap_or("");
        let accepted = parsed["payload"]["accepted"].as_bool().unwrap_or(false);

        let time_frame = match parsed["payload"]["time_frame"].as_str() {
            Some(tm) => TimeFrameType::from_str(tm),
            None => TimeFrameType::ERR,
        };

        match response {
            Some("Connected") => MessageType::Connected(ResponseBody {
                response: ResponseType::Connected,
                payload: None,
            }),
            Some("Reconnect") => MessageType::Reconnect(ResponseBody {
                response: ResponseType::Reconnect,
                payload: None,
            }),
            Some("InitSession") => MessageType::InitSession(ResponseBody {
                response: ResponseType::InitSession,
                payload: Some(parse_bot_data(payload)),
            }),
            Some("GetMarketHours") => MessageType::MarketHours(ResponseBody {
                response: ResponseType::GetMarketHours,
                payload: Some(parse_market_hours(payload)),
            }),
            Some("GetActivePositions") => MessageType::ActivePositions(ResponseBody {
                response: ResponseType::GetActivePositions,
                payload: Some(parse_active_positions(payload)),
            }),
            Some("IsMarketOpen") => MessageType::IsMarketOpen(ResponseBody {
                response: ResponseType::GetMarketHours,
                payload: Some(parse_market_open(payload)),
            }),
            Some("GetInstrumentTick") => MessageType::InstrumentTick(ResponseBody {
                response: ResponseType::GetInstrumentTick,
                payload: Some(parse_tick_data(payload)),
            }),
            Some("GetInstrumentData") => MessageType::InstrumentData(ResponseBody {
                response: ResponseType::GetInstrumentData,
                payload: Some(InstrumentData {
                    symbol: symbol.to_owned(),
                    time_frame,
                    data: parse_vec_dohlc(data),
                }),
            }),
            Some("SubscribeStream") => MessageType::StreamResponse(ResponseBody {
                response: ResponseType::SubscribeStream,
                payload: Some(InstrumentData {
                    symbol: symbol.to_owned(),
                    time_frame,
                    data: parse_dohlc(payload),
                }),
            }),
            Some("SubscribeTickPrices") => MessageType::StreamTickResponse(ResponseBody {
                response: ResponseType::SubscribeTickPrices,
                payload: Some(parse_tick_data(payload)),
            }),
            // Some("SubscribeTrades") => MessageType::StreamTradesResponse(ResponseBody {
            //     response: ResponseType::SubscribeTrades,
            //     payload: Some(parse_trades_data(payload)),
            // }),
            Some("TradeInFulfilled") => MessageType::TradeInFulfilled(ResponseBody {
                response: ResponseType::TradeInFulfilled,
                payload: Some(TradeResponse {
                    symbol: symbol.to_owned(),
                    accepted,
                    data: parse_trade_in(data),
                }),
            }),
            Some("TradeOutFulfilled") => MessageType::TradeOutFulfilled(ResponseBody {
                response: ResponseType::TradeOutFulfilled,
                payload: Some(TradeResponse {
                    symbol: symbol.to_owned(),
                    accepted,
                    data: parse_trade_out(data),
                }),
            }),
            _ => MessageType::Error(ResponseBody {
                response: ResponseType::Error,
                payload: None,
            }),
        }
    } else {
        MessageType::Error(ResponseBody {
            response: ResponseType::Error,
            payload: None,
        })
    }
}

pub fn parse_trade_in(data: &Value) -> TradeIn {
    let trade_in: TradeIn = serde_json::from_value(data.clone()).unwrap();
    trade_in
}

pub fn parse_trade_out(data: &Value) -> TradeOut {
    let trade_out: TradeOut = serde_json::from_value(data.clone()).unwrap();
    trade_out
}

pub fn parse_bot_data(data: &Value) -> BotData {
    let bot_data: BotData = serde_json::from_value(data.clone()).unwrap();
    bot_data
}

pub fn parse_tick_data(data: &Value) -> InstrumentTick {
    let tick: InstrumentTick = serde_json::from_value(data.clone()).unwrap();
    tick
}

pub fn parse_trades_data(data: &Value) -> TradeResult {
    let trade_result: TradeResult = serde_json::from_value(data.clone()).unwrap();
    trade_result
}

pub fn parse_market_hours(data: &Value) -> MarketHours {
    let market_hours: MarketHours = serde_json::from_value(data.clone()).unwrap();
    market_hours
}

pub fn parse_market_open(data: &Value) -> bool {
    let market_open: bool = serde_json::from_value(data.clone()).unwrap();
    market_open
}

pub fn parse_active_positions(data: &Value) -> PositionResult {
    let active_positions: PositionResult = serde_json::from_value(data.clone()).unwrap();
    active_positions
}

pub fn parse_dohlc(data: &Value) -> DOHLC {
    let obj = data.as_array().unwrap();
    let date = DateTime::from_str(obj[0].as_str().unwrap()).unwrap();
    let open = obj[1].as_f64().unwrap();
    let high = obj[2].as_f64().unwrap();
    let low = obj[3].as_f64().unwrap();
    let close = obj[4].as_f64().unwrap();
    let volume = obj[5].as_f64().unwrap();
    (date, open, high, low, close, volume)
}

pub fn parse_vec_dohlc(data: &Value) -> VEC_DOHLC {
    let mut result: VEC_DOHLC = vec![];
    for obj in data.as_array().unwrap() {
        result.push(parse_dohlc(obj));
    }
    result
}
