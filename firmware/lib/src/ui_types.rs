cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::vec::Vec;
    } else {
        use alloc::vec::Vec;
    }
}

slint::include_modules!();

use core::fmt;

#[derive(Debug, Clone, Copy)]
pub enum Pump {
    Dose(DosingPump),
    Cfg(Outlet),
}

impl From<Outlet> for Pump {
    fn from(outlet: Outlet) -> Self {
        Pump::Cfg(outlet)
    }
}

impl fmt::Display for SensorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorType::Ph => write!(f, "pH"),
            SensorType::Conductivity => write!(f, "conductivity"),
            SensorType::Orp => write!(f, "ORP"),
            SensorType::Temperature => write!(f, "temperature"),
        }
    }
}

impl From<crate::peripherals::DosingPumpState> for DosingPumpUiState {
    fn from(backend_state: crate::peripherals::DosingPumpState) -> Self {
        Self {
            name: backend_state.name.unwrap_or_default(),
            status: backend_state.status,
            enabled: backend_state.enabled,
            treatment_solution: backend_state.treatment_solution,
        }
    }
}

impl From<crate::peripherals::OutletState> for OutletUiState {
    fn from(backend_state: crate::peripherals::OutletState) -> Self {
        use slint::ModelRc;
        use slint::VecModel;

        let windows: Vec<UiScheduledRunWindow> = backend_state
            .schedule
            .events()
            .iter()
            .map(|event| event.clone().into())
            .collect();

        Self {
            name: backend_state.name.unwrap_or_default(),
            status: backend_state.status,
            outlet_mode: backend_state.mode,
            schedule_windows: ModelRc::new(VecModel::from_slice(&windows)),
            enable: backend_state.enabled,
            stir_seconds: backend_state.stir_seconds.unwrap_or(60) as i32,
            fill_seconds: backend_state.max_fill_seconds.unwrap_or(60) as i32,
        }
    }
}

impl From<crate::config::outlet_schedule::ScheduledEvent> for UiScheduledRunWindow {
    fn from(event: crate::config::outlet_schedule::ScheduledEvent) -> Self {
        use chrono::Timelike;

        let (sunday, monday, tuesday, wednesday, thursday, friday, saturday) =
            event.active_days.to_bools();

        let (duration_value, duration_unit) = if event.run_duration.num_hours() > 0 {
            (event.run_duration.num_hours() as i32, "hours".into())
        } else if event.run_duration.num_minutes() > 0 {
            (event.run_duration.num_minutes() as i32, "minutes".into())
        } else {
            (event.run_duration.num_seconds() as i32, "seconds".into())
        };

        Self {
            start_hour: event.start_time.hour() as i32,
            start_minute: event.start_time.minute() as i32,
            duration_value,
            duration_unit,
            sunday,
            monday,
            tuesday,
            wednesday,
            thursday,
            friday,
            saturday,
        }
    }
}

impl TryFrom<UiScheduledRunWindow> for crate::config::outlet_schedule::ScheduledEvent {
    type Error = &'static str;

    fn try_from(ui_window: UiScheduledRunWindow) -> Result<Self, Self::Error> {
        use crate::config::outlet_schedule::DaysOfWeek;
        use chrono::{Duration, NaiveTime};

        let start_time = NaiveTime::from_hms_opt(
            ui_window.start_hour as u32,
            ui_window.start_minute as u32,
            0,
        )
        .ok_or("Invalid start time")?;

        let run_duration = match ui_window.duration_unit.as_str() {
            "hours" => Duration::hours(ui_window.duration_value as i64),
            "minutes" => Duration::minutes(ui_window.duration_value as i64),
            "seconds" => Duration::seconds(ui_window.duration_value as i64),
            _ => return Err("Invalid duration unit"),
        };

        let active_days = DaysOfWeek::from_bools(
            ui_window.sunday,
            ui_window.monday,
            ui_window.tuesday,
            ui_window.wednesday,
            ui_window.thursday,
            ui_window.friday,
            ui_window.saturday,
        );

        Ok(Self {
            start_time,
            run_duration,
            active_days,
        })
    }
}
