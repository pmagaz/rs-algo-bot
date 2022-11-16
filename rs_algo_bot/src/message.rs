use rs_algo_shared::broker::{LECHES, VEC_DOHLC};
use rs_algo_shared::helpers::date::DateTime;
use rs_algo_shared::helpers::date::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::ws::message::*;

use serde_json::Value;
use std::str::FromStr;

pub fn parse(msg: &str) -> Response {
    if !msg.is_empty() {
        let parsed: Value = serde_json::from_str(msg).expect("Can't parse to JSON");
        let response = parsed["response"].as_str();
        let symbol = parsed["payload"]["symbol"].as_str().unwrap_or("");

        let time_frame = match parsed["payload"]["time_frame"].as_str() {
            Some(tm) => TimeFrameType::from_str(tm),
            None => TimeFrameType::ERR,
        };

        match response {
            Some("Connected") => Response::Connected(ResponseBody {
                response: ResponseType::Connected,
                payload: None,
            }),
            Some("GetInstrumentData") => Response::InstrumentData(ResponseBody {
                response: ResponseType::GetInstrumentData,
                payload: Some(InstrumentData {
                    symbol: symbol.to_owned(),
                    time_frame,
                    data: parse_dohlc(&parsed["payload"]["data"]),
                }),
            }),
            Some("SubscribeStream") => Response::StreamResponse(ResponseBody {
                response: ResponseType::SubscribeStream,
                payload: Some(InstrumentData {
                    symbol: symbol.to_owned(),
                    time_frame,
                    data: parse_stream(&parsed["payload"]),
                }),
            }),
            _ => Response::Error(ResponseBody {
                response: ResponseType::Error,
                payload: None,
            }),
        }
    } else {
        Response::Error(ResponseBody {
            response: ResponseType::Error,
            payload: None,
        })
    }
}

pub fn parse_dohlc(data: &Value) -> VEC_DOHLC {
    let mut result: VEC_DOHLC = vec![];
    for obj in data.as_array().unwrap() {
        let date = DateTime::from_str(obj[0].as_str().unwrap()).unwrap();
        let open = obj[1].as_f64().unwrap();
        let high = obj[2].as_f64().unwrap();
        let low = obj[3].as_f64().unwrap();
        let close = obj[4].as_f64().unwrap();
        let volume = obj[5].as_f64().unwrap();
        result.push((date, open, high, low, close, volume));
    }
    result
}

pub fn parse_stream(data: &Value) -> LECHES {
    let arr = data.as_array().unwrap();
    let ask = arr[0].as_f64().unwrap();
    let bid = arr[1].as_f64().unwrap();
    let high = arr[2].as_f64().unwrap();
    let low = arr[3].as_f64().unwrap();
    let volume = arr[4].as_f64().unwrap();
    let timestamp = arr[5].as_f64().unwrap();
    let date = parse_time(timestamp as i64);
    let spread = arr[6].as_f64().unwrap();
    (date, ask, bid, high, low, volume, spread)
}
