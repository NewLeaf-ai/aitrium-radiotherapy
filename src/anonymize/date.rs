use crate::anonymize::policy::DateStrategy;
use chrono::{Datelike, Duration, NaiveDate};

pub fn transform_date_like(
    input: &str,
    vr: &str,
    strategy: DateStrategy,
    days_shift: i32,
    fixed_value: Option<&str>,
) -> Option<String> {
    match strategy {
        DateStrategy::KeepYearOnly => keep_year_only(input, vr),
        DateStrategy::FixedShiftDataset => shift_dataset_date(input, vr, days_shift),
        DateStrategy::FixedValue => fixed_value
            .map(|value| value.to_string())
            .or_else(|| default_fixed(vr).map(ToOwned::to_owned)),
    }
}

fn keep_year_only(input: &str, vr: &str) -> Option<String> {
    let raw = input.trim();
    match vr {
        "DA" => {
            if raw.is_empty() {
                return Some(String::new());
            }
            if raw.len() < 4 {
                return None;
            }
            Some(format!("{}0101", &raw[..4]))
        }
        "DT" => {
            if raw.is_empty() {
                return Some(String::new());
            }
            if raw.len() < 4 {
                return None;
            }
            Some(format!("{}0101000000", &raw[..4]))
        }
        "TM" => {
            if raw.is_empty() {
                Some(String::new())
            } else {
                Some("000000".to_string())
            }
        }
        _ => None,
    }
}

fn shift_dataset_date(input: &str, vr: &str, days_shift: i32) -> Option<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Some(String::new());
    }

    match vr {
        "DA" => {
            let date = parse_da(raw)?;
            Some(
                (date + Duration::days(i64::from(days_shift)))
                    .format("%Y%m%d")
                    .to_string(),
            )
        }
        "DT" => {
            if raw.len() < 8 {
                return None;
            }
            let date = parse_da(&raw[..8])? + Duration::days(i64::from(days_shift));
            let suffix = if raw.len() > 8 { &raw[8..] } else { "" };
            Some(format!("{}{}", date.format("%Y%m%d"), suffix))
        }
        "TM" => Some(raw.to_string()),
        _ => None,
    }
}

fn parse_da(value: &str) -> Option<NaiveDate> {
    if value.len() < 8 {
        return None;
    }
    let year = value[..4].parse::<i32>().ok()?;
    let month = value[4..6].parse::<u32>().ok()?;
    let day = value[6..8].parse::<u32>().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn default_fixed(vr: &str) -> Option<&'static str> {
    match vr {
        "DA" => Some("19000101"),
        "DT" => Some("19000101000000"),
        "TM" => Some("000000"),
        _ => None,
    }
}

pub fn derive_shift(seed: &[u8]) -> i32 {
    if seed.is_empty() {
        return 0;
    }
    let mut acc: i64 = 0;
    for byte in seed {
        acc = (acc * 131 + i64::from(*byte)) % 7301;
    }
    let shifted = acc - 3650;
    if shifted == 0 {
        31
    } else {
        shifted as i32
    }
}

pub fn year_only_safe_string(date: NaiveDate) -> String {
    format!("{}0101", date.year())
}
