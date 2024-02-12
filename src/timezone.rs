use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use ical::parser::ical::component::{IcalTimeZone, IcalTimeZoneTransition};
use rrule::{RRule, RRuleSet, RRuleSetIter, Tz};

use crate::{types::{self, IcalDateTime}, Error};

pub struct TimezoneTransition {
    pub local_start_time: NaiveDateTime,
    pub offset: Duration,
    pub r_rules: Option<RRuleSet>,
}

fn make_rrule_datetime(dt: NaiveDateTime) -> DateTime<Tz> {
    DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).with_timezone(&Tz::UTC)
}

impl TryFrom<IcalTimeZoneTransition> for TimezoneTransition {
    type Error = Error;

    fn try_from(value: IcalTimeZoneTransition) -> Result<Self, Self::Error> {
        let mut local_start_time = None;
        let mut offset = None;
        let mut r_rule_list = Vec::new();
        let mut r_date_list = Vec::new();

        for prop in value.properties {
            match prop.name.as_str() {
                "DTSTART" => local_start_time = prop.value.map(|time| types::date_time_parser::date_time(&time).unwrap()),
                "TZOFFSETTO" => offset = prop.value.map(|offset| types::timzeone_parser::utc_offset(&offset).unwrap()),
                "RRULE" => {
                    let rrule = RRule::from_str(&prop.value.unwrap()).or(Err(Error::InvalidTimezone))?;
                    r_rule_list.push(rrule);
                },
                "RDATE" => {
                    let rdate = types::date_time_parser::date_time(&prop.value.unwrap()).or(Err(Error::InvalidTimezone))?;
                    match rdate {
                        IcalDateTime::Floating { date_time } => r_date_list.push(date_time),
                        _ => return Err(Error::InvalidTimezone)
                    }
                },
                _ => {}
            }
        }

        let local_start_time = match local_start_time {
            Some(IcalDateTime::Floating { date_time }) => date_time,
            _ => return Err(Error::InvalidTimezone),
        };
        let offset = offset.ok_or(Error::InvalidTimezone)?;

        let r_rules = if !r_rule_list.is_empty() || !r_date_list.is_empty() {
            let dt_start = make_rrule_datetime(local_start_time);
            let mut r_rules = RRuleSet::new(dt_start);
            for rrule in r_rule_list {
                r_rules = r_rules.rrule(rrule.validate(dt_start).map_err(|_| Error::InvalidTimezone)?);
            }

            for rdate in r_date_list {
                r_rules = r_rules.rdate(make_rrule_datetime(rdate));
            }

            Some(r_rules)
        } else {
            None
        };

        Ok(TimezoneTransition { local_start_time, offset, r_rules })
    }
}

impl<'a> IntoIterator for &'a TimezoneTransition {
    type Item = (NaiveDateTime, Duration);
    type IntoIter = TimezoneTransitionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        let rrule_iter = self.r_rules.as_ref().map(|rrules| rrules.into_iter());
        TimezoneTransitionIter { offset: self.offset, inital: Some(self.local_start_time), rrule_iter }
    }
}

pub struct TimezoneTransitionIter<'a> {
    offset: Duration,
    inital: Option<NaiveDateTime>,
    rrule_iter: Option<RRuleSetIter<'a>>,
}

impl<'a> Iterator for TimezoneTransitionIter<'a> {
    type Item = (NaiveDateTime, Duration);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.inital.take(), self.rrule_iter.as_mut()) {
            (Some(time), _) => Some((time, self.offset)),
            (None, Some(ref mut iter)) => iter.next().map(|time| (time.naive_utc(), self.offset)),
            (None, None) => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let inital = if self.inital.is_some() { 1 } else { 0 };
        let rrule = self.rrule_iter.as_ref().map(|iter| iter.size_hint().0).unwrap_or(0);
        (inital + rrule, None)
    }
}

pub struct Timezone {
    pub tzid: String,
    pub transitions: Vec<TimezoneTransition>,
}

impl Timezone {
    pub fn offset_time(&self, time: NaiveDateTime) -> Result<NaiveDateTime, Error> {
        let mut offset = None;
        let mut last_update = None;
        for transition in &self.transitions {
            transition.r_rules.as_ref().unwrap().into_iter();
            if time >= transition.local_start_time && (last_update.is_none() || transition.local_start_time > last_update.unwrap()) {
                offset = Some(transition.offset);
                last_update = Some(transition.local_start_time);
            }
        }
        
        let offset = offset.ok_or(Error::InvalidTimezone)?;
        Ok(time + offset)
    }

    pub fn to_utc(&self, time: NaiveDateTime) -> Result<DateTime<Utc>, Error> {
        Ok(DateTime::from_naive_utc_and_offset(self.offset_time(time)?, Utc))
    }
}

impl TryFrom<IcalTimeZone> for Timezone {
    type Error = Error;

    fn try_from(value: IcalTimeZone) -> Result<Self, Self::Error> {
        let mut tzid = None;
        for prop in value.properties {
            if prop.name == "TZID" {
                tzid = prop.value;
            }
        }

        let tzid = tzid.ok_or(Error::InvalidTimezone)?;


        let transitions = value.transitions.into_iter().map(|transition| transition.try_into()).collect::<Result<Vec<_>, _>>()?;

        Ok(Timezone { tzid, transitions })

    }
}

pub type TimezoneMap = HashMap<String, Timezone>;