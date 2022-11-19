use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::helpers::vars::*;
use crate::message;

//use crate::strategies::stats::*;
use crate::strategies::strategy::*;

use rs_algo_shared::helpers::date::Local;
use rs_algo_shared::helpers::uuid::*;
use rs_algo_shared::helpers::{date::*, uuid};
use rs_algo_shared::models::bot::BotData;
use rs_algo_shared::models::market::*;
use rs_algo_shared::models::strategy::StrategyStats;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::scanner::instrument::{HigherTMInstrument, Instrument};
use rs_algo_shared::ws::message::*;
use rs_algo_shared::ws::ws_client::WebSocket;
use serde::Serialize;

#[derive(Serialize)]
pub struct Bot {
    #[serde(skip_serializing)]
    websocket: WebSocket,
    #[serde(rename = "_id")]
    uuid: uuid::Uuid,
    symbol: String,
    market: Market,
    strategy_name: String,
    strategy_type: StrategyType,
    time_frame: TimeFrameType,
    higher_time_frame: TimeFrameType,
    date_start: DbDateTime,
    last_update: DbDateTime,
    instrument: Instrument,
    higher_tf_instrument: HigherTMInstrument,
    trades_in: Vec<TradeIn>,
    trades_out: Vec<TradeOut>,
    #[serde(skip_serializing)]
    strategy: Box<dyn Strategy>,
    strategy_stats: StrategyStats,
}

impl Bot {
    pub fn new() -> BotBuilder {
        BotBuilder::new()
    }

    pub fn generate_bot_uuid(&mut self) -> Uuid {
        let seed = [
            &self.symbol,
            &self.strategy_name,
            &self.time_frame.to_string(),
            &self.strategy_type.to_string(),
        ];

        uuid::generate(seed)
    }

    pub async fn init_session(&mut self) {
        log::info!("Creating session for {}_{}", &self.symbol, &self.time_frame);

        self.uuid = self.generate_bot_uuid();

        self.instrument.init();

        let update_bot_data_command = Command {
            command: CommandType::InitSession,
            data: Some(&self),
        };

        self.websocket
            .send(&serde_json::to_string(&update_bot_data_command).unwrap())
            .await
            .unwrap();
    }

    pub async fn get_instrument_data(&mut self) {
        log::info!("Requesting {}_{} data", &self.symbol, &self.time_frame);

        let get_instrument_data = Command {
            command: CommandType::GetInstrumentData,
            data: Some(Payload {
                symbol: &self.symbol,
                strategy: &self.strategy_name,
                time_frame: self.time_frame.to_owned(),
                strategy_type: self.strategy_type.to_owned(),
            }),
        };

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
    }

    pub async fn subscribing_to_stream(&mut self) {
        log::info!("Subscribing to {}_{}", &self.symbol, &self.time_frame);

        let subscribe_command = Command {
            command: CommandType::SubscribeStream,
            data: Some(Payload {
                symbol: &self.symbol,
                strategy: &self.strategy_name,
                strategy_type: self.strategy_type.to_owned(),
                time_frame: self.time_frame.to_owned(),
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&subscribe_command).unwrap())
            .await
            .unwrap();
    }

    pub async fn restore_values(&mut self, data: BotData) {
        self.date_start = data.date_start().clone();
        self.trades_in = data.trades_in().clone();
        self.trades_out = data.trades_out().clone();
        self.strategy_stats = data.strategy_stats().clone();
    }

    pub async fn send_bot_status(&mut self) {
        self.last_update = to_dbtime(Local::now());

        let update_bot_data_command = Command {
            command: CommandType::UpdateBotData,
            data: Some(&self),
        };

        self.websocket
            .send(&serde_json::to_string(&update_bot_data_command).unwrap())
            .await
            .unwrap();
    }

    pub async fn run(&mut self) {
        self.init_session().await;

        loop {
            let msg = self.websocket.read().await.unwrap();
            match msg {
                Message::Text(txt) => {
                    let msg = message::parse(&txt);

                    match msg {
                        Response::Connected(_res) => {
                            log::info!("Connected to server");
                        }
                        Response::InitSession(res) => {
                            log::info!("Getting session data");
                            let bot_data = res.payload.unwrap();
                            self.restore_values(bot_data).await;
                            self.get_instrument_data().await;
                        }
                        Response::InstrumentData(res) => {
                            let payload = res.payload.unwrap();
                            let time_frame = payload.time_frame;
                            let data = payload.data;

                            if is_base_time_frame(&self.time_frame, &time_frame) {
                                self.instrument.set_data(data).unwrap();
                                //MOVE IT!!!!!
                                self.subscribing_to_stream().await;
                            } else {
                                match &mut self.higher_tf_instrument {
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

                            log::info!("{:?} stream data received", data.clone());

                            if is_base_time_frame(&self.time_frame, &time_frame) {
                                let data = adapt_to_time_frame(data, &time_frame);
                                self.instrument.next(data).unwrap();
                            } else {
                                let data = adapt_to_time_frame(data, &self.higher_time_frame);

                                match &mut self.higher_tf_instrument {
                                    HigherTMInstrument::HigherTMInstrument(htf_instrument) => {
                                        htf_instrument.next(data).unwrap();
                                    }
                                    HigherTMInstrument::None => (),
                                };
                            }

                            let (trade_out_result, trade_in_result) = self
                                .strategy
                                .tick(
                                    &self.instrument,
                                    &self.higher_tf_instrument,
                                    &self.trades_in,
                                    &self.trades_out,
                                )
                                .await;

                            match &trade_out_result {
                                TradeResult::TradeOut(trade_out) => {
                                    //Call server for sell

                                    let execute_trade_out = Command {
                                        command: CommandType::ExecuteTrade,
                                        data: Some(trade_out),
                                    };

                                    self.websocket
                                        .send(&serde_json::to_string(&execute_trade_out).unwrap())
                                        .await
                                        .unwrap();
                                }
                                _ => (),
                            };

                            match &trade_in_result {
                                TradeResult::TradeIn(trade_in) => {
                                    let execute_trade_in = Command {
                                        command: CommandType::ExecuteTrade,
                                        data: Some(trade_in),
                                    };

                                    self.websocket
                                        .send(&serde_json::to_string(&execute_trade_in).unwrap())
                                        .await
                                        .unwrap();
                                }
                                _ => (),
                            };

                            //Update stats
                            self.strategy_stats = self.strategy.update_stats(
                                &self.instrument,
                                &self.trades_in,
                                &self.trades_out,
                                0.,
                                0.,
                            );

                            //Send bot data
                            self.send_bot_status().await;
                        }
                        _ => (),
                    };
                }
                Message::Ping(_txt) => {
                    log::info!("Ping received");
                    self.websocket.pong(b"").await;

                    // let ping_command: Command<bool> = Command {
                    //     command: CommandType::Leches,
                    //     data: None,
                    // };
                    // self.websocket
                    //     .send(&serde_json::to_string(&ping_command).unwrap())
                    //     .await
                    //     .unwrap();
                }
                _ => panic!("Unexpected response type!"),
            };
        }
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
        self.symbol = Some(val);
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
            let instrument = Instrument::new()
                .symbol(&symbol)
                .market(market.to_owned())
                .time_frame(time_frame.to_owned())
                .build()
                .unwrap();

            Ok(Bot {
                uuid: uuid::Uuid::new(),
                symbol,
                market,
                time_frame,
                date_start: to_dbtime(Local::now()),
                last_update: to_dbtime(Local::now()),
                higher_time_frame,
                websocket,
                instrument,
                higher_tf_instrument: HigherTMInstrument::None,
                trades_in: vec![],
                trades_out: vec![],
                strategy: set_strategy(&strategy_name),
                strategy_name: strategy_name.clone(),
                strategy_type,
                strategy_stats: StrategyStats::new(),
            })
        } else {
            Err(RsAlgoError {
                err: RsAlgoErrorKind::WrongInstrumentConf,
            })
        }
    }
}
