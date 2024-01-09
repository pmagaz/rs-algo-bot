use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::helpers::vars::*;
use crate::message;
use crate::strategies::strategy::*;

use rs_algo_shared::helpers::date::{Local, Timelike};
use rs_algo_shared::helpers::http::{request, HttpMethod};
use rs_algo_shared::helpers::uuid::*;
use rs_algo_shared::helpers::{date::*, uuid};
use rs_algo_shared::models::environment::{self, Environment};
use rs_algo_shared::models::order::{Order, OrderStatus};
use rs_algo_shared::models::tick::InstrumentTick;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::models::{market::*, order};
use rs_algo_shared::models::{strategy::*, trade};
use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::{HTFInstrument, Instrument};
use rs_algo_shared::ws::message::*;
use rs_algo_shared::ws::ws_client::WebSocket;

use futures::Future;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Serialize)]
pub struct Bot {
    env: Environment,
    #[serde(skip_serializing)]
    websocket: WebSocket,
    #[serde(rename = "_id")]
    uuid: uuid::Uuid,
    symbol: String,
    market: Market,
    #[serde(skip_serializing)]
    tick: InstrumentTick,
    #[serde(skip_serializing)]
    market_hours: MarketHours,
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
            &self.env.value(),
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

    pub async fn get_historic_data(&mut self) {
        let time_frame = self.time_frame.clone();

        let initial_limit = env::var("INITIAL_BARS").unwrap().parse::<i64>().unwrap();

        log::info!(
            "Requesting HTF {}_{} historic data",
            &self.symbol,
            &time_frame,
        );

        let historic_command_data = Command {
            command: CommandType::GetHistoricData,
            data: Some(HistoricDataPayload {
                symbol: &self.symbol,
                strategy: &self.strategy_name,
                time_frame: time_frame,
                strategy_type: self.strategy_type.to_owned(),
                limit: initial_limit,
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&historic_command_data).unwrap())
            .await
            .unwrap();

        let higher_time_frame = match &self.higher_time_frame {
            Some(htf) => htf,
            None => &TimeFrameType::ERR,
        };

        if is_mtf_strategy(&self.strategy_type) {
            let get_higher_instrument_data = Command {
                command: CommandType::GetHistoricData,
                data: Some(HistoricDataPayload {
                    symbol: &self.symbol,
                    strategy: &self.strategy_name,
                    strategy_type: self.strategy_type.to_owned(),
                    time_frame: higher_time_frame.to_owned(),
                    limit: initial_limit,
                }),
            };

            self.websocket
                .send(&serde_json::to_string(&get_higher_instrument_data).unwrap())
                .await
                .unwrap();
        }
    }

    pub async fn get_tick_data(&mut self) {
        let instrument_tick_data = Command {
            command: CommandType::GetInstrumentTick,
            data: Some(Symbol {
                symbol: self.symbol.to_owned(),
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&instrument_tick_data).unwrap())
            .await
            .unwrap();
    }

    pub async fn is_market_open(&mut self) {
        log::info!("Checking {} market is open...", &self.symbol,);

        let instrument_pricing_data = Command {
            command: CommandType::IsMarketOpen,
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

    pub async fn fullfill_activated_order<T: Trade>(
        &mut self,
        activated_orders_result: &PositionResult,
        trade: &T,
        order: &Order,
        open_positions: &mut bool,
    ) {
        let should_open = match activated_orders_result {
            PositionResult::MarketInOrder(_, _) => true,
            _ => false,
        };

        if *open_positions != should_open {
            order::fulfill_bot_order::<T>(trade, order, &mut self.orders, &self.instrument);

            *open_positions = should_open;

            log::info!(
                "Sending {} {:?} ...",
                trade.get_index_in(),
                trade.get_type()
            );

            self.send_position::<PositionResult>(
                activated_orders_result,
                self.symbol.clone(),
                self.strategy_name.clone(),
            )
            .await;
        }
    }

    async fn process_trade(
        &mut self,
        new_position_result: &PositionResult,
        associated_orders: Option<&Vec<Order>>,
        open_positions: &mut bool,
    ) {
        match new_position_result {
            PositionResult::MarketIn(TradeResult::TradeIn(trade_in), _) if !*open_positions => {
                log::info!(
                    "Sending {} {:?} ...",
                    trade_in.get_index_in(),
                    trade_in.get_type()
                );

                self.send_position::<PositionResult>(
                    &new_position_result,
                    self.symbol.clone(),
                    self.strategy_name.clone(),
                )
                .await;

                if let Some(new_ords) = associated_orders {
                    self.orders = order::add_pending(self.orders.clone(), new_ords.clone());
                }
                *open_positions = true;
            }
            PositionResult::MarketOut(TradeResult::TradeOut(trade_out)) if *open_positions => {
                log::info!(
                    "Sending {} {:?} ...",
                    trade_out.get_index_in(),
                    trade_out.get_type()
                );

                self.send_position::<PositionResult>(
                    &new_position_result,
                    self.symbol.clone(),
                    self.strategy_name.clone(),
                )
                .await;
                *open_positions = false;
            }
            PositionResult::PendingOrder(associated_orders) if !*open_positions => {
                self.orders = order::add_pending(self.orders.clone(), associated_orders.clone());
            }
            _ => (),
        }
    }

    async fn process_activated_orders(
        &mut self,
        activated_orders_result: &PositionResult,
        open_positions: &mut bool,
    ) {
        match activated_orders_result {
            PositionResult::MarketInOrder(TradeResult::TradeIn(trade_in), order) => {
                self.fullfill_activated_order::<TradeIn>(
                    activated_orders_result,
                    trade_in,
                    order,
                    open_positions,
                )
                .await;

                self.add_trade_in(trade_in);
            }
            PositionResult::MarketOutOrder(TradeResult::TradeOut(trade_out), order) => {
                self.fullfill_activated_order::<TradeOut>(
                    activated_orders_result,
                    trade_out,
                    order,
                    open_positions,
                )
                .await;

                self.add_trade_out(trade_out);
            }
            _ => {
                //log::error!("{:?}", activated_orders_result);
            }
        };
    }

    fn add_trade_in(&mut self, trade_in: &TradeIn) {
        let num_trades_in = self.trades_in.len();
        let num_trades_out = self.trades_out.len();

        let max_buy_orders = env::var("MAX_BUY_ORDERS")
            .unwrap()
            .parse::<usize>()
            .unwrap();

        let has_active_trade = match num_trades_in.checked_sub(num_trades_out) {
            Some(diff) => diff < max_buy_orders,
            None => false,
        };

        if has_active_trade {
            self.trades_in.push(trade_in.clone());
        } else {
            log::error!(
                "Uncontrolled trade in. TradesIn: {} TradesOut: {}",
                num_trades_in,
                num_trades_out
            );
        }
    }

    fn add_trade_out(&mut self, trade_out: &TradeOut) {
        let num_trades_in = self.trades_in.len();
        let num_trades_out = self.trades_out.len();
        let has_active_trade = num_trades_in > num_trades_out;

        if has_active_trade {
            self.trades_out.push(trade_out.clone());
        } else {
            log::error!(
                "Uncontrolled trade out. TradesIn: {} TradesOut: {}",
                num_trades_in,
                num_trades_out
            );
            //panic!();
        }
    }

    async fn process_activated_positions(
        &mut self,
        new_position_result: &PositionResult,
        open_positions: &mut bool,
    ) {
        match new_position_result {
            PositionResult::MarketIn(TradeResult::TradeIn(trade_in), associated_orders) => {
                self.process_trade(
                    new_position_result,
                    associated_orders.as_ref(),
                    open_positions,
                )
                .await;

                self.add_trade_in(trade_in);
            }
            PositionResult::MarketOut(TradeResult::TradeOut(trade_out)) => {
                self.process_trade(new_position_result, None, open_positions)
                    .await;

                self.add_trade_out(trade_out);
            }
            PositionResult::PendingOrder(associated_orders) => {
                if !*open_positions {
                    self.orders =
                        order::add_pending(self.orders.clone(), associated_orders.clone());
                }
            }
            _ => (),
        }
    }

    pub async fn send_position<T>(&mut self, trade: &T, symbol: String, strategy_name: String)
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
        let execute_trade = Command {
            command: CommandType::ExecutePosition,
            data: Some(TradeData {
                symbol,
                strategy_name,
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

    pub async fn get_market_hours(&mut self) {
        log::info!("Checking {} trading hours...", &self.symbol,);
        //sleep(Duration::from_millis(200)).await;
        let data = Command {
            command: CommandType::GetMarketHours,
            data: Some(Symbol {
                symbol: self.symbol.to_owned(),
            }),
        };

        self.websocket
            .send(&serde_json::to_string(&data).unwrap())
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

    pub fn clean_previous_data(&mut self) {
        log::info!("Cleaning {} previous session data", &self.symbol);

        self.trades_in = vec![];
        self.trades_out = vec![];
        self.orders = vec![];
        self.strategy_stats = StrategyStats::new();
    }

    pub async fn set_tick(&mut self) {
        let tick_endpoint = format!(
            "{}{}",
            env::var("BACKEND_BACKTEST_PRICING_ENDPOINT").unwrap(),
            self.symbol
        );

        let tick: InstrumentTick = request(&tick_endpoint, &String::from("all"), HttpMethod::Get)
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        self.tick = tick;
    }

    pub async fn run(&mut self) {
        self.init_session().await;
        self.set_tick().await;
        let mut open_positions = false;
        let bot_str = [&self.symbol, "_", &self.time_frame.to_string()].concat();
        let mut counter = 0;

        loop {
            match self.websocket.read().await {
                Ok(msg) => {
                    match msg {
                        Message::Text(txt) => {
                            let msg_type = message::get_type(&txt);

                            match msg_type {
                                MessageType::Connected(_res) => {
                                    log::info!("{} connected to server", bot_str);
                                }
                                MessageType::Reconnect(_res) => {
                                    log::info!("{} reconnect msg received!", bot_str);
                                    self.reconnect().await;
                                }
                                MessageType::InitSession(res) => {
                                    //self.clean_previous_data();
                                    if self.trades_in.is_empty() {
                                        self.get_market_hours().await;
                                    } else {
                                        panic!("Reeeeestart");
                                    }
                                }
                                MessageType::MarketHours(res) => {
                                    let market_hours = res.payload.unwrap();
                                    let is_trading_hours = true;
                                    log::info!("Trading hours {}", &is_trading_hours);

                                    match is_trading_hours {
                                        true => self.is_market_open().await,
                                        false => {
                                            let will_open_at = market_hours.wait_until();

                                            let wait_until = will_open_at
                                                .signed_duration_since(Local::now())
                                                .num_seconds()
                                                as u64;

                                            log::info!(
                                                "Not in trading hours. Trading available at {}. Waiting {} secs / {} hours",
                                                will_open_at, wait_until, wait_until / 3600
                                            );

                                            sleep(Duration::from_secs(wait_until)).await;
                                            log::info!("{} Reconnecting", bot_str);
                                            self.reconnect().await;
                                        }
                                    };

                                    self.market_hours = market_hours;
                                }
                                MessageType::IsMarketOpen(res) => {
                                    let is_market_open = true;
                                    match is_market_open {
                                        true => {
                                            log::info!("{} Market open!", self.symbol);
                                            self.get_historic_data().await;
                                            self.get_tick_data().await;
                                        }
                                        false => {
                                            let secs_to_retry = env::var("MARKET_CLOSED_RETRY")
                                                .unwrap()
                                                .parse::<u64>()
                                                .unwrap();

                                            log::warn!(
                                                "{} Market closed!. Retrying after {} secs...",
                                                self.symbol,
                                                secs_to_retry
                                            );

                                            sleep(Duration::from_secs(secs_to_retry)).await;
                                            self.get_market_hours().await;
                                        }
                                    }
                                }
                                MessageType::StreamTickResponse(res)
                                | MessageType::InstrumentTick(res) => {}
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
                                                    "Instrument {}_{} HTF data received from {:?}",
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
                                    self.send_bot_status(&bot_str).await;
                                }
                                MessageType::StreamResponse(res) => {
                                    let payload = res.payload.unwrap();
                                    let data = payload.data;
                                    let index = self.instrument.data.len().checked_sub(1).unwrap();
                                    let new_candle = self.instrument.next(data).unwrap();
                                    let candle_date = data.0;
                                    let mut higher_candle: Candle = new_candle.clone();

                                    let current_session =
                                        &self.market_hours.current_session(candle_date).unwrap();

                                    if is_mtf_strategy(&self.strategy_type) {
                                        if let HTFInstrument::HTFInstrument(
                                            ref mut htf_instrument,
                                        ) = self.htf_instrument
                                        {
                                            higher_candle = htf_instrument.next(data).unwrap();
                                        }
                                    }

                                    let close_date = format!(
                                        "{}:{} {}-{}",
                                        candle_date.hour(),
                                        candle_date.minute(),
                                        candle_date.day(),
                                        candle_date.month()
                                    );

                                    let (new_position_result, activated_orders_result) = self
                                        .strategy
                                        .next(
                                            &self.instrument,
                                            &self.htf_instrument,
                                            &self.trades_in,
                                            &self.trades_out,
                                            &self.orders,
                                            &self.tick,
                                            &self.market_hours,
                                        )
                                        .await;

                                    if new_candle.is_closed() {
                                        log::info!(
                                            "{:?} Session. Candle {:?} closed. Open pos: {} ",
                                            &current_session,
                                            close_date,
                                            open_positions
                                        );
                                        self.instrument
                                            .init_candle(data, &Some(self.time_frame.clone()));
                                    }

                                    if higher_candle.is_closed()
                                        && is_mtf_strategy(&self.strategy_type)
                                    {
                                        log::info!("HTF Candle closed {:?}", higher_candle.date());
                                        if let HTFInstrument::HTFInstrument(
                                            ref mut htf_instrument,
                                        ) = self.htf_instrument
                                        {
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
                                    }

                                    match new_position_result {
                                        PositionResult::None => (),
                                        _ => {
                                            //self.send_bot_status(&bot_str).await;
                                            self.process_activated_positions(
                                                &new_position_result,
                                                &mut open_positions,
                                            )
                                            .await
                                        }
                                    };

                                    match activated_orders_result {
                                        PositionResult::None => (),
                                        _ => {
                                            //self.send_bot_status(&bot_str).await;
                                            self.process_activated_orders(
                                                &activated_orders_result,
                                                &mut open_positions,
                                            )
                                            .await
                                        }
                                    };

                                    if !open_positions {
                                        self.orders = order::cancel_pending_expired_orders(
                                            index,
                                            &self.instrument,
                                            &mut self.orders,
                                        );
                                    }

                                    let update_bot_on_stream = env::var("SEND_UPDATE_ON_STREAM")
                                        .unwrap()
                                        .parse::<bool>()
                                        .unwrap();

                                    if update_bot_on_stream {
                                        self.send_bot_status(&bot_str).await;
                                    }
                                }
                                MessageType::TradeInFulfilled(res) => {
                                    let payload = res.payload.unwrap();
                                    let accepted = &payload.accepted;

                                    match accepted {
                                        true => {
                                            log::info!(
                                                "{:?} {} fullfilled ask: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                            );

                                            let trade_in = payload.data;

                                            order::update_trade_pending_orders(
                                                &mut self.orders,
                                                &trade_in,
                                            );

                                            trade::update_last(&mut self.trades_in, trade_in);

                                            self.strategy_stats = self.strategy.update_stats(
                                                &self.instrument,
                                                &self.trades_in,
                                                &self.trades_out,
                                            );

                                            open_positions = true;

                                            self.send_bot_status(&bot_str).await;
                                        }
                                        false => {
                                            log::error!(
                                                "{:?} {} not fullfilled ask: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                            );

                                            trade::delete_last(&mut self.trades_in);
                                            order::update_state_pending_orders(
                                                &payload.data,
                                                &mut self.orders,
                                            );
                                            open_positions = false;
                                        }
                                    }
                                }
                                MessageType::TradeOutFulfilled(res) => {
                                    let payload = res.payload.unwrap();
                                    let accepted = &payload.accepted;

                                    match accepted {
                                        true => {
                                            log::info!(
                                                "{:?} {} fullfilled ask: {} bid: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                                &payload.data.bid,
                                            );

                                            let trade_out = payload.data;
                                            let updated_trade_out =
                                                self.strategy.update_trade_stats(
                                                    self.trades_in.last().unwrap(),
                                                    &trade_out,
                                                    &self.instrument.data,
                                                );

                                            log::info!(
                                                "TradeOut stats profit {} profit_per {} ",
                                                &updated_trade_out.profit,
                                                &updated_trade_out.profit_per,
                                            );

                                            order::update_state_pending_orders(
                                                &updated_trade_out,
                                                &mut self.orders,
                                            );

                                            trade::update_last(
                                                &mut self.trades_out,
                                                updated_trade_out.clone(),
                                            );

                                            self.strategy_stats = self.strategy.update_stats(
                                                &self.instrument,
                                                &self.trades_in,
                                                &self.trades_out,
                                            );

                                            open_positions = false;
                                            self.send_bot_status(&bot_str).await;
                                        }
                                        false => {
                                            log::error!(
                                                "{:?} {} not fullfilled ask: {} bid: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                                &payload.data.bid,
                                            );

                                            trade::delete_last(&mut self.trades_out);

                                            open_positions = true;
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
    env: Option<Environment>,
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
            env: None,
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

    pub fn env(mut self, val: Environment) -> Self {
        if val.is_prod() {
            log::info!("Launching bot in {} env", val.value());
        } else {
            log::warn!("Launching bot in {} env", val.value());
        }
        self.env = Some(val);
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

    pub fn server_url(mut self, val: String) -> Self {
        let ws_client = WebSocket::connect(&val);
        self.websocket = Some(ws_client);
        self
    }

    pub fn build(self) -> Result<Bot> {
        if let (
            Some(env),
            Some(symbol),
            Some(market),
            Some(time_frame),
            Some(higher_time_frame),
            Some(strategy_name),
            Some(strategy_type),
            Some(websocket),
        ) = (
            self.env,
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

            let strategy = set_strategy(
                &strategy_name,
                &time_frame.to_string(),
                Some(&self.higher_time_frame.clone().unwrap().to_string()),
                strategy_type.clone(),
            );

            Ok(Bot {
                uuid: uuid::Uuid::new(),
                env,
                symbol,
                market,
                tick: InstrumentTick::default(),
                market_hours: MarketHours::default(),
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
