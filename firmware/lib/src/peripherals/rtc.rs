use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, Timelike, Utc};
use embedded_hal::i2c::I2c;
use log::error;
use thiserror::Error;

use mcp794xx::{ic::Mcp7940n, interface::I2cInterface, DateTimeAccess, Mcp794xx};

#[allow(async_fn_in_trait)]
pub trait RealTimeClock {
    async fn get_datetime(&mut self) -> Result<DateTime<Utc>, RtcError>;
    async fn set_datetime(&mut self, datetime: DateTime<Utc>) -> Result<(), RtcError>;
}

#[derive(Error, Debug, Clone)]
pub enum RtcError {
    #[error("RTC hardware communication failure")]
    HardwareCommunication,
    #[error("Invalid date/time parameters provided")]
    InvalidDateTime,
    #[error("RTC failed to initialize or shutdown properly")]
    InitializationFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SystemTimeInfo {
    time_at_set: DateTime<Utc>,
    micros_at_set: i64,
}

impl SystemTimeInfo {
    pub fn new(time_at_set: DateTime<Utc>, micros_at_set: i64) -> Self {
        Self {
            time_at_set,
            micros_at_set,
        }
    }

    pub const fn default() -> Self {
        Self {
            time_at_set: match DateTime::<Utc>::from_timestamp_millis(0) {
                Some(dt) => dt,
                None => DateTime::<Utc>::UNIX_EPOCH,
            },
            micros_at_set: 0,
        }
    }

    pub fn get_current_time(&self, micros: u64) -> DateTime<Utc> {
        let elapsed_micros = micros as i64 - self.micros_at_set;
        self.time_at_set + TimeDelta::microseconds(elapsed_micros)
    }

    pub fn update_date(
        &self,
        year: i32,
        month: i32,
        day: i32,
        current_ticks: u64,
    ) -> Result<Self, RtcError> {
        let current = self.get_current_time(current_ticks);
        let month_u32 = month.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let day_u32 = day.try_into().map_err(|_| RtcError::InvalidDateTime)?;

        let new_date = NaiveDate::from_ymd_opt(year, month_u32, day_u32)
            .ok_or(RtcError::InvalidDateTime)?;
        let new_datetime = new_date
            .and_hms_opt(
                current.hour(),
                current.minute(),
                current.second(),
            )
            .ok_or(RtcError::InvalidDateTime)?;

        log::info!("New system date: {:?}", new_datetime);

        Ok(Self {
            time_at_set: new_datetime.and_utc(),
            micros_at_set: current_ticks as i64,
        })
    }

    pub fn update_time(
        &self,
        hour: i32,
        minute: i32,
        second: i32,
        current_ticks: u64,
    ) -> Result<Self, RtcError> {
        let current = self.get_current_time(current_ticks);
        let hour_u32 = hour.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let minute_u32 = minute.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let second_u32 = second.try_into().map_err(|_| RtcError::InvalidDateTime)?;

        let new_datetime = current.date_naive()
            .and_hms_opt(hour_u32, minute_u32, second_u32)
            .ok_or(RtcError::InvalidDateTime)?;

        log::info!("New system time: {:?}", new_datetime);

        Ok(Self {
            time_at_set: new_datetime.and_utc(),
            micros_at_set: current_ticks as i64,
        })
    }
}

pub struct Mcp7940<I2C> {
    rtc: Mcp794xx<I2cInterface<I2C>, Mcp7940n>,
}

impl<I2C: I2c> Mcp7940<I2C> {
    pub fn new(i2c_device: I2C) -> Result<Self, RtcError> {
        let mut rtc = Mcp794xx::new_mcp7940n(i2c_device);

        let power_failed = rtc.has_power_failed().map_err(|_| RtcError::HardwareCommunication)?;
        if power_failed {
            log::error!("RTC power failed.  Starting from default date/time");
            rtc.clear_power_failed().map_err(|_| RtcError::HardwareCommunication)?;
        }

        rtc.disable_external_oscillator().map_err(|_| RtcError::HardwareCommunication)?;

        rtc.enable_backup_battery_power()
            .map_err(|_| RtcError::HardwareCommunication)?;

        rtc.enable().map_err(|_| RtcError::HardwareCommunication)?;

        Ok(Self { rtc })
    }

    fn disable(&mut self) -> Result<(), RtcError> {
        self.rtc
            .disable()
            .map_err(|_| RtcError::HardwareCommunication)?;

        for _ in 0..20 {
            if !self
                .rtc
                .is_oscillator_running()
                .map_err(|_| RtcError::HardwareCommunication)?
            {
                return Ok(());
            }
        }

        error!("RTC didn't shut down in time");
        Err(RtcError::InitializationFailure)
    }

    pub fn set_datetime(&mut self, year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> Result<(), RtcError> {
        self.disable()?;

        let month_u32 = month.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let day_u32 = day.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let hour_u32 = hour.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let minute_u32 = minute.try_into().map_err(|_| RtcError::InvalidDateTime)?;
        let second_u32 = second.try_into().map_err(|_| RtcError::InvalidDateTime)?;

        let datetime_to_set = chrono::NaiveDate::from_ymd_opt(year, month_u32, day_u32)
            .ok_or(RtcError::InvalidDateTime)?
            .and_hms_opt(hour_u32, minute_u32, second_u32)
            .ok_or(RtcError::InvalidDateTime)?;

        log::info!("Setting RTC to {:?}", datetime_to_set);

        self.rtc
            .set_datetime(&datetime_to_set)
            .map_err(|_| RtcError::HardwareCommunication)?;

        self.rtc
            .enable()
            .map_err(|_| RtcError::HardwareCommunication)?;

        Ok(())
    }

    pub fn destroy(self) -> I2C {
        self.rtc.destroy()
    }

    pub fn get_datetime(&mut self) -> Result<DateTime<Utc>, RtcError> {
        let datetime = self.rtc.datetime()
            .map_err(|_| RtcError::HardwareCommunication)?;
        Ok(datetime.and_utc())
    }
}

impl<I2C: I2c> RealTimeClock for Mcp7940<I2C> {
    async fn get_datetime(&mut self) -> Result<DateTime<Utc>, RtcError> {
        self.get_datetime()
    }

    async fn set_datetime(&mut self, datetime: DateTime<Utc>) -> Result<(), RtcError> {
        let naive_datetime = datetime.naive_utc();
        self.set_datetime(
            naive_datetime.date().year(),
            naive_datetime.date().month() as i32,
            naive_datetime.date().day() as i32,
            naive_datetime.time().hour() as i32,
            naive_datetime.time().minute() as i32,
            naive_datetime.time().second() as i32,
        )?;
        Ok(())
    }
}
