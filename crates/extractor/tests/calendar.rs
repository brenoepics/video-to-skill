//! UTC calendar formatting is deterministic and independent of host programs.

use vts_extract::calendar::date_utc_from_unix_secs;

#[test]
fn unix_epoch_is_1970_01_01() {
    assert_eq!(date_utc_from_unix_secs(0), "1970-01-01");
}

#[test]
fn leap_days_and_year_boundaries_are_formatted_correctly() {
    assert_eq!(date_utc_from_unix_secs(951_782_400), "2000-02-29");
    assert_eq!(date_utc_from_unix_secs(1_704_067_199), "2023-12-31");
    assert_eq!(date_utc_from_unix_secs(1_704_067_200), "2024-01-01");
}
