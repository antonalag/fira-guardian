//! UTC timestamp formatter for the S11 `created_at` field
//! (report-provenance-slice, design §5).
//!
//! Pure integer arithmetic on a caller-supplied count of seconds since the Unix
//! epoch. CORE never reads a clock; the adapter supplies the value.

/// Last accepted value: `9999-12-31T23:59:59Z`.
const MAX_YEAR: i64 = 9999;

/// `YYYY-MM-DDTHH:MM:SSZ` (UTC, second precision) for `unix_seconds`, or `None`
/// if the year would exceed 9999.
pub(super) fn format_utc_timestamp(unix_seconds: u64) -> Option<String> {
    // u64::MAX / 86_400 fits comfortably in i64.
    let days = i64::try_from(unix_seconds / 86_400).ok()?;
    let sod = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    if year > MAX_YEAR {
        return None;
    }
    let (hh, mm, ss) = (sod / 3600, sod % 3600 / 60, sod % 60);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}Z"
    ))
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → proleptic
/// Gregorian `(year, month, day)`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
