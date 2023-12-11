use crate::error::{Result, RsAlgoError, RsAlgoErrorKind};
use crate::helpers::vars::*;
use crate::message;
use crate::strategies::strategy::*;

use rs_algo_shared::helpers::date::{Local, Timelike};
use rs_algo_shared::helpers::uuid::*;
use rs_algo_shared::helpers::{date::*, uuid};
use rs_algo_shared::models::bot::BotData;
use rs_algo_shared::models::environment::{self, Environment};
use rs_algo_shared::models::mode::ExecutionMode;
use rs_algo_shared::models::order::{Order, OrderStatus};
use rs_algo_shared::models::strategy::StrategyStats;
use rs_algo_shared::models::tick::InstrumentTick;
use rs_algo_shared::models::time_frame::*;
use rs_algo_shared::models::trade::*;
use rs_algo_shared::models::{market::*, order};
use rs_algo_shared::models::{strategy::*, trade};
use rs_algo_shared::scanner::candle::Candle;
use rs_algo_shared::scanner::instrument::{HTFInstrument, Instrument};
use rs_algo_shared::ws::message::*;
use rs_algo_shared::ws::ws_client::WebSocket;

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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

    pub async fn restore_values(&mut self, data: BotData) {
        let max_historical_positions = env::var("MAX_HISTORICAL_POSITIONS")
            .unwrap()
            .parse::<usize>()
            .unwrap();

        self.strategy_stats = data.strategy_stats().clone();

        let trades_in = data.trades_in();
        let trades_out = data.trades_out();
        let orders = data.orders();

        self.trades_in = trades_in
            .iter()
            .skip(trades_in.len().saturating_sub(max_historical_positions))
            .take(max_historical_positions)
            .cloned()
            .collect();

        self.trades_out = trades_out
            .iter()
            .skip(trades_out.len().saturating_sub(max_historical_positions))
            .take(max_historical_positions)
            .cloned()
            .collect();

        self.orders = orders
            .iter()
            .skip(orders.len().saturating_sub(max_historical_positions))
            .take(max_historical_positions)
            .cloned()
            .collect();
    }

    pub async fn fullfill_activated_order<T: Trade>(
        &mut self,
        activated_orders_result: &PositionResult,
        trade_in: &T,
        order: &Order,
        open_positions: &mut bool,
    ) {
        let should_open = match activated_orders_result {
            PositionResult::MarketInOrder(_, _) => true,
            _ => false,
        };

        if *open_positions != should_open {
            log::info!("Sending {:?} ...", trade_in.get_type());
            self.send_position::<PositionResult>(
                activated_orders_result,
                self.symbol.clone(),
                self.time_frame.clone(),
            )
            .await;
            order::fulfill_bot_order::<T>(trade_in, order, &mut self.orders, &self.instrument);

            *open_positions = should_open;
        }
    }

    async fn process_trade(
        &mut self,
        new_position_result: &PositionResult,
        associated_orders: Option<&Vec<Order>>,
        open_positions: &mut bool,
    ) {
        match new_position_result {
            PositionResult::MarketIn(TradeResult::TradeIn(t), _) if !*open_positions => {
                log::info!("Sending {:?} ...", t.trade_type);

                self.send_position::<PositionResult>(
                    new_position_result,
                    self.symbol.clone(),
                    self.time_frame.clone(),
                )
                .await;
                if let Some(new_ords) = associated_orders {
                    self.orders = order::add_pending(self.orders.clone(), new_ords.clone());
                }
                *open_positions = true;
            }
            PositionResult::MarketOut(TradeResult::TradeOut(t)) if *open_positions => {
                log::info!("Sending {:?} ...", t.trade_type);
                self.send_position::<PositionResult>(
                    new_position_result,
                    self.symbol.clone(),
                    self.time_frame.clone(),
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
            }
            PositionResult::MarketOutOrder(TradeResult::TradeOut(trade_out), order) => {
                self.fullfill_activated_order::<TradeOut>(
                    activated_orders_result,
                    trade_out,
                    order,
                    open_positions,
                )
                .await;
            }
            _ => todo!(),
        };
    }

    async fn process_activated_positions(
        &mut self,
        new_position_result: &PositionResult,
        open_positions: &mut bool,
    ) {
        match new_position_result {
            PositionResult::MarketIn(TradeResult::TradeIn(_), associated_orders) => {
                self.process_trade(
                    new_position_result,
                    associated_orders.as_ref(),
                    open_positions,
                )
                .await;
            }
            PositionResult::MarketOut(TradeResult::TradeOut(_)) => {
                self.process_trade(new_position_result, None, open_positions)
                    .await;
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

    pub async fn send_position<T>(&mut self, trade: &T, symbol: String, _time_frame: TimeFrameType)
    where
        for<'de> T: Serialize + Deserialize<'de>,
    {
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

    pub async fn get_active_positions(&mut self) {
        log::info!("Getting {} active positons...", &self.symbol,);

        let active_positions_command = Command {
            command: CommandType::GetActivePositions,
            data: Some(&self),
        };

        self.websocket
            .send(&serde_json::to_string(&active_positions_command).unwrap())
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
        let mut secs = env::var("DISCONNECTED_RETRY")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        let ten_random_secs = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .subsec_nanos()
            % 11) as u64;

        // Add the random number to secs
        secs += ten_random_secs;

        log::info!("Reconnecting in {} secs...", secs);

        sleep(Duration::from_secs(secs)).await;
        self.websocket.re_connect().await;
        self.init_session().await;
    }

    pub async fn run(&mut self) {
        self.init_session().await;
        let mut open_positions = false;
        let bot_str = [&self.symbol, "_", &self.time_frame.to_string()].concat();

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
                                    let env = environment::from_str(&env::var("ENV").unwrap());

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
                                            log::info!("{} active trades found", num_active_trades);
                                            open_positions = true
                                        }
                                        _ => {
                                            log::info!("No active trades found");
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

                                    let num_active_stop_losses = active_stop_losses.len();

                                    match num_active_stop_losses.cmp(&0) {
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

                                    if num_active_trades != num_active_stop_losses {
                                        log::error!(
                                            "Active trades {} do not match active stop losses {} !",
                                            num_active_trades,
                                            num_active_stop_losses
                                        );
                                    } else {
                                        self.restore_values(bot_data).await;
                                        self.get_market_hours().await;
                                    }

                                    //TODO RECONCILIATION
                                    if env.is_prod() {
                                        self.get_active_positions().await;
                                    }
                                }
                                MessageType::ActivePositions(res) => {
                                    let position_result = res.payload.unwrap();
                                    match position_result {
                                        PositionResult::MarketIn(
                                            TradeResult::TradeIn(trade_in),
                                            orders,
                                        ) => {
                                            if trade::trade_exists(&self.trades_in, trade_in.id) {
                                                log::info!(
                                                    "Active position {:?} {} found. Updating position...",
                                                    trade_in.trade_type,
                                                    trade_in.id
                                                );
                                                trade::update_trades(&mut self.trades_in, trade_in);

                                                match orders {
                                                    Some(orders) => {
                                                        order::update_orders(
                                                            &mut self.orders,
                                                            &orders,
                                                        );
                                                    }
                                                    None => (),
                                                }
                                            } else {
                                                log::info!(
                                                    "Active position {:?} {} found. Adding position...",
                                                    trade_in.trade_type,
                                                    trade_in.id
                                                );
                                                match orders {
                                                    Some(orders) => {
                                                        self.trades_in.push(trade_in);

                                                        self.orders = order::add_pending(
                                                            self.orders.clone(),
                                                            orders,
                                                        )
                                                    }
                                                    None => (),
                                                }
                                            }

                                            open_positions = true;
                                        }
                                        _ => {
                                            log::info!("No active positions found");
                                            open_positions = false;
                                        }
                                    }
                                }
                                MessageType::MarketHours(res) => {
                                    let market_hours = res.payload.unwrap();
                                    let is_trading_hours = market_hours.is_trading_time();

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
                                MessageType::IsMarketOpen(_) => {
                                    let is_market_open = true;
                                    match is_market_open {
                                        true => {
                                            log::info!("{} Market open!", self.symbol);
                                            self.get_instrument_data().await;
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
                                MessageType::InstrumentTick(res) => {
                                    let tick = res.payload.unwrap();
                                    self.tick = tick;
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
                                        )
                                        .await;

                                    if new_candle.is_closed() {
                                        log::info!(
                                            "{} {:?} Session - Candle {:?} closed - Open pos: {} ",
                                            &self.env.value(),
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
                                        log::info!(
                                            "{:?} Session - HTF Candle {:?} closed - Open pos: {} ",
                                            &current_session,
                                            higher_candle.date(),
                                            open_positions
                                        );

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
                                    self.send_bot_status(&bot_str).await;
                                }
                                MessageType::StreamTickResponse(res) => {
                                    let tick = res.payload.unwrap();
                                    let tick = InstrumentTick::new()
                                        .symbol(self.symbol.clone())
                                        .ask(tick.ask())
                                        .bid(tick.bid())
                                        .high(tick.high())
                                        .low(tick.low())
                                        .spread(tick.spread())
                                        .pip_size(tick.pip_size())
                                        .time(Local::now().timestamp())
                                        .build()
                                        .unwrap();

                                    let index = self.instrument.data.len().checked_sub(1).unwrap();
                                    let pending_orders = order::get_pending(&self.orders);

                                    let activated_orders_result =
                                        self.strategy.pending_orders_activated(
                                            index,
                                            &self.instrument,
                                            &pending_orders,
                                            &self.trades_in,
                                            Some(&tick),
                                            true,
                                        );

                                    match activated_orders_result {
                                        PositionResult::None => (),
                                        _ => {
                                            self.process_activated_orders(
                                                &activated_orders_result,
                                                &mut open_positions,
                                            )
                                            .await
                                        }
                                    };

                                    self.tick = tick;
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

                                            let trade_response = payload;
                                            self.trades_in.push(trade_response.data);

                                            self.strategy_stats = self.strategy.update_stats(
                                                &self.instrument,
                                                &self.trades_in,
                                                &self.trades_out,
                                            );

                                            open_positions = true;
                                            order::extend_all_pending_orders(&mut self.orders);
                                            self.send_bot_status(&bot_str).await;
                                        }
                                        false => {
                                            log::error!(
                                                "{:?} {} not fullfilled ask: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                            );

                                            open_positions = false;

                                            order::cancel_trade_pending_orders(
                                                &payload.data,
                                                &mut self.orders,
                                            );
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

                                            let trade_response = payload;

                                            let updated_trade_out =
                                                self.strategy.update_trade_stats(
                                                    self.trades_in.last().unwrap(),
                                                    &trade_response.data,
                                                    &self.instrument.data,
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
                                            log::error!(
                                                "{:?} {} not fullfilled ask: {} bid: {}",
                                                &payload.data.trade_type,
                                                &payload.data.id,
                                                &payload.data.ask,
                                                &payload.data.bid,
                                            );

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
        log::warn!("Launching bot in {} env", &val.value());
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
