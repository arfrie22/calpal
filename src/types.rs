use std::str::FromStr;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc, Duration};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ical::property::Property;
use http::Uri;

use crate::{Error, ICalTypes};

peg::parser! {
    pub grammar ical_type_parser() for str {
        pub rule boolean() -> bool
            = "TRUE" { true }
            / "FALSE" { false }

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

            rule pm_negative() -> bool
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

            pub rule duration() -> ICalDuration
            = negative:pm_negative()? "P" duration:(days:duration_days() / time:duration_time() / weeks:duration_weeks()) {
                ICalDuration{ duration: if negative.unwrap_or(false) {
                    -duration
                } else {
                    duration
                }}
            }

        pub rule period() -> IcalPeriod
            = start:date_time() "/" end:date_time() {
                IcalPeriod::StartEnd {
                    start,
                    end
                }
            }
            / start:date_time() "/" duration:duration() {
                IcalPeriod::StartDuration {
                    start,
                    duration: duration
                }
            }

            pub rule utc_offset() -> IcalUTCOffset
            = negative:pm_negative() hours:$(['0'..='9']{2}) minutes:$(['0'..='9']{2}) seconds:$(['0'..='9']{2})? {
                let offset = Duration::hours(hours.parse().unwrap()) + Duration::minutes(minutes.parse().unwrap()) + Duration::seconds(seconds.unwrap_or("0").parse().unwrap());
                IcalUTCOffset{ offset: if negative {
                    -offset
                } else {
                    offset
                }}
            }

            rule recur_frequency_t() -> ICalRecurFrequency
                = "SECONDLY" { ICalRecurFrequency::Secondly }
                / "MINUTELY" { ICalRecurFrequency::Minutely }
                / "HOURLY" { ICalRecurFrequency::Hourly }
                / "DAILY" { ICalRecurFrequency::Daily }
                / "WEEKLY" { ICalRecurFrequency::Weekly }
                / "MONTHLY" { ICalRecurFrequency::Monthly }
                / "YEARLY" { ICalRecurFrequency::Yearly }

            rule recur_frequency() -> ICalRecurFrequency
                = "FREQ=" freq:recur_frequency_t() {
                    freq
                }

            rule recur_until() -> IcalRecurBuilder
                = ";UNTIL=" date:date() {
                    IcalRecurBuilder {
                        limit: Some(IcalRecurLimit::Until(IcalRecurUntil::Date(date))),
                        ..Default::default()
                    }
                }
                / ";UNTIL=" date_time:date_time() {
                    ? match date_time {
                        IcalDateTime::Utc { date_time } => Ok(IcalRecurBuilder {
                            limit: Some(IcalRecurLimit::Until(IcalRecurUntil::DateTime(date_time))),
                            ..Default::default()
                        }),
                        _ => Err("Recur until must be in UTC")
                    }
                }

            rule recur_count() -> IcalRecurBuilder
                = ";COUNT=" count:$(['0'..='9']+) {
                    IcalRecurBuilder {
                        limit: Some(IcalRecurLimit::Count(count.parse().unwrap())),
                        ..Default::default()
                    }
                }

            rule recur_interval() -> IcalRecurBuilder
                = ";INTERVAL=" interval:$(['0'..='9']+) {
                    IcalRecurBuilder {
                        interval: Some(interval.parse().unwrap()),
                        ..Default::default()
                    }
                }

            rule recur_u8_list() -> Vec<u8>
                = data:(($(['0'..='9']*<1,2>) ++ ",")) {
                    data.iter().map(|s| s.parse().unwrap()).collect()
                }

            rule two_digit_i8() -> i8
                = negative:pm_negative()? input:$(['0'..='9']*<1,2>) { 
                    i8::from_str_radix(input, 10).unwrap() * if negative.unwrap_or(false) { -1 } else { 1 }
                }

            rule three_digit_i16() -> i16
                = negative:pm_negative()? input:$(['0'..='9']*<1,3>) { 
                    i16::from_str_radix(input, 10).unwrap() * if negative.unwrap_or(false) { -1 } else { 1 }
                }

            rule recur_i8_list() -> Vec<i8>
                = data:(two_digit_i8() ++ ",") {
                    data
                }

            rule recur_i16_list() -> Vec<i16>
                = data:(three_digit_i16() ++ ",") {
                    data
                }

            rule recur_day_of_week() -> ICalRecurDayOfWeek
                = "SU" { ICalRecurDayOfWeek::Sunday }
                / "MO" { ICalRecurDayOfWeek::Monday }
                / "TU" { ICalRecurDayOfWeek::Tuesday }
                / "WE" { ICalRecurDayOfWeek::Wednesday }
                / "TH" { ICalRecurDayOfWeek::Thursday }
                / "FR" { ICalRecurDayOfWeek::Friday }
                / "SA" { ICalRecurDayOfWeek::Saturday }
            
            
            rule recur_by_second() -> IcalRecurBuilder
                = ";BYSECOND=" seconds:recur_u8_list() {
                    IcalRecurBuilder {
                        by_second: Some(seconds),
                        ..Default::default()
                    }
                }
            
            rule recur_by_minute() -> IcalRecurBuilder
                = ";BYMINUTE=" minutes:recur_u8_list() {
                    IcalRecurBuilder {
                        by_minute: Some(minutes),
                        ..Default::default()
                    }
                }

            rule recur_by_hour() -> IcalRecurBuilder
                = ";BYHOUR=" hours:recur_u8_list() {
                    IcalRecurBuilder {
                        by_hour: Some(hours),
                        ..Default::default()
                    }
                }

            rule recur_weekday() -> IcalRecurWeekDay
                = ordwk:two_digit_i8()? weekday:recur_day_of_week() {
                    IcalRecurWeekDay {
                        day: weekday,
                        nth_of_month: ordwk
                    }
                }

            rule recur_by_day_list() -> Vec<IcalRecurWeekDay>
                = days:(recur_weekday() ++ ",") {
                    days
                }

            rule recur_by_day() -> IcalRecurBuilder
                = ";BYDAY=" days:recur_by_day_list() {
                    IcalRecurBuilder {
                        by_day: Some(days),
                        ..Default::default()
                    }
                }

            rule recur_by_month_day() -> IcalRecurBuilder
                = ";BYMONTHDAY=" days:recur_i8_list() {
                    IcalRecurBuilder {
                        by_month_day: Some(days),
                        ..Default::default()
                    }
                }

            rule recur_by_year_day() -> IcalRecurBuilder
                = ";BYYEARDAY=" days:recur_i16_list() {
                    IcalRecurBuilder {
                        by_year_day: Some(days),
                        ..Default::default()
                    }
                }

            rule recur_by_week_no() -> IcalRecurBuilder
                = ";BYWEEKNO=" weeks:recur_i8_list() {
                    IcalRecurBuilder {
                        by_week_no: Some(weeks),
                        ..Default::default()
                    }
                }

            rule recur_by_month() -> IcalRecurBuilder
                = ";BYMONTH=" months:recur_u8_list() {
                    IcalRecurBuilder {
                        by_month: Some(months),
                        ..Default::default()
                    }
                }

            rule recur_by_set_pos() -> IcalRecurBuilder
                = ";BYSETPOS=" pos:recur_i16_list() {
                    IcalRecurBuilder {
                        by_set_pos: Some(pos),
                        ..Default::default()
                    }
                }

            rule recur_wkst() -> IcalRecurBuilder
                = ";WKST=" weekday:recur_day_of_week() {
                    IcalRecurBuilder {
                        wkst: Some(weekday),
                        ..Default::default()
                    }
                }

            rule recur_keyword() -> IcalRecurBuilder
                = recur_until() / recur_count() / recur_interval() / recur_by_second() / recur_by_minute() / recur_by_hour() / recur_by_day() / recur_by_month_day() / recur_by_year_day() / recur_by_week_no() / recur_by_month() / recur_by_set_pos() / recur_wkst()

            rule recur_builder() -> IcalRecurBuilder
                = inital:recur_keyword() new:recur_builder()? {
                    ? match new {
                        Some(new) => inital.merge(new).map_err(|_| "Invalid recur"),
                        None => Ok(inital)
                    }
                }

            pub rule recur() -> IcalRecur
                = frequency:recur_frequency() builder:recur_builder()? {
                    builder.unwrap_or_default().build(frequency)
                }
                
    }
}

pub struct ICalBinary {
    pub data: Vec<u8>,
}

impl TryFrom<Property> for ICalBinary {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                Ok(ICalBinary { data: STANDARD.decode(value.as_bytes()).map_err(|_| Error::TypeDecode(ICalTypes::Binary))? })
            },
            None => Err(Error::TypeDecode(ICalTypes::Binary))
        }
    }
}

pub struct ICalBoolean {
    pub value: bool,
}

impl TryFrom<Property> for ICalBoolean {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match ical_type_parser::boolean(&value) {
                    Ok(value) => Ok(ICalBoolean { value }),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Boolean))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Boolean))
        }
    }
}

pub struct ICalCalAddress {
    pub address: Uri,
}

impl TryFrom<Property> for ICalCalAddress {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match Uri::from_str(&value) {
                    Ok(address) => Ok(ICalCalAddress { address }),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::CalAddress))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::CalAddress))
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
                match ical_type_parser::date(&value) {
                    Ok(date) => Ok(IcalDate { date }),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Date))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Date))
        }
    }
}

fn get_tzid(property: &Property) -> Option<String> {
    property.params.as_ref()?.iter().find(|param| param.0 == "TZID").map(|param| param.1.first()).unwrap().cloned()
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
                match ical_type_parser::date_time(&value) {
                    Ok(date_time) => {
                        match (date_time, tzid) {
                            (IcalDateTime::Utc { date_time }, None) => Ok(IcalDateTime::Utc { date_time }),
                            (IcalDateTime::Floating { date_time }, None) => Ok(IcalDateTime::Floating { date_time }),
                            (IcalDateTime::Floating { date_time }, Some(tzid)) => Ok(IcalDateTime::TimeZone { date_time, tzid }),
                            _ => Err(Error::TypeDecode(ICalTypes::DateTime))
                        }
                    },
                    Err(_) => Err(Error::TypeDecode(ICalTypes::DateTime))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::DateTime))
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
                match ical_type_parser::duration(&value) {
                    Ok(duration) => Ok(duration),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Duration))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Duration))
        }
    }
}

pub struct IcalFloat {
    pub value: f32,
}

impl TryFrom<Property> for IcalFloat {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match value.parse() {
                    Ok(value) => Ok(IcalFloat { value }),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Float))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Float))
        }
    }
}

pub struct IcalInteger {
    pub value: i32,
}

impl TryFrom<Property> for IcalInteger {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match value.parse() {
                    Ok(value) => Ok(IcalInteger { value }),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Integer))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Integer))
        }
    }
}

pub enum IcalPeriod {
    StartEnd {
        start: IcalDateTime,
        end: IcalDateTime,
    },
    StartDuration {
        start: IcalDateTime,
        duration: ICalDuration,
    },
}

impl TryFrom<Property> for IcalPeriod {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                let parts: Vec<&str> = value.split('/').collect();
                if parts.len() == 2 {
                    let start = IcalDateTime::try_from(Property { name: property.name.clone(), value: Some(parts[0].to_string()), params: property.params.clone() })?;
                    let end = IcalDateTime::try_from(Property { name: property.name.clone(), value: Some(parts[1].to_string()), params: property.params.clone() })?;
                    Ok(IcalPeriod::StartEnd { start, end })
                } else if parts.len() == 1 {
                    let start = IcalDateTime::try_from(Property { name: property.name.clone(), value: Some(parts[0].to_string()), params: property.params.clone() })?;
                    let duration = ICalDuration::try_from(Property { name: property.name.clone(), value: Some(parts[1].to_string()), params: property.params.clone() })?;
                    Ok(IcalPeriod::StartDuration { start, duration })
                } else {
                    Err(Error::TypeDecode(ICalTypes::Period))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Period))
        }
    }
}

pub enum IcalRecurUntil {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

pub enum IcalRecurLimit {
    Count(u64),
    Until(IcalRecurUntil),
}

pub enum ICalRecurFrequency {
    Secondly,
    Minutely,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

pub enum ICalRecurDayOfWeek {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

pub struct IcalRecurWeekDay {
    pub day: ICalRecurDayOfWeek,
    pub nth_of_month: Option<i8>,
}

#[derive(Default)]
struct IcalRecurBuilder {
    limit: Option<IcalRecurLimit>,
    interval: Option<u64>,
    by_second: Option<Vec<u8>>,
    by_minute: Option<Vec<u8>>,
    by_hour: Option<Vec<u8>>,
    by_day: Option<Vec<IcalRecurWeekDay>>,
    by_month_day: Option<Vec<i8>>,
    by_year_day: Option<Vec<i16>>,
    by_week_no: Option<Vec<i8>>,
    by_month: Option<Vec<u8>>,
    by_set_pos: Option<Vec<i16>>,
    wkst: Option<ICalRecurDayOfWeek>,
}

impl IcalRecurBuilder {
    fn build(self, frequency: ICalRecurFrequency) -> IcalRecur {
        IcalRecur {
            frequency,
            limit: self.limit,
            interval: self.interval,
            by_second: self.by_second,
            by_minute: self.by_minute,
            by_hour: self.by_hour,
            by_day: self.by_day,
            by_month_day: self.by_month_day,
            by_year_day: self.by_year_day,
            by_week_no: self.by_week_no,
            by_month: self.by_month,
            by_set_pos: self.by_set_pos,
            wkst: self.wkst,
        }
    }

    fn merge(self, new: IcalRecurBuilder) -> Result<IcalRecurBuilder, Error> {
        let mut out = IcalRecurBuilder::default();

        if self.limit.is_some() && new.limit.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }
        
        out.limit = self.limit.or(new.limit);

        if self.interval.is_some() && new.interval.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.interval = self.interval.or(new.interval);

        if self.by_second.is_some() && new.by_second.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_second = self.by_second.or(new.by_second);

        if self.by_minute.is_some() && new.by_minute.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_minute = self.by_minute.or(new.by_minute);

        if self.by_hour.is_some() && new.by_hour.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_hour = self.by_hour.or(new.by_hour);

        if self.by_day.is_some() && new.by_day.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_day = self.by_day.or(new.by_day);

        if self.by_month_day.is_some() && new.by_month_day.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_month_day = self.by_month_day.or(new.by_month_day);

        if self.by_year_day.is_some() && new.by_year_day.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_year_day = self.by_year_day.or(new.by_year_day);

        if self.by_week_no.is_some() && new.by_week_no.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_week_no = self.by_week_no.or(new.by_week_no);

        if self.by_month.is_some() && new.by_month.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_month = self.by_month.or(new.by_month);

        if self.by_set_pos.is_some() && new.by_set_pos.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.by_set_pos = self.by_set_pos.or(new.by_set_pos);

        if self.wkst.is_some() && new.wkst.is_some() {
            return Err(Error::TypeDecode(ICalTypes::Recur))
        }

        out.wkst = self.wkst.or(new.wkst);

        Ok(out)
    }
}

pub struct IcalRecur {
    frequency: ICalRecurFrequency,
    limit: Option<IcalRecurLimit>,
    interval: Option<u64>,
    by_second: Option<Vec<u8>>,
    by_minute: Option<Vec<u8>>,
    by_hour: Option<Vec<u8>>,
    by_day: Option<Vec<IcalRecurWeekDay>>,
    by_month_day: Option<Vec<i8>>,
    by_year_day: Option<Vec<i16>>,
    by_week_no: Option<Vec<i8>>,
    by_month: Option<Vec<u8>>,
    by_set_pos: Option<Vec<i16>>,
    wkst: Option<ICalRecurDayOfWeek>,
}

impl TryFrom<Property> for IcalRecur {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match ical_type_parser::recur(&value) {
                    Ok(recur) => Ok(recur),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Recur))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Recur))
        }
    }
}

pub struct IcalText {
    pub value: String,
}

impl TryFrom<Property> for IcalText {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                Ok(IcalText { value })
            },
            None => Err(Error::TypeDecode(ICalTypes::Text))
        }
    }
}

pub enum ICalTime {
    Utc {
        time: NaiveTime,
    },
    Floating {
        time: NaiveTime,
    },
    Local {
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
                match ical_type_parser::time(&value) {
                    Ok(time) => {
                        match (time, tzid) {
                            (ICalTime::Utc { time }, None) => Ok(ICalTime::Utc { time }),
                            (ICalTime::Floating { time }, None) => Ok(ICalTime::Floating { time }),
                            (ICalTime::Floating { time }, Some(tzid)) => Ok(ICalTime::Local { time, tzid }),
                            _ => Err(Error::TypeDecode(ICalTypes::Time))
                        }
                    },
                    Err(_) => Err(Error::TypeDecode(ICalTypes::Time))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::Time))
        }
    }
}

pub struct IcalURI {
    pub value: Uri,
}

impl TryFrom<Property> for IcalURI {
    type Error = Error;
    fn try_from(property: Property) -> Result<Self, Self::Error>{
        match property.value {
            Some(value) => {
                match Uri::from_str(&value) {
                    Ok(value) => Ok(IcalURI { value }),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::URI))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::URI))
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
                match ical_type_parser::utc_offset(&value) {
                    Ok(offset) => Ok(offset),
                    Err(_) => Err(Error::TypeDecode(ICalTypes::UTCOffset))
                }
            },
            None => Err(Error::TypeDecode(ICalTypes::UTCOffset))
        }
    }
}