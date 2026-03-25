cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::vec::Vec;
    } else {
        use alloc::vec::Vec;
    }
}

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// Bitmask for days of week. Bit 0 = Sunday, Bit 6 = Saturday
/// Example: 0b0111110 = Monday through Friday
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaysOfWeek(u8);

impl DaysOfWeek {
    pub const SUNDAY: u8 = 1 << 0;
    pub const MONDAY: u8 = 1 << 1;
    pub const TUESDAY: u8 = 1 << 2;
    pub const WEDNESDAY: u8 = 1 << 3;
    pub const THURSDAY: u8 = 1 << 4;
    pub const FRIDAY: u8 = 1 << 5;
    pub const SATURDAY: u8 = 1 << 6;

    pub const EVERY_DAY: u8 = 0b0111_1111;

    pub const fn new(days: u8) -> Self {
        Self(days & Self::EVERY_DAY)
    }

    pub const fn every_day() -> Self {
        Self(Self::EVERY_DAY)
    }

    pub const fn is_active_on(&self, day: u8) -> bool {
        (self.0 & (1 << day)) != 0
    }

    pub fn from_bools(
        sunday: bool,
        monday: bool,
        tuesday: bool,
        wednesday: bool,
        thursday: bool,
        friday: bool,
        saturday: bool,
    ) -> Self {
        let mut mask = 0u8;
        if sunday {
            mask |= Self::SUNDAY;
        }
        if monday {
            mask |= Self::MONDAY;
        }
        if tuesday {
            mask |= Self::TUESDAY;
        }
        if wednesday {
            mask |= Self::WEDNESDAY;
        }
        if thursday {
            mask |= Self::THURSDAY;
        }
        if friday {
            mask |= Self::FRIDAY;
        }
        if saturday {
            mask |= Self::SATURDAY;
        }
        Self(mask)
    }

    pub fn to_bools(self) -> (bool, bool, bool, bool, bool, bool, bool) {
        (
            self.is_active_on(0),
            self.is_active_on(1),
            self.is_active_on(2),
            self.is_active_on(3),
            self.is_active_on(4),
            self.is_active_on(5),
            self.is_active_on(6),
        )
    }
}

impl Default for DaysOfWeek {
    fn default() -> Self {
        Self::every_day()
    }
}

/// A single scheduled event: run at a specific time for a duration
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledEvent {
    pub start_time: NaiveTime,
    pub run_duration: Duration,
    pub active_days: DaysOfWeek,
}

impl ScheduledEvent {
    pub fn new(start_time: NaiveTime, run_duration: Duration) -> Self {
        Self {
            start_time,
            run_duration,
            active_days: DaysOfWeek::every_day(),
        }
    }

    pub fn with_days(mut self, days: DaysOfWeek) -> Self {
        self.active_days = days;
        self
    }

    /// Check if this event should run on the given day of week (0 = Sunday)
    pub const fn is_active_on_day(&self, day_of_week: u8) -> bool {
        self.active_days.is_active_on(day_of_week)
    }

    /// Get the time when this event ends (may wrap past midnight)
    pub fn end_time(&self) -> NaiveTime {
        self.start_time + self.run_duration
    }

    /// Check if a given time falls within this event's active period
    /// Handles wrap-around past midnight
    pub fn is_active_at(&self, time: NaiveTime) -> bool {
        let end = self.end_time();

        if self.start_time <= end {
            time >= self.start_time && time < end
        } else {
            time >= self.start_time || time < end
        }
    }
}

/// Schedule for a configurable outlet
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutletSchedule {
    /// List of scheduled events, should be kept sorted by start_time for efficiency
    events: Vec<ScheduledEvent>,
}

impl OutletSchedule {
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn with_events(events: Vec<ScheduledEvent>) -> Self {
        let mut schedule = Self { events };
        schedule.sort_events();
        schedule
    }

    pub fn add_event(&mut self, event: ScheduledEvent) {
        self.events.push(event);
        self.sort_events();
    }

    pub fn remove_event(&mut self, index: usize) -> Option<ScheduledEvent> {
        if index < self.events.len() {
            Some(self.events.remove(index))
        } else {
            None
        }
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    /// Sort events by start time for efficient lookup
    fn sort_events(&mut self) {
        self.events.sort_by_key(|e| e.start_time);
    }

    /// Check if the outlet should be running at a given time and day
    /// Returns Some(event_index) if active, None if not
    pub fn is_active_at(&self, time: NaiveTime, day_of_week: u8) -> Option<usize> {
        for (i, event) in self.events.iter().enumerate() {
            if !event.is_active_on_day(day_of_week) {
                continue;
            }

            if event.is_active_at(time) {
                return Some(i);
            }
        }

        None
    }

    pub fn next_transition(&self, now: DateTime<Utc>, tz: Tz) -> Option<ScheduleTransition> {
        let mut best: Option<ScheduleTransition> = None;

        for event in self.events.iter() {
            for candidate in event.next_boundaries(now, tz) {
                if best.as_ref().is_none_or(|b| candidate.at < b.at) {
                    best = Some(candidate);
                }
            }
        }

        best
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleTransition {
    pub at: DateTime<Utc>,
    pub turn_on: bool,
}

impl ScheduledEvent {
    fn local_to_utc(naive: chrono::NaiveDateTime, tz: Tz) -> DateTime<Utc> {
        match tz.from_local_datetime(&naive) {
            chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
            chrono::LocalResult::None => {
                let shifted = naive + Duration::hours(1);
                tz.from_local_datetime(&shifted)
                    .earliest()
                    .unwrap_or_else(|| tz.from_utc_datetime(&naive))
                    .with_timezone(&Utc)
            }
        }
    }

    fn next_boundaries(&self, now: DateTime<Utc>, tz: Tz) -> Vec<ScheduleTransition> {
        let mut candidates = Vec::new();
        let now_local = now.with_timezone(&tz);
        let now_time = now_local.time();
        let today = now_local.date_naive();
        let day_of_week = now_local.weekday().num_days_from_sunday() as u8;

        if self.is_active_on_day(day_of_week) && self.is_active_at(now_time) {
            let end_time = self.end_time();
            let end_date = if end_time <= self.start_time {
                today + Duration::days(1)
            } else {
                today
            };
            let at = Self::local_to_utc(end_date.and_time(end_time), tz);
            if at > now {
                candidates.push(ScheduleTransition { at, turn_on: false });
            }
        }

        for days_ahead in 0..=7i64 {
            let candidate_date = today + Duration::days(days_ahead);
            let candidate_dow = ((day_of_week as i64 + days_ahead) % 7) as u8;
            if !self.is_active_on_day(candidate_dow) {
                continue;
            }
            let at = Self::local_to_utc(candidate_date.and_time(self.start_time), tz);
            if at > now {
                candidates.push(ScheduleTransition { at, turn_on: true });
                break;
            }
        }

        candidates
    }
}

pub struct ScheduledTransitions {
    pub at: DateTime<Utc>,
    pub outlets: Vec<(usize, bool)>,
}

pub fn compute_next_schedule_change(
    now: DateTime<Utc>,
    schedules: &[&OutletSchedule],
    tz: Tz,
) -> Option<ScheduledTransitions> {
    let mut earliest: Option<DateTime<Utc>> = None;

    for schedule in schedules.iter() {
        if let Some(transition) = schedule.next_transition(now, tz) {
            if earliest.is_none_or(|e| transition.at < e) {
                earliest = Some(transition.at);
            }
        }
    }

    let earliest = earliest?;

    let mut outlets = Vec::new();
    for (i, schedule) in schedules.iter().enumerate() {
        if let Some(transition) = schedule.next_transition(now, tz) {
            if transition.at == earliest {
                outlets.push((i, transition.turn_on));
            }
        }
    }

    Some(ScheduledTransitions {
        at: earliest,
        outlets,
    })
}

impl Default for OutletSchedule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    const UTC: Tz = Tz::UTC;

    #[test]
    fn test_days_of_week_bitmask() {
        let weekdays = DaysOfWeek::from_bools(false, true, true, true, true, true, false);
        assert!(weekdays.is_active_on(1)); // Monday
        assert!(weekdays.is_active_on(5)); // Friday
        assert!(!weekdays.is_active_on(0)); // Sunday
        assert!(!weekdays.is_active_on(6)); // Saturday
    }

    #[test]
    fn test_scheduled_event() {
        let event = ScheduledEvent::new(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            Duration::minutes(30),
        )
        .with_days(DaysOfWeek::from_bools(
            false, true, true, true, true, true, false,
        ));

        assert!(event.is_active_on_day(1)); // Monday
        assert!(!event.is_active_on_day(0)); // Sunday

        let end = event.end_time();
        assert_eq!(end.hour(), 6);
        assert_eq!(end.minute(), 30);
    }

    #[test]
    fn test_event_is_active_at() {
        let event = ScheduledEvent::new(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            Duration::minutes(30),
        );

        let during = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        let before = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let after = NaiveTime::from_hms_opt(7, 0, 0).unwrap();

        assert!(event.is_active_at(during));
        assert!(!event.is_active_at(before));
        assert!(!event.is_active_at(after));
    }

    #[test]
    fn test_event_crosses_midnight() {
        let event = ScheduledEvent::new(
            NaiveTime::from_hms_opt(23, 30, 0).unwrap(),
            Duration::minutes(60),
        );

        let during_before_midnight = NaiveTime::from_hms_opt(23, 45, 0).unwrap();
        let during_after_midnight = NaiveTime::from_hms_opt(0, 15, 0).unwrap();
        let outside = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        assert!(event.is_active_at(during_before_midnight));
        assert!(event.is_active_at(during_after_midnight));
        assert!(!event.is_active_at(outside));

        let end = event.end_time();
        assert_eq!(end.hour(), 0);
        assert_eq!(end.minute(), 30);
    }

    #[test]
    fn test_outlet_schedule_is_active() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            Duration::minutes(30),
        ));

        let during = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        let before = NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let after = NaiveTime::from_hms_opt(7, 0, 0).unwrap();

        assert!(schedule.is_active_at(during, 0).is_some());
        assert!(schedule.is_active_at(before, 0).is_none());
        assert!(schedule.is_active_at(after, 0).is_none());
    }

    #[test]
    fn test_outlet_schedule_day_of_week() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(
            ScheduledEvent::new(
                NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
                Duration::minutes(30),
            )
            .with_days(DaysOfWeek::from_bools(
                false, true, true, true, true, true, false,
            )),
        );

        let during = NaiveTime::from_hms_opt(6, 15, 0).unwrap();

        assert!(schedule.is_active_at(during, 1).is_some()); // Monday
        assert!(schedule.is_active_at(during, 0).is_none()); // Sunday
    }

    #[test]
    fn test_serialization() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(
            ScheduledEvent::new(
                NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
                Duration::minutes(30),
            )
            .with_days(DaysOfWeek::from_bools(
                false, true, true, true, true, true, false,
            )),
        );

        let serialized = serde_json::to_string(&schedule).unwrap();
        let deserialized: OutletSchedule = serde_json::from_str(&serialized).unwrap();

        assert_eq!(schedule, deserialized);
    }

    #[test]
    fn test_multiple_events_sorted() {
        let mut schedule = OutletSchedule::new();

        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            Duration::minutes(15),
        ));

        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            Duration::minutes(15),
        ));

        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            Duration::minutes(15),
        ));

        let events = schedule.events();
        assert_eq!(events[0].start_time.hour(), 6);
        assert_eq!(events[1].start_time.hour(), 12);
        assert_eq!(events[2].start_time.hour(), 18);
    }

    use chrono::NaiveDate;

    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, min, sec)
            .unwrap()
            .and_utc()
    }

    #[test]
    fn test_next_transition_before_event() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            Duration::minutes(30),
        ));

        let now = utc(2026, 2, 17, 9, 0, 0); // Tuesday
        let transition = schedule.next_transition(now, UTC).unwrap();
        assert!(transition.turn_on);
        assert_eq!(transition.at, utc(2026, 2, 17, 10, 0, 0));
    }

    #[test]
    fn test_next_transition_during_event() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            Duration::minutes(30),
        ));

        let now = utc(2026, 2, 17, 10, 15, 0);
        let transition = schedule.next_transition(now, UTC).unwrap();
        assert!(!transition.turn_on);
        assert_eq!(transition.at, utc(2026, 2, 17, 10, 30, 0));
    }

    #[test]
    fn test_next_transition_after_all_events_today() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            Duration::minutes(30),
        ));

        let now = utc(2026, 2, 17, 20, 0, 0);
        let transition = schedule.next_transition(now, UTC).unwrap();
        assert!(transition.turn_on);
        assert_eq!(transition.at, utc(2026, 2, 18, 6, 0, 0));
    }

    #[test]
    fn test_next_transition_empty_schedule() {
        let schedule = OutletSchedule::new();
        let now = utc(2026, 2, 17, 12, 0, 0);
        assert!(schedule.next_transition(now, UTC).is_none());
    }

    #[test]
    fn test_next_transition_not_active_today_but_tomorrow() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(
            ScheduledEvent::new(
                NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
                Duration::minutes(30),
            )
            .with_days(DaysOfWeek::new(DaysOfWeek::WEDNESDAY)),
        );

        let now = utc(2026, 2, 17, 12, 0, 0); // Tuesday
        let transition = schedule.next_transition(now, UTC).unwrap();
        assert!(transition.turn_on);
        assert_eq!(transition.at, utc(2026, 2, 18, 8, 0, 0));
    }

    #[test]
    fn test_next_transition_picks_soonest() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(14, 0, 0).unwrap(),
            Duration::minutes(30),
        ));
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            Duration::minutes(15),
        ));

        let now = utc(2026, 2, 17, 9, 0, 0);
        let transition = schedule.next_transition(now, UTC).unwrap();
        assert!(transition.turn_on);
        assert_eq!(transition.at, utc(2026, 2, 17, 10, 0, 0));
    }

    #[test]
    fn test_next_transition_midnight_crossing() {
        let mut schedule = OutletSchedule::new();
        schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(23, 30, 0).unwrap(),
            Duration::minutes(60),
        ));

        let now = utc(2026, 2, 17, 23, 45, 0);
        let transition = schedule.next_transition(now, UTC).unwrap();
        assert!(!transition.turn_on);
        assert_eq!(transition.at, utc(2026, 2, 18, 0, 30, 0));
    }

    #[test]
    fn test_compute_next_schedule_change_across_outlets() {
        let mut schedule_a = OutletSchedule::new();
        schedule_a.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(14, 0, 0).unwrap(),
            Duration::minutes(30),
        ));

        let mut schedule_b = OutletSchedule::new();
        schedule_b.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            Duration::minutes(15),
        ));

        let now = utc(2026, 2, 17, 9, 0, 0);
        let result = compute_next_schedule_change(now, &[&schedule_a, &schedule_b], UTC).unwrap();
        assert_eq!(result.at, utc(2026, 2, 17, 10, 0, 0));
        assert_eq!(result.outlets, vec![(1, true)]);
    }

    #[test]
    fn test_compute_next_schedule_change_simultaneous_transitions() {
        let mut schedule_a = OutletSchedule::new();
        schedule_a.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            Duration::minutes(30),
        ));

        let mut schedule_b = OutletSchedule::new();
        schedule_b.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            Duration::minutes(15),
        ));

        let now = utc(2026, 2, 17, 9, 0, 0);
        let result = compute_next_schedule_change(now, &[&schedule_a, &schedule_b], UTC).unwrap();
        assert_eq!(result.at, utc(2026, 2, 17, 10, 0, 0));
        assert_eq!(result.outlets, vec![(0, true), (1, true)]);
    }
}
