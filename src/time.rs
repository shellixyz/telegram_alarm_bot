use chrono::Duration;
use compound_duration::format_dhms;
use serde::{Serialize,Deserialize};
use std::ops::Deref;

type TimestampInner = chrono::DateTime<chrono::Local>;

#[derive(Serialize,Deserialize)]
pub struct Timestamp(TimestampInner);

impl Deref for Timestamp {
    type Target = TimestampInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Local::now())
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct LastSeenDuration(Duration);

impl LastSeenDuration {
    pub fn new(seen_timestamp: &Timestamp) -> Self {
        Self(chrono::Local::now().signed_duration_since(seen_timestamp.0).max(Duration::seconds(0)))
    }
}

impl std::fmt::Display for LastSeenDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(format_dhms(self.0.num_seconds()).as_str())
    }
}

impl Deref for LastSeenDuration {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
