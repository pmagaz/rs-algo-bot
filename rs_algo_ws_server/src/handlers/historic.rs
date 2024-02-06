use crate::error::RsAlgoErrorKind;

use csv::{ReaderBuilder, StringRecord};
use rs_algo_shared::broker::{DOHLC, VEC_DOHLC};
use rs_algo_shared::helpers::date::*;
use rs_algo_shared::models::time_frame::TimeFrameType;

use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::path::Path;

pub async fn get_historic_data(
    symbol: &str,
    time_frame: &TimeFrameType,
    limit: i64,
) -> Result<VEC_DOHLC, RsAlgoErrorKind> {
    let instrument = read_csv(&symbol, &time_frame, limit).await?;

    Ok(instrument)
}

fn round_down_to_interval(time: DateTime<Local>, time_frame: &TimeFrameType) -> DateTime<Local> {
    match time_frame {
        TimeFrameType::M5 => {
            time - Duration::minutes(time.minute() as i64 % 5)
                - Duration::seconds(time.second() as i64)
                - Duration::nanoseconds(time.nanosecond() as i64)
        }
        TimeFrameType::M15 => {
            time - Duration::minutes(time.minute() as i64 % 15)
                - Duration::seconds(time.second() as i64)
                - Duration::nanoseconds(time.nanosecond() as i64)
        }
        TimeFrameType::M30 => {
            time - Duration::minutes(time.minute() as i64 % 30)
                - Duration::seconds(time.second() as i64)
                - Duration::nanoseconds(time.nanosecond() as i64)
        }
        TimeFrameType::H1 => time
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
        TimeFrameType::H4 => {
            let hour_rounded = time.hour() - (time.hour() % 4);
            time.with_hour(hour_rounded)
                .unwrap()
                .with_minute(0)
                .unwrap()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap()
        }
        _ => time,
    }
}

async fn read_csv(
    symbol: &str,
    time_frame: &TimeFrameType,
    records_limit: i64,
) -> Result<Vec<DOHLC>, RsAlgoErrorKind> {
    let file_path = &format!(
        "{}{}.csv",
        env::var("BACKEND_HISTORIC_DATA_FOLDER").unwrap(),
        symbol
    );

    let file = File::open(Path::new(&file_path)).map_err(|_| RsAlgoErrorKind::File)?;

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b';')
        .from_reader(file);

    let mut count = 0;
    let mut data: BTreeMap<DateTime<Local>, (f64, f64, f64, f64, f64)> = BTreeMap::new();
    for result in rdr.records() {
        if records_limit != 0 && count >= records_limit {
            break;
        }
        let record: StringRecord = result.map_err(|_| RsAlgoErrorKind::File)?;

        let date_str = format!("{}", &record[0]);
        let date_time = Local
            .datetime_from_str(&date_str, "%Y%m%d %H%M%S")
            .map_err(|_| RsAlgoErrorKind::File)?;

        let open = record[1]
            .parse::<f64>()
            .map_err(|_| RsAlgoErrorKind::File)?;
        let high = record[2]
            .parse::<f64>()
            .map_err(|_| RsAlgoErrorKind::File)?;
        let low = record[3]
            .parse::<f64>()
            .map_err(|_| RsAlgoErrorKind::File)?;
        let close = record[4]
            .parse::<f64>()
            .map_err(|_| RsAlgoErrorKind::File)?;
        let volume = record[5]
            .parse::<f64>()
            .map_err(|_| RsAlgoErrorKind::File)?;

        let rounded_datetime = round_down_to_interval(date_time, time_frame);
        data.entry(rounded_datetime)
            .and_modify(|e| {
                e.0 = if e.0 == 0.0 { open } else { e.0 };
                e.1 = e.1.max(high);
                e.2 = e.2.min(low);
                e.3 = close;
                e.4 += volume;
            })
            .or_insert((open, high, low, close, volume));

        count += 1;
    }

    let final_data = data
        .into_iter()
        .map(|(dt, ohlc)| (dt, ohlc.0, ohlc.1, ohlc.2, ohlc.3, ohlc.4))
        .collect();

    Ok(final_data)
}
