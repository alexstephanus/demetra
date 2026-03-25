#![cfg_attr(all(not(test), not(feature = "simulation")), no_std)]

#[cfg(all(not(test), not(feature = "simulation")))]
extern crate alloc;

#[deny(unsafe_code)]
pub mod config;
pub mod logging;
pub mod peripherals;
pub mod state;
pub mod storage;
pub mod tasks;
pub mod ui_backend;
pub mod ui_types;
pub mod units;
