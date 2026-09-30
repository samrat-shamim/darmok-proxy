use crate::value::Value;

pub const MYSQL_ZERO_DATE: &str = "0000-00-00";
pub const MYSQL_ZERO_DATETIME: &str = "0000-00-00 00:00:00";
pub const PG_ZERO_DATE_SENTINEL: &str = "0001-01-01";
pub const PG_ZERO_DATETIME_SENTINEL: &str = "0001-01-01 00:00:00";

pub fn mysql_zero_temporal_to_pg_sentinel(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed == MYSQL_ZERO_DATE {
        return Some(PG_ZERO_DATE_SENTINEL.to_owned());
    }
    if trimmed == MYSQL_ZERO_DATETIME {
        return Some(PG_ZERO_DATETIME_SENTINEL.to_owned());
    }

    let time = trimmed.strip_prefix("0000-00-00 ")?;
    zero_datetime_time_component(time).map(|time| format!("{PG_ZERO_DATE_SENTINEL} {time}"))
}

pub fn pg_date_sentinel_to_mysql_zero(text: &str) -> Option<Value> {
    (text.trim() == PG_ZERO_DATE_SENTINEL).then(|| Value::Date {
        year: 0,
        month: 0,
        day: 0,
    })
}

pub fn pg_datetime_sentinel_to_mysql_zero(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let time = trimmed.strip_prefix("0001-01-01 ")?;
    zero_datetime_time_component(time).map(|time| {
        let micros = time
            .split_once('.')
            .and_then(|(_, fraction)| fraction_to_micros(fraction))
            .unwrap_or(0);
        Value::DateTime {
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            micros,
        }
    })
}

pub fn mysql_zero_date_value_to_pg_sentinel(value: &Value) -> Option<String> {
    match value {
        Value::Date {
            year: 0,
            month: 0,
            day: 0,
        } => Some(PG_ZERO_DATE_SENTINEL.to_owned()),
        Value::DateTime {
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            micros,
        } => {
            if *micros == 0 {
                Some(PG_ZERO_DATETIME_SENTINEL.to_owned())
            } else {
                Some(format!("{PG_ZERO_DATETIME_SENTINEL}.{micros:06}"))
            }
        }
        _ => None,
    }
}

pub fn mysql_type_accepts_zero_date(mysql_type: &str) -> bool {
    let head = mysql_type.split('(').next().unwrap_or(mysql_type);
    let normalized = head.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "date" | "datetime" | "timestamp")
}

fn zero_datetime_time_component(time: &str) -> Option<&str> {
    if time == "00:00:00" {
        return Some(time);
    }
    let fraction = time.strip_prefix("00:00:00.")?;
    (!fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())).then_some(time)
}

fn fraction_to_micros(fraction: &str) -> Option<u32> {
    if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut micros = fraction.chars().take(6).collect::<String>();
    while micros.len() < 6 {
        micros.push('0');
    }
    micros.parse().ok()
}
