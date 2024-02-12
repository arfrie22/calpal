use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use ical::property::Property;

use crate::{timezone::TimezoneMap, types::{ICalDuration, IcalDate, IcalDateTime}, Error};


pub enum EventTimeRange {
    Date {
        start: NaiveDate,
        end: NaiveDate
    },
    DateTime {
        start: DateTime<Utc>,
        end: DateTime<Utc>
    },
    FloatingDateTime {
        start: NaiveDateTime,
        end: NaiveDateTime
    }
}  

enum TimeValue {
    Date(IcalDate),
    DateTime(IcalDateTime),
}

impl TryFrom<Property> for TimeValue {
    type Error = Error;

    fn try_from(value: Property) -> Result<Self, Self::Error> {
        IcalDate::try_from(value.clone()).map(TimeValue::Date).or_else(|_| IcalDateTime::try_from(value).map(TimeValue::DateTime))
    }
}

struct RawTiming {
    start: Property,
    end: Option<Property>,
    duration: Option<Property>,
}

impl RawTiming {
    pub fn get_time_range(&self, timezone_map: &TimezoneMap) -> Result<EventTimeRange, Error> {
        let start = TimeValue::try_from(self.start.clone())?;
        let end = self.end.clone().map(|end| TimeValue::try_from(end)).transpose()?;
        let duration = self.duration.clone().map(|duration| ICalDuration::try_from(duration)).transpose()?;

        match (start, end, duration) {
            ( TimeValue::Date(start), None, None ) => {
                let start = start.date;
                Ok(EventTimeRange::Date { start, end: start + chrono::Duration::days(1) })
            },
            ( TimeValue::Date(start), Some(TimeValue::Date(end)), None ) => {
                Ok(EventTimeRange::Date { start: start.date, end: end.date })
            },
            ( TimeValue::DateTime(start), None, None ) => {
                match start {
                    IcalDateTime::Utc { date_time } => Ok(EventTimeRange::DateTime { start: date_time, end: date_time + chrono::Duration::days(1) }),
                    IcalDateTime::Floating { date_time } => Ok(EventTimeRange::FloatingDateTime { start: date_time, end: date_time + chrono::Duration::days(1) }),
                    IcalDateTime::TimeZone { date_time, tzid } => {
                        let timezone = timezone_map.get(&tzid).ok_or(Error::InvalidTimezone)?;
                        let next_day = date_time.date().succ_opt().ok_or(Error::InvalidDate)?;
                        let end_date_time = NaiveDateTime::new(next_day, NaiveTime::MIN) - chrono::Duration::seconds(1);
                        Ok(EventTimeRange::DateTime { start: timezone.to_utc(date_time)?, end: timezone.to_utc(end_date_time)? })
                    }
                }
            },
            ( TimeValue::DateTime(start), Some(TimeValue::DateTime(end)), None ) => {
                match (start, end) {
                    (IcalDateTime::Utc { date_time: start }, IcalDateTime::Utc { date_time: end }) => Ok(EventTimeRange::DateTime { start, end }),
                    (IcalDateTime::Floating { date_time: start }, IcalDateTime::Floating { date_time: end }) => Ok(EventTimeRange::FloatingDateTime { start, end }),
                    (IcalDateTime::TimeZone { date_time: start, tzid: start_tzid }, IcalDateTime::TimeZone { date_time: end, tzid: end_tzid }) => {
                        let start_timezone = timezone_map.get(&start_tzid).ok_or(Error::InvalidTimezone)?;
                        let end_timezone = timezone_map.get(&end_tzid).ok_or(Error::InvalidTimezone)?;
                        Ok(EventTimeRange::DateTime { start: start_timezone.to_utc(start)?, end: end_timezone.to_utc(end)? })
                    },
                    _ => Err(Error::InvalidDateTime)
                }
            },
            ( TimeValue::DateTime(start), None, Some(duration) ) => {
                match start {
                    IcalDateTime::Utc { date_time } => Ok(EventTimeRange::DateTime { start: date_time, end: date_time + duration.duration }),
                    IcalDateTime::Floating { date_time } => Ok(EventTimeRange::FloatingDateTime { start: date_time, end: date_time + duration.duration }),
                    IcalDateTime::TimeZone { date_time, tzid } => {
                        let timezone = timezone_map.get(&tzid).ok_or(Error::InvalidTimezone)?;
                        Ok(EventTimeRange::DateTime { start: timezone.to_utc(date_time)?, end: timezone.to_utc(date_time + duration.duration)? })
                    }
                }
            },
            _ => Err(Error::InvalidTimeRange)
        }
    }
}

pub struct Event {
    time: EventTimeRange,
}