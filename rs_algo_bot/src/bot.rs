use std::cmp::Ordering;
use std::env;

use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::helpers::vars::*;
use crate::message;

use crate::strategies::strategy::*;

use futures::Future;
use rs_algo_shared::helpers::date::Local;
use rs_algo_shared::helpers::uuid::*;
use rs_algo_shared::helpers::{date::*, uuid};
use rs_algo_shared::models::bot::BotData;
use rs_algo_shared::models::mode::ExecutionMode;
use rs_algo_shared::models::order::{Order, OrderStatus};
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
use std::time::Duration;
use tokio::time::sleep;

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
            &self.higher_time_frame.clone().unwrap().to_string(),
            &self.strategy_type.to_string(),
        ];

        uuid::generate(seed)
    }

    pub async fn retry<F, T>(&mut self, seconds: u64, mut callback: F)
    where
        F: Send + FnMut() -> T,
        T: Future + Send + 'static,
    {
        sleep(Duration::from_secs(seconds)).await;
        callback().await;
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
        let num_bars = env::var("NUM_BARS").unwrap().parse::<i64>().unwrap();
        let time_frame_from =
            TimeFrame::get_starting_bar(num_bars, &self.time_frame, &ExecutionMode::Bot);

        log::info!(
            "Requesting {}_{} data from {:?}",
            &self.symbol,
            &self.time_frame,
            time_frame_from
        );
        let get_instrument_data = Command {
            command: CommandType::GetInstrumentData,
            data: Some(InstrumentDataPayload {
                symbol: &self.symbol,
                strategy: &self.strategy_name,
                time_frame: self.time_frame.to_owned(),
                strategy_type: self.strategy_type.to_owned(),
                num_bars,
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

        if is_mtf_strategy(&self.strategy_type) {
            let get_higher_instrument_data = Command {
                command: CommandType::GetInstrumentData,
                data: Some(InstrumentDataPayload {
                    symbol: &self.symbol,
                    strategy: &self.strategy_name,
                    strategy_type: self.strategy_type.to_owned(),
                    time_frame: higher_time_frame.to_owned(),
                    num_bars,
                }),
            };

            self.websocket
                .send(&serde_json::to_string(&get_higher_instrument_data).unwrap())
                .await
                .unwrap();

            log::info!(
                "Requesting HTF {}_{} data from {:?}",
                &self.symbol,
                &higher_time_frame,
                time_frame_from
            );
        }
    }

    pub async fn get_pricing_data(&mut self) {
        //log::info!("Requesting {} pricing data", &self.symbol,);

        let instrument_pricing_data = Command {
            command: CommandType::GetInstrumentPricing,
            data: Some(Symbol {
                symbol: self.symbol.to_owned(),
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&instrument_pricing_data).unwrap())
            .await
            .unwrap();
    }

    pub async fn is_market_open(&mut self) {
        log::info!("Checking {} market is open...", &self.symbol,);

        let instrument_pricing_data = Command {
            command: CommandType::GetMarketHours,
            data: Some(Symbol {
                symbol: self.symbol.to_owned(),
            }),
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
            .cloned()
            .collect();

        self.trades_out = data
            .trades_out()
            .iter()
            .rev()
            .take(max_historical_positions)
            .cloned()
            .collect();

        self.orders = data
            .orders()
            .iter()
            .filter(|x| !x.is_pending())
            .rev()
            .take(max_historical_positions)
            .cloned()
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
                data: trade,
                options: TradeOptions {
                    non_profitable_out: env::var("NON_PROFITABLE_OUTS")
                        .unwrap()
                        .parse::<bool>()
                        .unwrap(),
                },
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&execute_trade).unwrap())
            .await
            .unwrap();
    }

    pub async fn send_bot_status(&mut self, _bot_str: &str) {
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

    pub async fn reconnect(&mut self) {
        let secs = env::var("DISCONNECTED_RETRY")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        log::info!("Reconnecting in {} secs...", secs);

        sleep(Duration::from_secs(secs)).await;
        self.websocket.re_connect().await;
        self.init_session().await;
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
            match self.websocket.read_msg().await {
                Ok(msg) => {
                    match msg {
                        Message::Text(txt) => {
                            let msg_type = message::get_type(&txt);

                            match msg_type {
                                MessageType::Connected(_res) => {
                                    log::info!("{} connected to server", bot_str);
                                }
                                MessageType::Reconnect(_res) => {
                                    log::info!("{} reconnect msg received", bot_str);
                                    self.reconnect().await;
                                }
                                MessageType::InitSession(res) => {
                                    log::info!("Getting {} previous session", bot_str);

                                    let now = Local::now();
                                    let bot_data = res.payload.unwrap();
                                    let trades_in = bot_data.trades_in().len();
                                    let trades_out = bot_data.trades_out().len();
                                    let orders = bot_data.orders();
                                    let pending_orders = order::get_pending(orders);

                                    let num_active_trades = trades_in - trades_out;
                                    match trades_in.cmp(&trades_out) {
                                        Ordering::Greater => {
                                            log::info!("{} trades found", num_active_trades);
                                            open_positions = true
                                        }
                                        _ => {
                                            log::info!("No trades found");
                                            //open_positions = false
                                        }
                                    };

                                    let active_stop_losses: Vec<Order> = pending_orders
                                        .iter()
                                        .filter(|x| {
                                            open_positions
                                                && x.order_type.is_stop()
                                                && x.is_still_valid(now)
                                                && x.status == OrderStatus::Pending
                                        })
                                        .cloned()
                                        .collect();

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
                                    self.is_market_open().await;
                                }
                                MessageType::MarketHours(res) => {
                                    let market_hours = res.payload.unwrap();
                                    let open = market_hours.open();

                                    match market_hours.open() {
                                        true => {
                                            log::info!("{} market open: {}", self.symbol, open);
                                            self.get_instrument_data().await;
                                            self.get_pricing_data().await;
                                        }
                                        false => {
                                            let secs_to_retry = env::var("MARKET_CLOSED_RETRY")
                                                .unwrap()
                                                .parse::<u64>()
                                                .unwrap();

                                            log::warn!(
                                                "{} market closed!. Retrying after {} secs...",
                                                self.symbol,
                                                secs_to_retry
                                            );

                                            sleep(Duration::from_secs(secs_to_retry)).await;
                                            self.is_market_open().await;
                                        }
                                    }
                                }
                                MessageType::PricingData(res) => {
                                    let pricing = res.payload.unwrap();
                                    self.pricing = pricing;
                                }
                                MessageType::InstrumentData(res) => {
                                    let payload = res.payload.unwrap();
                                    let time_frame = payload.time_frame;
                                    let data = payload.data;
                                    let since_date = match &data.first() {
                                        Some(x) => x.0.to_string(),
                                        None => "".to_string(),
                                    };

                                    if is_base_time_frame(&self.time_frame, &time_frame) {
                                        log::info!(
                                            "Instrument {} data received from {:?}",
                                            bot_str,
                                            &since_date,
                                        );

                                        self.instrument.set_data(data).unwrap();

                                        if !is_mtf_strategy(&self.strategy_type) {
                                            self.subscribing_to_stream().await;
                                        }
                                    } else if is_mtf_strategy(&self.strategy_type) {
                                        match self.htf_instrument {
                                            HTFInstrument::HTFInstrument(
                                                ref mut htf_instrument,
                                            ) => {
                                                let since_date = match &htf_instrument.data.first()
                                                {
                                                    Some(x) => x.date().to_string(),
                                                    None => "".to_owned(),
                                                };
                                                log::info!(
                                                    "Instrument {}_{} data received from {:?}",
                                                    &self.symbol,
                                                    &htf_instrument.time_frame(),
                                                    &since_date
                                                );

                                                htf_instrument.set_data(data).unwrap();

                                                self.subscribing_to_stream().await;
                                            }
                                            HTFInstrument::None => {}
                                        };
                                    }

                                    //TODO REVIEW ACTIVE ORDERS FOR GAPS
                                    self.send_bot_status(&bot_str).await;
                                }
                                MessageType::StreamResponse(res) => {
                                    let payload = res.payload.unwrap();
                                    let data = payload.data;
                                    let index = &self.instrument.data.len() - 1;
                                    let new_candle = self.instrument.next(data).unwrap();
                                    let mut higher_candle: Candle = new_candle.clone();

                                    if is_mtf_strategy(&self.strategy_type) {
                                        match self.htf_instrument {
                                            HTFInstrument::HTFInstrument(
                                                ref mut htf_instrument,
                                            ) => {
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
                                            self.instrument
                                                .init_candle(data, &Some(self.time_frame.clone()));
                                        }
                                        false => (),
                                    };

                                    if higher_candle.is_closed()
                                        && is_mtf_strategy(&self.strategy_type)
                                    {
                                        log::info!("HTF Candle closed {:?}", higher_candle.date());
                                        match self.htf_instrument {
                                            HTFInstrument::HTFInstrument(
                                                ref mut htf_instrument,
                                            ) => {
                                                let htf_data = (
                                                    higher_candle.date(),
                                                    higher_candle.open(),
                                                    higher_candle.high(),
                                                    higher_candle.low(),
                                                    higher_candle.close(),
                                                    higher_candle.volume(),
                                                );
                                                htf_instrument
                                                    .init_candle(htf_data, &self.higher_time_frame);
                                            }
                                            HTFInstrument::None => (),
                                        };
                                    }

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
                                                    trade_in,
                                                    order,
                                                    &mut self.orders,
                                                    &self.instrument,
                                                );

                                                order::extend_all_pending_orders(&mut self.orders);
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
                                                    trade_out,
                                                    order,
                                                    &mut self.orders,
                                                    &self.instrument,
                                                );

                                                // order::cancel_trade_pending_orders(
                                                //     trade_out,
                                                //     &mut self.orders,
                                                // );
                                            }
                                        }
                                        _ => (),
                                    };

                                    match &position_result {
                                        PositionResult::MarketIn(
                                            TradeResult::TradeIn(_trade_in),
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
                                        PositionResult::MarketOut(TradeResult::TradeOut(
                                            trade_out,
                                        )) => {
                                            if open_positions {
                                                log::info!("Position Result TradeOut");
                                                self.send_position::<PositionResult>(
                                                    &position_result,
                                                    self.symbol.clone(),
                                                    self.time_frame.clone(),
                                                )
                                                .await;

                                                // order::cancel_trade_pending_orders(
                                                //     trade_out,
                                                //     &mut self.orders,
                                                // );
                                            }
                                        }
                                        PositionResult::PendingOrder(new_orders) => {
                                            log::info!("OPEN POSITIONS {:?}", &open_positions);
                                            if !open_positions {
                                                match overwrite_orders {
                                                    true => {
                                                        log::info!(
                                                            "OVERWRITING ORDERS {:?}",
                                                            &self.orders.len()
                                                        );

                                                        // order::cancel_pending_expired_orders(
                                                        //     0,
                                                        //     &self.instrument,
                                                        //     &mut self.orders,
                                                        // );
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

                                    self.orders = order::cancel_pending_expired_orders(
                                        index,
                                        &self.instrument,
                                        &mut self.orders,
                                    );
                                    //self.get_pricing_data().await;
                                    self.send_bot_status(&bot_str).await;
                                }
                                MessageType::StreamPricingResponse(res) => {
                                    let current_pip_size = self.pricing.pip_size();
                                    let current_percentage = self.pricing.percentage();
                                    let pricing = res.payload.unwrap();
                                    self.pricing = Pricing::new(
                                        pricing.symbol(),
                                        pricing.ask(),
                                        pricing.bid(),
                                        pricing.spread(),
                                        current_pip_size,
                                        current_percentage,
                                    );
                                }
                                MessageType::ExecuteTradeIn(res) => {
                                    let payload = res.payload.unwrap();
                                    let accepted = &payload.accepted;

                                    match accepted {
                                        true => {
                                            log::info!(
                                                "{:?} {} accepted!",
                                                &payload.data.trade_type,
                                                &payload.data.id
                                            );

                                            let trade_response = payload;
                                            self.trades_in.push(trade_response.data);

                                            self.strategy_stats = self.strategy.update_stats(
                                                &self.instrument,
                                                &self.trades_in,
                                                &self.trades_out,
                                            );

                                            open_positions = true;
                                            self.send_bot_status(&bot_str).await;
                                        }
                                        false => {
                                            log::warn!(
                                                "{:?} {} not accepted!",
                                                &payload.data.trade_type,
                                                &payload.data.id
                                            );
                                        }
                                    }
                                }
                                MessageType::ExecuteTradeOut(res) => {
                                    let payload = res.payload.unwrap();
                                    let accepted = &payload.accepted;

                                    match accepted {
                                        true => {
                                            log::info!(
                                                "{:?} {} accepted ask: {} bid: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                                &payload.data.bid,
                                            );

                                            log::info!("TRADE OUT {:?}", (payload.data));
                                            let trade_response = payload;

                                            let updated_trade_out =
                                                self.strategy.update_trade_stats(
                                                    self.trades_in.last().unwrap(),
                                                    &trade_response.data,
                                                    &self.instrument.data,
                                                    &self.pricing,
                                                );

                                            order::cancel_trade_pending_orders(
                                                &updated_trade_out,
                                                &mut self.orders,
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
                                        false => {
                                            log::warn!(
                                                "{:?} {} not accepted ask: {} bid: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                                &payload.data.bid
                                            );
                                        }
                                    };
                                }
                                _ => (),
                            };
                        }
                        Message::Ping(_txt) => {
                            self.websocket.pong(b"").await;
                        }
                        _ => panic!("Unexpected response type!"),
                    };
                }
                Err(err) => {
                    log::warn!("[ERROR] Disconnected from server: {:?}", err);
                    self.reconnect().await;
                    //Message::Ping(b"".to_vec())
                }
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
                .unwrap();

            let htf_instrument = Instrument::new()
                .symbol(&symbol)
                .market(market.to_owned())
                .time_frame(higher_time_frame)
                .build()
                .unwrap();

            let htf_instrument = match self.strategy_type {
                Some(strategy) => match strategy {
                    StrategyType::OnlyLongMTF
                    | StrategyType::OnlyShortMTF
                    | StrategyType::LongShortMTF => HTFInstrument::HTFInstrument(htf_instrument),
                    _ => HTFInstrument::None,
                },
                None => HTFInstrument::None,
            };

            // let higher_time_frame = match &self.higher_time_frame {
            //     Some(htf) => htf,
            //     None => &TimeFrameType::ERR,
            // };

            let strategy = set_strategy(
                &strategy_name,
                &time_frame.to_string(),
                Some(&self.higher_time_frame.clone().unwrap().to_string()),
                strategy_type.clone(),
            );

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
                strategy,
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
