use rs_algo_shared::broker::{LECHES, VEC_DOHLC};
use rs_algo_shared::helpers::date::DateTime;
use rs_algo_shared::helpers::date::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::ws::message::*;

use serde_json::Value;
use std::str::FromStr;

// pub fn handle(msg: &str) {
//     let response = parse_response(&msg);
//     let leches = match response {
//         Response::Connected(res) => {
//             println!("Connected {:?}", res);
//         }
//         Response::InstrumentData(res) => {
//             instrument.set_data(res.data).unwrap();
//         }
//         // Response::StreamResponse() => (),
//         _ => (),
//     };
// }

pub fn parse_response(msg: &str) -> Response {
    if msg.len() > 0 {
        let parsed: Value = serde_json::from_str(&msg).expect("Can't parse to JSON");
        let response = parsed["response"].as_str();
        let symbol = match parsed["symbol"].as_str() {
            Some(txt) => txt,
            None => "",
        };

        let time_frame = match parsed["time_frame"].as_str() {
            Some(txt) => TimeFrameType::from_str(txt),
            None => TimeFrameType::M1,
        };

        log::info!("Processing response {:?}...", response);

        match response {
            Some("Connected") => Response::Connected(ResponseBody {
                response: ResponseType::Connected,
                data: None,
            }),
            Some("GetInstrumentData") => Response::InstrumentData(ResponseBody {
                response: ResponseType::GetInstrumentData,
                data: Some(SymbolData {
                    symbol: symbol.to_owned(),
                    time_frame: time_frame,
                    data: parse_dohlc(&parsed["data"]),
                }),
            }),
            Some("SubscribeStream") => Response::StreamResponse(ResponseBody {
                response: ResponseType::SubscribeStream,
                data: Some(SymbolData {
                    symbol: symbol.to_owned(),
                    time_frame: time_frame,
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
