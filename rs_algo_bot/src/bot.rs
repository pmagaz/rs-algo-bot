use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::message;

use rs_algo_shared::helpers::date::{DateTime, Duration as Dur, Local, Utc};
use rs_algo_shared::models::market::*;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::scanner::instrument::{self, HigherTMInstrument, Instrument};
use rs_algo_shared::ws::message::*;
use rs_algo_shared::ws::ws_client::WebSocket;

pub struct Bot {
    pub websocket: WebSocket,
    pub instrument: Instrument,
    pub higher_tm_instrument: HigherTMInstrument,
    pub symbol: String,
    pub market: Market,
    pub time_frame: TimeFrameType,
    pub strategy_name: String,
    pub strategy_type: StrategyType,
}

impl Bot {
    pub fn new() -> BotBuilder {
        BotBuilder::new()
    }

    pub async fn run(&mut self) {
        let get_symbol_data = Command {
            command: CommandType::GetSymbolData,
            data: Some(Data {
                strategy: "EMA200-2",
                strategy_type: StrategyType::OnlyLong,
                symbol: "BITCOIN",
                time_frame: "W",
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&get_symbol_data).unwrap())
            .await
            .unwrap();

        let subscribe_command = Command {
            command: CommandType::SubscribeStream,
            data: Some(Data {
                strategy: "EMA200-2",
                strategy_type: StrategyType::OnlyLong,
                symbol: "BITCOIN",
                time_frame: "W",
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&subscribe_command).unwrap())
            .await
            .unwrap();

        // let mut last_msg = Local::now();
        // let msg_timeout = env::var("MSG_TIMEOUT").unwrap().parse::<u64>().unwrap();

        loop {
            let msg = self.websocket.read().await.unwrap();
            match msg {
                Message::Text(txt) => {
                    //let msg = message::handle(&txt);

                    let response = message::parse_response(&txt);
                    match response {
                        Response::Connected(res) => {
                            println!("Connected {:?}", res);
                        }
                        Response::DataResponse(res) => {
                            let data = res.data.unwrap().data;
                            self.instrument.set_data(data).unwrap();
                            log::info!("Parsed Instrument data");
                        }
                        Response::StreamResponse(res) => {
                            let data = res.data.unwrap().data;
                            log::info!("Stream data received");
                            //self.instrument.next(data).unwrap();
                        }
                        _ => (),
                    };

                    // let timeout = Local::now() - Dur::milliseconds(msg_timeout as i64);
                    // if last_msg < timeout {
                    //     log::error!("No data received in last {} milliseconds", msg_timeout);
                    // } else {
                    //     last_msg = Local::now();
                    // }
                }
                Message::Ping(_txt) => {
                    log::info!("Ping received");
                    self.websocket.pong(b"").await;
                }
                _ => panic!("Unexpected message type!"),
            };
        }
    }
}

pub struct BotBuilder {
    symbol: Option<String>,
    market: Option<Market>,
    time_frame: Option<TimeFrameType>,
    strategy_name: Option<String>,
    strategy_type: Option<StrategyType>,
    websocket: Option<WebSocket>,
}

impl BotBuilder {
    pub fn new() -> BotBuilder {
        Self {
            symbol: None,
            market: None,
            time_frame: None,
            strategy_name: None,
            strategy_type: None,
            websocket: None,
        }
    }
    pub fn symbol(mut self, val: String) -> Self {
        self.symbol = Some(String::from(val));
        self
    }

    pub fn market(mut self, val: Market) -> Self {
        self.market = Some(val);
        self
    }

    pub fn time_frame(mut self, val: TimeFrameType) -> Self {
        self.time_frame = Some(val);
        self
    }

    pub fn strategy_name(mut self, val: String) -> Self {
        self.strategy_name = Some(val);
        self
    }

    pub fn strategy_type(mut self, val: StrategyType) -> Self {
        self.strategy_type = Some(val);
        self
    }

    pub fn websocket(mut self, val: WebSocket) -> Self {
        self.websocket = Some(val);
        self
    }

    pub fn build(self) -> Result<Bot> {
        if let (
            Some(symbol),
            Some(market),
            Some(time_frame),
            Some(strategy_name),
            Some(strategy_type),
            Some(websocket),
        ) = (
            self.symbol,
            self.market,
            self.time_frame,
            self.strategy_name,
            self.strategy_type,
            self.websocket,
        ) {
            let mut instrument = Instrument::new()
                .symbol(&symbol)
                .market(market.to_owned())
                .time_frame(time_frame.to_owned())
                .build()
                .unwrap();

            Ok(Bot {
                symbol,
                market,
                time_frame,
                strategy_name,
                strategy_type,
                websocket,
                instrument,
                higher_tm_instrument: HigherTMInstrument::None,
            })
        } else {
            Err(RsAlgoError {
                err: RsAlgoErrorKind::WrongInstrumentConf,
            })
        }
    }
}
