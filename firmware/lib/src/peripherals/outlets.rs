cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::format;
    } else {
        use alloc::format;
    }
}

use crate::config::outlet_schedule::OutletSchedule;
use crate::ui_types::{Outlet, OutletMode, Status};
use slint::SharedString;

impl Outlet {
    pub fn to_int(&self) -> usize {
        match self {
            Outlet::One => 0,
            Outlet::Two => 1,
            Outlet::Three => 2,
            Outlet::Four => 3,
        }
    }

    pub fn from_int(int: usize) -> Option<Self> {
        match int {
            0 => Some(Outlet::One),
            1 => Some(Outlet::Two),
            2 => Some(Outlet::Three),
            3 => Some(Outlet::Four),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct OutletState {
    pub outlet: Outlet,
    pub name: Option<SharedString>,
    pub status: Status,
    pub enabled: bool,
    pub mode: OutletMode,
    pub schedule: OutletSchedule,
    pub stir_seconds: Option<u32>,
    pub max_fill_seconds: Option<u32>,
}

impl OutletState {
    pub const fn default(outlet: Outlet) -> Self {
        Self {
            outlet,
            name: None,
            status: Status::Ok,
            enabled: false,
            mode: OutletMode::Unconfigured,
            schedule: OutletSchedule::new(),
            stir_seconds: None,
            max_fill_seconds: None,
        }
    }

    pub fn get_label(&self) -> SharedString {
        match &self.name {
            Some(set_name) => set_name.clone(),
            None => SharedString::from(format!("Outlet {}", self.outlet.to_int() + 1)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct OutletStateList {
    outlets: [OutletState; 4],
}

impl OutletStateList {
    pub const fn default() -> Self {
        Self {
            outlets: [
                OutletState::default(Outlet::One),
                OutletState::default(Outlet::Two),
                OutletState::default(Outlet::Three),
                OutletState::default(Outlet::Four),
            ],
        }
    }

    pub fn get_outlet_state(&self, outlet: Outlet) -> OutletState {
        self.outlets[outlet.to_int()].clone()
    }

    pub fn get_outlet_state_mut(&mut self, outlet: Outlet) -> &mut OutletState {
        &mut self.outlets[outlet.to_int()]
    }

    pub fn get_stir_outlet(&self) -> Option<&OutletState> {
        self.outlets
            .iter()
            .find(|o| o.enabled && o.mode == OutletMode::StirPump)
    }

    pub fn get_solenoid_outlet(&self) -> Option<&OutletState> {
        self.outlets
            .iter()
            .find(|o| o.enabled && o.mode == OutletMode::Solenoid)
    }

    pub fn get_fertigation_outlets(&self) -> impl Iterator<Item = Outlet> + '_ {
        self.outlets.iter().filter_map(|o| {
            if o.enabled && o.mode == OutletMode::FertigationPump {
                Some(o.outlet)
            } else {
                None
            }
        })
    }
}
