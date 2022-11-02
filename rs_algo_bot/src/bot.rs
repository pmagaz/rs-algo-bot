use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::message;

use crate::helpers::vars::*;
use crate::strategies;
use crate::strategies::strategy::*;
use rs_algo_shared::broker::{LECHES, VEC_DOHLC};
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
    pub higher_time_frame: TimeFrameType,
    pub strategy: Box<dyn Strategy>,
    pub strategy_name: String,
    pub strategy_type: StrategyType,
}

impl Bot {
    pub fn new() -> BotBuilder {
        BotBuilder::new()
    }

    pub async fn run(&mut self) {
        let get_instrument_data = Command {
            command: CommandType::GetInstrumentData,
            data: Some(Payload {
                symbol: &self.symbol,
                strategy: &self.strategy_name,
                time_frame: self.time_frame.to_owned(),
                strategy_type: self.strategy_type.to_owned(),
            }),
        };

        log::info!("Requesting {} {} data", &self.symbol, &self.time_frame);

        self.websocket
            .send(&serde_json::to_string(&get_instrument_data).unwrap())
            .await
            .unwrap();

        let is_multi_timeframe_strategy = match self.strategy_type {
            StrategyType::OnlyLongMultiTF => true,
            StrategyType::LongShortMultiTF => true,
            StrategyType::OnlyShortMultiTF => true,
            _ => false,
        };

        if is_multi_timeframe_strategy {
            let get_higher_instrument_data = Command {
                command: CommandType::GetInstrumentData,
                data: Some(Payload {
                    symbol: &self.symbol,
                    strategy: &self.strategy_name,
                    strategy_type: self.strategy_type.to_owned(),
                    time_frame: self.higher_time_frame.to_owned(),
                }),
            };

            log::info!(
                "Requesting {} {} data",
                &self.symbol,
                &self.higher_time_frame
            );

            self.websocket
                .send(&serde_json::to_string(&get_higher_instrument_data).unwrap())
                .await
                .unwrap();
        }

        let subscribe_command = Command {
            command: CommandType::SubscribeStream,
            data: Some(Payload {
                symbol: &self.symbol,
                strategy: &self.strategy_name,
                strategy_type: self.strategy_type.to_owned(),
                time_frame: self.time_frame.to_owned(),
            }),
        };

        log::info!(
            "Subscribing to {} {} stream",
            &self.symbol,
            &self.time_frame
        );

        self.websocket
            .send(&serde_json::to_string(&subscribe_command).unwrap())
            .await
            .unwrap();

        loop {
            let msg = self.websocket.read().await.unwrap();
            match msg {
                Message::Text(txt) => {
                    let msg = message::parse(&txt);

                    match msg {
                        Response::Connected(res) => {
                            println!("Connected {:?}", res);
                        }
                        Response::InstrumentData(res) => {
                            let payload = res.payload.unwrap();
                            let time_frame = payload.time_frame;
                            let data = payload.data;

                            if is_base_time_frame(&self.time_frame, &time_frame) {
                                log::info!("Getting {} {} data", &self.symbol, &self.time_frame);

                                self.instrument.set_data(data).unwrap();
                            } else {
                                log::info!(
                                    "Getting {} {} data",
                                    &self.symbol,
                                    &self.higher_time_frame
                                );

                                match &mut self.higher_tm_instrument {
                                    HigherTMInstrument::HigherTMInstrument(htf_instrument) => {
                                        htf_instrument.set_data(data).unwrap();
                                    }
                                    HigherTMInstrument::None => (),
                                };
                            }
                        }
                        Response::StreamResponse(res) => {
                            let payload = res.payload.unwrap();
                            let time_frame = payload.time_frame;
                            let data = payload.data;

                            log::info!("Stream {} data received", &self.symbol);

                            if is_base_time_frame(&self.time_frame, &time_frame) {
                            } else {
                            }
                            //let adapted = adapt_to_timeframe(data);
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
                _ => panic!("Unexpected response type!"),
            };
        }
    }

    pub fn adapt_to_timeframe(data: LECHES) -> LECHES {
        data
    }
}

pub struct BotBuilder {
    symbol: Option<String>,
    market: Option<Market>,
    time_frame: Option<TimeFrameType>,
    higher_time_frame: Option<TimeFrameType>,
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
            higher_time_frame: None,
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

    pub fn higher_time_frame(mut self, val: TimeFrameType) -> Self {
        self.higher_time_frame = Some(val);
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
            Some(higher_time_frame),
            Some(strategy_name),
            Some(strategy_type),
            Some(websocket),
        ) = (
            self.symbol,
            self.market,
            self.time_frame,
            self.higher_time_frame,
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
                higher_time_frame,
                strategy_name: strategy_name.clone(),
                strategy_type,
                websocket,
                instrument,
                higher_tm_instrument: HigherTMInstrument::None,
                strategy: set_strategy(&strategy_name),
            })
        } else {
            Err(RsAlgoError {
                err: RsAlgoErrorKind::WrongInstrumentConf,
            })
        }
    }
}
