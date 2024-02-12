use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc, Duration};
use ical::property::Property;

use crate::Error;

peg::parser! {
    pub grammar date_time_parser() for str {
        pub rule date() -> NaiveDate
            = year:$(['0'..='9']{4}) month:$(['0'..='9']{2}) day:$(['0'..='9']{2}) {
                ?NaiveDate::from_ymd_opt(year.parse().unwrap(), month.parse().unwrap(), day.parse().unwrap()).ok_or("Invalid date")
            }

        rule raw_time() -> NaiveTime
            = hour:$(['0'..='9']{2}) minute:$(['0'..='9']{2}) second:$(['0'..='9']{2}) {
               ?NaiveTime::from_hms_opt(hour.parse().unwrap(), minute.parse().unwrap(), second.parse().unwrap()).ok_or("Invalid time")
            }

        pub rule time() -> ICalTime
            = time:raw_time() utc:"Z"? {
                if utc.is_some() {
                    ICalTime::Utc {
                        time
                    }
                } else {
                    ICalTime::Floating {
                        time
                    }
                }
            }

        pub rule date_time() -> IcalDateTime
            = date:date() "T" time:time() {
                ?match time {
                    ICalTime::Utc { time } => Ok(IcalDateTime::Utc {
                        date_time: DateTime::from_naive_utc_and_offset(NaiveDateTime::new(date, time), Utc)
                    }),
                    ICalTime::Floating { time } => Ok(IcalDateTime::Floating {
                        date_time: NaiveDateTime::new(date, time)
                    }),
                    _ => Err("Invalid date time")
                }
            }

            rule duration_negative() -> bool
                = "-" { true }
                / "+" { false }


            rule duration_seconds() -> Duration
                = seconds:$(['0'..='9']+) "S" {
                    Duration::seconds(seconds.parse().unwrap())
                }

            rule duration_minutes() -> Duration
                = minutes:$(['0'..='9']+) "M" seconds:duration_seconds()? {
                    Duration::minutes(minutes.parse().unwrap()) + seconds.unwrap_or(Duration::zero())
                }

            rule duration_hours() -> Duration
                = hours:$(['0'..='9']+) "H" minutes:duration_minutes()? {
                    Duration::hours(hours.parse().unwrap()) + minutes.unwrap_or(Duration::zero())
                }

            rule duration_time() -> Duration
                = "T" time:(hours:duration_hours() / minutes:duration_minutes() / seconds:duration_seconds()) {
                    time
                }

            rule duration_days() -> Duration
                = days:$(['0'..='9']+) "D" time:duration_time()? {
                    Duration::days(days.parse().unwrap()) + time.unwrap_or(Duration::zero())
                }

            rule duration_weeks() -> Duration
                = weeks:$(['0'..='9']+) "W" {
                    Duration::weeks(weeks.parse().unwrap())
                }

            pub rule duration() -> Duration
            = negative:duration_negative()? "P" duration:(days:duration_days() / time:duration_time() / weeks:duration_weeks()) {
                if negative.unwrap_or(false) {
                    -duration
                } else {
                    duration
                }
            }
    }
}

pub struct IcalDate {
    pub date: NaiveDate,
}

impl TryFrom<Property> for IcalDate {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match date_time_parser::date(&value) {
                    Ok(date) => Ok(IcalDate { date }),
                    Err(_) => Err(Error::InvalidDate)
                }
            },
            None => Err(Error::InvalidDate)
        }
    }
}

fn get_tzid(property: &Property) -> Option<String> {
    property.params.as_ref()?.iter().find(|param| param.0 == "TZID").map(|param| param.1.first()).unwrap().cloned()
}

pub enum ICalTime {
    Utc {
        time: NaiveTime,
    },
    Floating {
        time: NaiveTime,
    },
    TimeZone {
        time: NaiveTime,
        tzid: String,
    },
}

impl TryFrom<Property> for ICalTime {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match &property.value {
            Some(value) => {
                let tzid = get_tzid(&property);
                match date_time_parser::time(&value) {
                    Ok(time) => {
                        match (time, tzid) {
                            (ICalTime::Utc { time }, None) => Ok(ICalTime::Utc { time }),
                            (ICalTime::Floating { time }, None) => Ok(ICalTime::Floating { time }),
                            (ICalTime::Floating { time }, Some(tzid)) => Ok(ICalTime::TimeZone { time, tzid }),
                            _ => Err(Error::InvalidTime)
                        }
                    },
                    Err(_) => Err(Error::InvalidTime)
                }
            },
            None => Err(Error::InvalidTime)
        }
    }
}

pub enum IcalDateTime {
    Utc {
        date_time: DateTime<Utc>,
    },
    Floating {
        date_time: NaiveDateTime,
    },
    TimeZone {
        date_time: NaiveDateTime,
        tzid: String,
    },
}

impl TryFrom<Property> for IcalDateTime {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match &property.value {
            Some(value) => {
                let tzid = get_tzid(&property);
                match date_time_parser::date_time(&value) {
                    Ok(date_time) => {
                        match (date_time, tzid) {
                            (IcalDateTime::Utc { date_time }, None) => Ok(IcalDateTime::Utc { date_time }),
                            (IcalDateTime::Floating { date_time }, None) => Ok(IcalDateTime::Floating { date_time }),
                            (IcalDateTime::Floating { date_time }, Some(tzid)) => Ok(IcalDateTime::TimeZone { date_time, tzid }),
                            _ => Err(Error::InvalidDateTime)
                        }
                    },
                    Err(_) => Err(Error::InvalidDateTime)
                }
            },
            None => Err(Error::InvalidDateTime)
        }
    }
}

pub struct ICalDuration {
    pub duration: Duration,
}

impl TryFrom<Property> for ICalDuration {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match date_time_parser::duration(&value) {
                    Ok(duration) => Ok(ICalDuration { duration }),
                    Err(_) => Err(Error::InvalidDateTime)
                }
            },
            None => Err(Error::InvalidDateTime)
        }
    }
}

peg::parser! {
    pub grammar timzeone_parser() for str {
        rule utc_negative() -> bool
            = "-" { true }
            / "+" { false }

        pub rule utc_offset() -> Duration
            = negative:utc_negative() hours:$(['0'..='9']{2}) minutes:$(['0'..='9']{2}) seconds:$(['0'..='9']{2})? {
                let offset = Duration::hours(hours.parse().unwrap()) + Duration::minutes(minutes.parse().unwrap()) + Duration::seconds(seconds.unwrap_or("0").parse().unwrap());
                if negative {
                    -offset
                } else {
                    offset
                }
            }
    }
}

pub struct IcalUTCOffset {
    pub offset: Duration,
}

impl TryFrom<Property> for IcalUTCOffset {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match timzeone_parser::utc_offset(&value) {
                    Ok(offset) => Ok(IcalUTCOffset { offset }),
                    Err(_) => Err(Error::InvalidTimezone)
                }
            },
            None => Err(Error::InvalidTimezone)
        }
    }
}