//! UTC calendar dates without depending on host shell utilities.

use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

const SECONDS_PER_DAY: u64 = 86_400;

/// Current UTC date as ISO 8601 (`YYYY-MM-DD`).
pub fn today_utc() -> Result<String, SystemTimeError> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(date_utc_from_unix_secs(seconds))
}

/// Convert non-negative Unix seconds to an ISO 8601 UTC calendar date.
#[must_use]
pub fn date_utc_from_unix_secs(seconds: u64) -> String {
    let days = i64::try_from(seconds / SECONDS_PER_DAY).unwrap_or(i64::MAX - 719_468);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

// Howard Hinnant's civil-from-days algorithm. The 719468 offset maps the
// Unix epoch to the proleptic Gregorian calendar without lookup tables.
fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}
