use std::cmp::Ordering;
use std::env;

use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::helpers::vars::*;
use crate::message;

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
use serde::{Deserialize, Serialize};

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

        if is_multi_timeframe_strategy(&self.strategy_type) {
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
        log::info!(
            "Subscribing to {}_{} stream",
            &self.symbol,
            &self.time_frame
        );

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
        self.trades_in = data.trades_in().clone();
        self.trades_out = data.trades_out().clone();
        self.strategy_stats = data.strategy_stats().clone();
    }

    pub async fn send_trade<T>(&mut self, trade: &T, symbol: String, time_frame: TimeFrameType)
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        let execute_trade = Command {
            command: CommandType::ExecuteTrade,
            data: Some(TradeData {
                symbol,
                time_frame,
                data: trade,
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&execute_trade).unwrap())
            .await
            .unwrap();
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
        let bot_str = [&self.symbol, "_", &self.time_frame.to_string()].concat();

        loop {
            let msg = self.websocket.read().await.unwrap();
            match msg {
                Message::Text(txt) => {
                    let msg_type = message::get_type(&txt);

                    match msg_type {
                        MessageType::Connected(_res) => {
                            log::info!("{} connected to server", bot_str);
                        }
                        MessageType::InitSession(res) => {
                            log::info!("Getting {} previous session", bot_str);

                            let bot_data = res.payload.unwrap();
                            let trades_in = bot_data.trades_in().len();
                            let trades_out = bot_data.trades_out().len();
                            let num_active_trades = trades_in - trades_out;
                            match trades_in.cmp(&trades_out) {
                                Ordering::Greater => {
                                    log::info!("{} opened trades found", num_active_trades);
                                    true
                                }
                                _ => {
                                    log::info!("No opened trades found");
                                    false
                                }
                            };

                            self.restore_values(bot_data).await;
                            self.get_instrument_data().await;
                        }
                        MessageType::InstrumentData(res) => {
                            let payload = res.payload.unwrap();
                            let time_frame = payload.time_frame;
                            let data = payload.data;

                            if is_base_time_frame(&self.time_frame, &time_frame) {
                                log::info!("Instrument {} data received", bot_str);
                                self.instrument.set_data(data).unwrap();

                                if !is_multi_timeframe_strategy(&self.strategy_type) {
                                    self.subscribing_to_stream().await;
                                }
                            } else {
                                if is_multi_timeframe_strategy(&self.strategy_type) {
                                    match self.higher_tf_instrument.clone() {
                                        HigherTMInstrument::HigherTMInstrument(
                                            mut htf_instrument,
                                        ) => {
                                            log::info!(
                                                "Instrument {}_{} data received",
                                                &self.symbol,
                                                &self.higher_time_frame
                                            );

                                            htf_instrument.set_data(data).unwrap();
                                            self.subscribing_to_stream().await;
                                        }
                                        HigherTMInstrument::None => {}
                                    };
                                }
                            }
                        }
                        MessageType::StreamResponse(res) => {
                            let payload = res.payload.unwrap();
                            let data = payload.data;

                            let last_candle = self.instrument.data().last().unwrap().clone();
                            let adapted = adapt_to_time_frame(data, &self.time_frame);
                            self.instrument.next(adapted, &last_candle).unwrap();

                            log::info!("{} candle processed", bot_str);

                            if is_multi_timeframe_strategy(&self.strategy_type) {
                                match &mut self.higher_tf_instrument {
                                    HigherTMInstrument::HigherTMInstrument(htf_instrument) => {
                                        let last_htf_candle =
                                            htf_instrument.data().last().unwrap().clone();
                                        let adapted =
                                            adapt_to_time_frame(data, &self.higher_time_frame);
                                        htf_instrument.next(adapted, &last_htf_candle).unwrap();

                                        log::info!(
                                            "{}_{} candle processed",
                                            &self.symbol,
                                            &self.higher_time_frame
                                        );
                                    }
                                    HigherTMInstrument::None => (),
                                };

                                log::info!("Processed {} data", bot_str);
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
                                    self.send_trade::<TradeOut>(
                                        trade_out,
                                        self.symbol.clone(),
                                        self.time_frame.clone(),
                                    )
                                    .await;
                                }
                                _ => (),
                            };

                            match &trade_in_result {
                                TradeResult::TradeIn(trade_in) => {
                                    self.send_trade::<TradeIn>(
                                        trade_in,
                                        self.symbol.clone(),
                                        self.time_frame.clone(),
                                    )
                                    .await;
                                }
                                _ => (),
                            };

                            log::info!("Sending {} bot data", bot_str);

                            self.send_bot_status().await;
                        }
                        MessageType::ExecuteTradeIn(res) => {
                            let trade_in = res.payload.unwrap();
                            log::info!("TradeIn {} accepted", &trade_in.data.id);
                            self.trades_in.push(trade_in.data);

                            self.strategy_stats = self.strategy.update_stats(
                                &self.instrument,
                                &self.trades_in,
                                &self.trades_out,
                            );

                            self.send_bot_status().await;
                        }
                        MessageType::ExecuteTradeOut(res) => {
                            let trade_out = res.payload.unwrap();
                            log::info!("TradeOut {} accepted", &trade_out.data.id);

                            let updated_trade_out = self.strategy.update_trade_stats(
                                &self.trades_in.last().unwrap(),
                                &trade_out.data,
                                &self.instrument.data,
                            );

                            self.trades_out.push(updated_trade_out);

                            self.strategy_stats = self.strategy.update_stats(
                                &self.instrument,
                                &self.trades_in,
                                &self.trades_out,
                            );

                            self.send_bot_status().await;
                        }
                        _ => (),
                    };
                }
                Message::Ping(_txt) => {
                    //log::info!("HeartBeat received");
                    self.websocket.pong(b"").await;
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
            self.strategy_type.clone(),
            self.websocket,
        ) {
            let instrument = Instrument::new()
                .symbol(&symbol)
                .market(market.to_owned())
                .time_frame(time_frame.to_owned())
                .build()
                .unwrap()
                .init();

            let htf_instrument = Instrument::new()
                .symbol(&symbol)
                .market(market.to_owned())
                .time_frame(higher_time_frame.to_owned())
                .build()
                .unwrap()
                .init();

            let higher_tf_instrument = match self.strategy_type {
                Some(strategy) => match strategy {
                    StrategyType::OnlyLongMultiTF => {
                        HigherTMInstrument::HigherTMInstrument(htf_instrument)
                    }
                    StrategyType::OnlyShortMultiTF => {
                        HigherTMInstrument::HigherTMInstrument(htf_instrument)
                    }
                    StrategyType::LongShortMultiTF => {
                        HigherTMInstrument::HigherTMInstrument(htf_instrument)
                    }
                    _ => HigherTMInstrument::None,
                },
                None => HigherTMInstrument::None,
            };

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
                higher_tf_instrument,
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
