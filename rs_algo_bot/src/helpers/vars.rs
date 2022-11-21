use rs_algo_shared::models::market::*;
use rs_algo_shared::models::strategy::*;
use rs_algo_shared::models::time_frame::*;

pub fn get_market(market: String) -> Market {
    match market.as_ref() {
        "Forex" => Market::Forex,
        "Crypto" => Market::Crypto,
        _ => Market::Stock,
    }
}

// pub fn get_time_frame(time_frame: String) -> TimeFrameType {
//     match time_frame.as_ref() {
//         "M" => TimeFrameType::M,
//         "W" => TimeFrameType::W,
//         "D" => TimeFrameType::D,
//         "H4" => TimeFrameType::H4,
//         "H1" => TimeFrameType::H1,
//         "M30" => TimeFrameType::M30,
//         "M15" => TimeFrameType::M15,
//         "M5" => TimeFrameType::M5,
//         _ => TimeFrameType::M1,
//     }
// }

// pub fn get_strategy_type(strategy_type: String) -> StrategyType {
//     match strategy_type.as_ref() {
//         "OnlyLong" => StrategyType::OnlyLong,
//         "OnlyShort" => StrategyType::OnlyShort,
//         "LongShort" => StrategyType::LongShort,
//         "LongShortMultiTF" => StrategyType::LongShortMultiTF,
//         "OnlyLongMultiTF" => StrategyType::OnlyLongMultiTF,
//         _ => StrategyType::OnlyLongMultiTF,
//     }
// }

pub fn is_base_time_frame(a: &TimeFrameType, b: &TimeFrameType) -> bool {
    a == b
}
