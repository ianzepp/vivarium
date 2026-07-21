//! Shared human interval parsing (`30s`, `5m`, `1h`, bare seconds).

use std::time::Duration;

use crate::error::VivariumError;

/// Parse a positive duration from a human interval string.
///
/// Accepted forms: bare seconds (`30`), or a number with unit `s` / `m` / `h`
/// (`30s`, `5m`, `1h`). Zero and unknown units are rejected.
///
/// # Errors
/// Returns a config error when the value cannot be parsed as a positive interval.
pub fn parse_duration(value: &str) -> Result<Duration, VivariumError> {
    let value = value.trim();
    let Some((number, unit)) = split_duration(value) else {
        return Err(invalid_duration(value));
    };
    let amount = number.parse::<u64>().map_err(|_| invalid_duration(value))?;
    if amount == 0 {
        return Err(VivariumError::Config(
            "duration must be greater than zero".into(),
        ));
    }
    let seconds = match unit {
        "" | "s" => amount,
        "m" => amount.saturating_mul(60),
        "h" => amount.saturating_mul(60 * 60),
        _ => {
            return Err(VivariumError::Config(format!(
                "invalid duration unit '{unit}'; use s, m, or h"
            )));
        }
    };
    Ok(Duration::from_secs(seconds))
}

/// Parse a duration and return whole seconds.
///
/// # Errors
/// Same as [`parse_duration`].
pub fn parse_duration_secs(value: &str) -> Result<u64, VivariumError> {
    Ok(parse_duration(value)?.as_secs())
}

fn split_duration(value: &str) -> Option<(&str, &str)> {
    let first_unit = value
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    if first_unit == 0 {
        return None;
    }
    Some((&value[..first_unit], &value[first_unit..]))
}

fn invalid_duration(value: &str) -> VivariumError {
    VivariumError::Config(format!(
        "invalid duration '{value}'; use values like 30s, 5m, or 1h"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_units_and_bare_seconds() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_mins(5));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_hours(1));
        assert_eq!(parse_duration(" 2h ").unwrap(), Duration::from_hours(2));
    }

    #[test]
    fn rejects_zero_and_unknown() {
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("5ms").is_err());
        assert!(parse_duration("1d").is_err());
        assert!(parse_duration("2hr").is_err());
    }
}
