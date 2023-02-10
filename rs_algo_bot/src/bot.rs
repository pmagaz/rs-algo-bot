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
use rs_algo_shared::models::order::{Order, OrderType};
use rs_algo_shared::models::pricing::Pricing;
use rs_algo_shared::models::strategy::StrategyStats;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::models::{market::*, order};
use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::{HTFInstrument, Instrument};
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
    #[serde(skip_serializing)]
    pricing: Pricing,
    strategy_name: String,
    strategy_type: StrategyType,
    time_frame: TimeFrameType,
    higher_time_frame: Option<TimeFrameType>,
    date_start: DbDateTime,
    last_update: DbDateTime,
    instrument: Instrument,
    htf_instrument: HTFInstrument,
    trades_in: Vec<TradeIn>,
    trades_out: Vec<TradeOut>,
    orders: Vec<Order>,
    strategy_stats: StrategyStats,
    #[serde(skip_serializing)]
    strategy: Box<dyn Strategy>,
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
        log::info!(
            "Creating session for {}_{}_{}_{}",
            &self.symbol,
            &self.strategy_name,
            &self.time_frame,
            &self.strategy_type
        );

        self.uuid = self.generate_bot_uuid();

        log::info!("Session uuid: {}", &self.uuid);

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

        let higher_time_frame = match &self.higher_time_frame {
            Some(htf) => htf,
            None => &TimeFrameType::ERR,
        };

        if is_multi_timeframe_strategy(&self.strategy_type) {
            let get_higher_instrument_data = Command {
                command: CommandType::GetInstrumentData,
                data: Some(Payload {
                    symbol: &self.symbol,
                    strategy: &self.strategy_name,
                    strategy_type: self.strategy_type.to_owned(),
                    time_frame: higher_time_frame.to_owned(),
                }),
            };

            log::info!("Requesting {} {} data", &self.symbol, &higher_time_frame);

            self.websocket
                .send(&serde_json::to_string(&get_higher_instrument_data).unwrap())
                .await
                .unwrap();
        }
    }

    pub async fn get_pricing_data(&mut self) {
        log::info!("Requesting {} pricing data", &self.symbol,);

        let instrument_pricing_data = Command {
            command: CommandType::GetInstrumentPricing,
            data: Some(&self),
        };

        self.websocket
            .send(&serde_json::to_string(&instrument_pricing_data).unwrap())
            .await
            .unwrap();
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
        let max_historical_positions = env::var("MAX_HISTORICAL_POSITIONS")
            .unwrap()
            .parse::<usize>()
            .unwrap();

        self.strategy_stats = data.strategy_stats().clone();

        self.trades_in = data
            .trades_in()
            .iter()
            .rev()
            .take(max_historical_positions)
            .map(|x| x.clone())
            .collect();

        self.trades_out = data
            .trades_out()
            .iter()
            .rev()
            .take(max_historical_positions)
            .map(|x| x.clone())
            .collect();

        self.orders = data
            .orders()
            .iter()
            .rev()
            .take(max_historical_positions)
            .map(|x| x.clone())
            .collect();
    }

    pub async fn send_position<T>(&mut self, trade: &T, symbol: String, time_frame: TimeFrameType)
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        log::info!("Sending position {}_{}", symbol, time_frame);
        let execute_trade = Command {
            command: CommandType::ExecutePosition,
            data: Some(TradeData {
                symbol,
                //time_frame,
                data: trade,
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&execute_trade).unwrap())
            .await
            .unwrap();
    }

    pub async fn send_bot_status(&mut self, bot_str: &str) {
        log::info!("Sending {} bot data", bot_str);

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
        let mut open_positions = false;
        let bot_str = [&self.symbol, "_", &self.time_frame.to_string()].concat();
        let overwrite_orders = env::var("OVERWRITE_ORDERS")
            .unwrap()
            .parse::<bool>()
            .unwrap();

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
                            let orders = bot_data.orders();
                            let pending_orders = order::get_pending(&orders);
                            let active_stop_losses: Vec<Order> = pending_orders
                                .iter()
                                .filter(|x| x.order_type.is_stop())
                                .map(|x| x.clone())
                                .collect();
                            let num_active_trades = trades_in - trades_out;
                            match trades_in.cmp(&trades_out) {
                                Ordering::Greater => {
                                    log::info!("{} trades found", num_active_trades);
                                    //open_positions = true
                                }
                                _ => {
                                    log::info!("No trades found");
                                    //open_positions = false
                                }
                            };

                            match active_stop_losses.len().cmp(&0) {
                                Ordering::Greater => {
                                    log::info!(
                                        "{} active stop losses found",
                                        active_stop_losses.len()
                                    );
                                    open_positions = true
                                }
                                _ => {
                                    log::info!("No active stop losses");
                                    open_positions = false
                                }
                            };

                            self.restore_values(bot_data).await;
                            self.get_instrument_data().await;
                            self.get_pricing_data().await;
                        }
                        MessageType::PricingData(res) => {
                            let pricing = res.payload.unwrap();
                            log::info!(
                                "{} pricing received: {:?}",
                                self.symbol,
                                (pricing.ask(), pricing.bid(), pricing.spread())
                            );
                            self.pricing = pricing;
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
                            } else if is_multi_timeframe_strategy(&self.strategy_type) {
                                match self.htf_instrument {
                                    HTFInstrument::HTFInstrument(ref mut htf_instrument) => {
                                        log::info!(
                                            "Instrument {}_{} data received",
                                            &self.symbol,
                                            &htf_instrument.time_frame()
                                        );

                                        htf_instrument.set_data(data).unwrap();

                                        self.subscribing_to_stream().await;
                                    }
                                    HTFInstrument::None => {}
                                };
                            }

                            self.send_bot_status(&bot_str).await;
                        }
                        MessageType::StreamResponse(res) => {
                            let payload = res.payload.unwrap();
                            let data = payload.data;

                            let new_candle = self.instrument.next(data).unwrap();
                            let mut higher_candle: Candle = new_candle.clone();

                            log::info!("{} candle processed", bot_str);

                            if is_multi_timeframe_strategy(&self.strategy_type) {
                                match self.htf_instrument {
                                    HTFInstrument::HTFInstrument(ref mut htf_instrument) => {
                                        higher_candle = htf_instrument.next(data).unwrap();
                                    }
                                    HTFInstrument::None => (),
                                };
                            }

                            let (position_result, orders_position_result) = self
                                .strategy
                                .tick(
                                    &self.instrument,
                                    &self.htf_instrument,
                                    &self.trades_in,
                                    &self.trades_out,
                                    &self.orders,
                                    &self.pricing,
                                )
                                .await;

                            match new_candle.is_closed() {
                                true => {
                                    self.instrument.init_candle(data);
                                }
                                false => (),
                            };

                            if is_multi_timeframe_strategy(&self.strategy_type)
                                && higher_candle.is_closed()
                            {
                                match self.htf_instrument {
                                    HTFInstrument::HTFInstrument(ref mut htf_instrument) => {
                                        let htf_data = (
                                            higher_candle.date(),
                                            higher_candle.open(),
                                            higher_candle.high(),
                                            higher_candle.low(),
                                            higher_candle.close(),
                                            higher_candle.volume(),
                                        );
                                        htf_instrument.init_candle(htf_data);
                                    }
                                    HTFInstrument::None => (),
                                };
                            }

                            // if is_multi_timeframe_strategy(&self.strategy_type) {
                            //     match self.htf_instrument {
                            //         HTFInstrument::HTFInstrument(ref mut htf_instrument) => {
                            //             htf_instrument.init_candle(data);
                            //         }
                            //         HTFInstrument::None => (),
                            //     };
                            // }

                            match &orders_position_result {
                                PositionResult::MarketInOrder(
                                    TradeResult::TradeIn(trade_in),
                                    order,
                                ) => {
                                    if !open_positions {
                                        log::info!("Position Result MarketInOrder");
                                        self.send_position::<PositionResult>(
                                            &orders_position_result,
                                            self.symbol.clone(),
                                            self.time_frame.clone(),
                                        )
                                        .await;

                                        order::fulfill_bot_order::<TradeIn>(
                                            &trade_in,
                                            &order,
                                            &mut self.orders,
                                        );

                                        //temp
                                        open_positions = true;
                                    }
                                }
                                PositionResult::MarketOutOrder(
                                    TradeResult::TradeOut(trade_out),
                                    order,
                                ) => {
                                    if open_positions {
                                        log::info!("Position Result MarketOutOrder");

                                        self.send_position::<PositionResult>(
                                            &orders_position_result,
                                            self.symbol.clone(),
                                            self.time_frame.clone(),
                                        )
                                        .await;

                                        order::fulfill_bot_order::<TradeOut>(
                                            &trade_out,
                                            &order,
                                            &mut self.orders,
                                        );

                                        self.orders = order::cancel_trade_pending_orders(
                                            trade_out,
                                            self.orders.clone(),
                                        );

                                        //temp
                                        open_positions = false;
                                    }
                                }
                                _ => (),
                            };

                            match &position_result {
                                PositionResult::MarketIn(
                                    TradeResult::TradeIn(trade_in),
                                    new_orders,
                                ) => {
                                    if !open_positions {
                                        log::info!("Position Result TradeIn");

                                        self.send_position::<PositionResult>(
                                            &position_result,
                                            self.symbol.clone(),
                                            self.time_frame.clone(),
                                        )
                                        .await;

                                        //open_positions = true;
                                        //self.trades_in.push(trade_in.clone());

                                        match new_orders {
                                            Some(new_ords) => {
                                                self.orders = order::add_pending(
                                                    self.orders.clone(),
                                                    new_ords.clone(),
                                                )
                                            }
                                            None => (),
                                        }
                                    }
                                }
                                PositionResult::MarketOut(TradeResult::TradeOut(trade_out)) => {
                                    if open_positions {
                                        log::info!("Position Result TradeOut");
                                        self.send_position::<PositionResult>(
                                            &position_result,
                                            self.symbol.clone(),
                                            self.time_frame.clone(),
                                        )
                                        .await;

                                        self.orders = order::cancel_trade_pending_orders(
                                            trade_out,
                                            self.orders.clone(),
                                        );

                                        //open_positions = false;
                                        //self.trades_out.push(trade_out.clone());
                                    }
                                }
                                PositionResult::PendingOrder(new_orders) => {
                                    if !open_positions {
                                        log::info!("New Pending orders Result");
                                        match overwrite_orders {
                                            true => {
                                                self.orders = order::cancel_all_bot_pending_orders(
                                                    self.orders.clone(),
                                                );
                                            }
                                            false => (),
                                        }

                                        self.orders = order::add_pending(
                                            self.orders.clone(),
                                            new_orders.clone(),
                                        );
                                    }
                                }
                                _ => (),
                            };

                            log::info!("Total orders {:?}", self.orders.len());

                            self.get_pricing_data().await;
                            self.send_bot_status(&bot_str).await;
                        }
                        MessageType::ExecuteTradeIn(res) => {
                            let trade_in = res.payload.unwrap();
                            self.trades_in.push(trade_in.data);

                            self.strategy_stats = self.strategy.update_stats(
                                &self.instrument,
                                &self.trades_in,
                                &self.trades_out,
                            );

                            open_positions = true;
                            self.send_bot_status(&bot_str).await;
                        }
                        MessageType::ExecuteTradeOut(res) => {
                            let trade_out = res.payload.unwrap();
                            log::info!(
                                "TradeOut {} accepted ask {} bid {}",
                                &trade_out.data.id,
                                &trade_out.data.ask,
                                &trade_out.data.bid
                            );

                            let updated_trade_out = self.strategy.update_trade_stats(
                                self.trades_in.last().unwrap(),
                                &trade_out.data,
                                &self.instrument.data,
                                &self.pricing,
                            );

                            log::info!(
                                "TradeOut stats profit {} profit_per {} ",
                                &updated_trade_out.profit,
                                &updated_trade_out.profit_per,
                            );

                            self.trades_out.push(updated_trade_out);

                            self.strategy_stats = self.strategy.update_stats(
                                &self.instrument,
                                &self.trades_in,
                                &self.trades_out,
                            );

                            open_positions = false;
                            self.send_bot_status(&bot_str).await;
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
            self.higher_time_frame.clone(),
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

            let htf_instrument = match self.strategy_type {
                Some(strategy) => match strategy {
                    StrategyType::OnlyLongMTF => HTFInstrument::HTFInstrument(htf_instrument),
                    StrategyType::OnlyShortMTF => HTFInstrument::HTFInstrument(htf_instrument),
                    StrategyType::LongShortMTF => HTFInstrument::HTFInstrument(htf_instrument),
                    _ => HTFInstrument::None,
                },
                None => HTFInstrument::None,
            };

            // let higher_time_frame = match &self.higher_time_frame {
            //     Some(htf) => htf,
            //     None => &TimeFrameType::ERR,
            // };

            Ok(Bot {
                uuid: uuid::Uuid::new(),
                symbol,
                market,
                pricing: Pricing::default(),
                time_frame,
                higher_time_frame: self.higher_time_frame,
                date_start: to_dbtime(Local::now()),
                last_update: to_dbtime(Local::now()),
                websocket,
                instrument,
                htf_instrument,
                trades_in: vec![],
                trades_out: vec![],
                orders: vec![],
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
