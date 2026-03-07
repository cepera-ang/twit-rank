use time::format_description::well_known::{Rfc2822, Rfc3339};
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

const RFC3339_SECONDS: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
const X_CREATED_AT: &[BorrowedFormatItem<'static>] =
    format_description!("[weekday repr:short] [month repr:short] [day padding:space] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute] [year]");

pub fn now_unix_timestamp() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

pub fn now_rfc3339_seconds() -> String {
    format_utc(OffsetDateTime::now_utc())
}

pub fn parse_rfc2822_or_rfc3339(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc2822)
        .or_else(|_| OffsetDateTime::parse(s, &Rfc3339))
        .ok()
}

pub fn parse_x_created_at_to_rfc3339(s: &str) -> Option<String> {
    OffsetDateTime::parse(s, &X_CREATED_AT).ok().map(format_utc)
}

pub fn unix_millis_to_rfc3339_seconds(ms: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(ms / 1000)
        .ok()
        .map(format_utc)
}

fn format_utc(dt: OffsetDateTime) -> String {
    dt.to_offset(UtcOffset::UTC)
        .format(RFC3339_SECONDS)
        .expect("RFC3339 seconds format should always be valid")
}
