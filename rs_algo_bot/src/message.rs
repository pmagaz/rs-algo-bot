use rs_algo_shared::broker::{VEC_DOHLC, VEC_LECHES};
use rs_algo_shared::helpers::date::DateTime;
use rs_algo_shared::ws::message::*;
use serde_json::Value;
use std::str::FromStr;

pub fn parse(msg: &str) -> Response {
    if msg.len() > 0 {
        let parsed: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
        let response = parsed["response"].as_str();
        let symbol = match parsed["symbol"].as_str() {
            Some(txt) => txt,
            None => "",
        };
        log::info!("Processing response {:?}...", response);

        match response {
            Some("Connected") => Response::Connected(ResponseBody {
                response: ResponseType::Connected,
                data: None,
            }),
            Some("GetSymbolData") => Response::DataResponse(ResponseBody {
                response: ResponseType::GetSymbolData,
                data: Some(SymbolData {
                    symbol: symbol.to_owned(),
                    data: parse_dohlc(&parsed["data"]),
                }),
            }),
            Some("SubscribeStream") => Response::StreamResponse(ResponseBody {
                response: ResponseType::SubscribeStream,
                data: Some(SymbolData {
                    symbol: symbol.to_owned(),
                    data: parse_stream(&parsed["data"]),
                }),
            }),
            _ => Response::Error(ResponseBody {
                response: ResponseType::Error,
                data: None,
            }),
        }
    } else {
        Response::Error(ResponseBody {
            response: ResponseType::Error,
            data: None,
        })
    }
}

pub fn parse_dohlc(data: &Value) -> VEC_DOHLC {
    let mut result: VEC_DOHLC = vec![];
    for obj in data["data"].as_array().unwrap() {
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

pub fn parse_stream(data: &Value) -> VEC_LECHES {
    let mut result: VEC_LECHES = vec![];
    let arr = data.as_array().unwrap();
    let ask = arr[0].as_f64().unwrap();
    let bid = arr[1].as_f64().unwrap();
    let high = arr[2].as_f64().unwrap();
    let low = arr[3].as_f64().unwrap();
    let volume = arr[4].as_f64().unwrap();
    let timestamp = arr[5].as_f64().unwrap();
    let spread = arr[6].as_f64().unwrap();
    result.push((ask, bid, high, low, volume, timestamp, spread));
    result
}
