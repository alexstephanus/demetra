pub mod device;
pub mod outlet;
pub mod pump;
pub mod sensor;

use chrono::{DateTime, Utc};
use core::fmt::Debug;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embedded_storage::Storage;

use crate::{
    config::device_config::DeviceConfig,
    peripherals::{rtc::RealTimeClock, PumpController, SensorReadRaw, TreatmentControllerMutex},
    storage::ring_buffer::{EmptyMetadata, RingBuffer},
};

pub struct MessageContext<'a, 'b, Rtc, Sensors: SensorReadRaw, Pumps: PumpController, S, E> {
    pub current_timestamp: DateTime<Utc>,
    pub current_ticks: u64,
    pub rtc: &'b mut Rtc,
    pub treatment_controller: &'a TreatmentControllerMutex<'a, Sensors, Pumps>,
    pub config_buffer: &'b mut RingBuffer<DeviceConfig, EmptyMetadata, S, E>,
}

pub const UI_ACTION_CHANNEL_SIZE: usize = 25;

macro_rules! ui_actions {
    ($($Action:ident),* $(,)?) => {
        #[derive(Debug, Clone)]
        pub enum UiMessage {
            $($Action($Action)),*
        }

        pub static UI_ACTION_CHANNEL: Channel<CriticalSectionRawMutex, UiMessage, UI_ACTION_CHANNEL_SIZE> = Channel::new();

        pub fn register_all_callbacks(ui: &crate::ui_types::MainWindow) {
            register_all_callbacks_with_sender(ui, |msg| {
                if UI_ACTION_CHANNEL.try_send(msg).is_err() {
                    log::error!("UI action channel full");
                }
            });
        }

        pub fn register_all_callbacks_with_sender(
            ui: &crate::ui_types::MainWindow,
            send: impl Fn(UiMessage) + 'static + Clone,
        ) {
            $(
                let send_clone = send.clone();
                $Action::register_callback(ui, move |a| {
                    send_clone(UiMessage::$Action(a));
                });
            )*
        }

        pub async fn dispatch<
            'a,
            Rtc: RealTimeClock,
            Sensors: SensorReadRaw,
            Pumps: PumpController,
            S: Storage<Error = E>,
            E: Debug,
        >(
            msg: UiMessage,
            ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
        ) {
            match msg {
                $(UiMessage::$Action(action) => action.handle(ctx).await),*
            }
        }
    };
}

use device::*;
use outlet::*;
use pump::*;
use sensor::*;

ui_actions!(
    // Device
    SetTankSize,
    SetTemperatureDisplayUnit,
    SetConductivityDisplayUnit,
    SetDate,
    SetTime,
    // Sensor
    EnableSensor,
    DisableSensor,
    SetSensorMinValue,
    SetSensorMaxValue,
    SetThermistorBeta,
    ReadSensorRaw,
    ReadSensorCalibrated,
    FetchSensorChart,
    MeasureAndCalibrateEc,
    MeasureAndCalibrateOrp,
    SavePhCalibration,
    // Pump
    EnableDosingPump,
    DisableDosingPump,
    SetDosingPumpStatus,
    RenameDosingPump,
    SetTreatmentSolution,
    CalibrateDosingPump,
    RunDosingPump,
    RunDosingPumpVolumetric,
    StartDosingPump,
    StopDosingPump,
    // Outlet
    EnableOutlet,
    DisableOutlet,
    RenameOutlet,
    SetOutletMode,
    RunOutlet,
    AddScheduleWindow,
    UpdateScheduleWindow,
    DeleteScheduleWindow,
    SetSolenoidFillTime,
    SetStirPumpDuration,
);

#[cfg(test)]
pub(crate) mod test_helpers {
    use std::sync::{Arc, Mutex as StdMutex};

    use super::{MessageContext, UiMessage};
    use crate::{
        config::device_config::DeviceConfig,
        peripherals::{
            rtc::{RealTimeClock, RtcError},
            Pump, PumpController, PumpError, SensorController, SensorError, SensorReadRaw,
            TreatmentController,
        },
        storage::ring_buffer::{
            EmptyMetadata, MockFlashStorage, MockFlashStorageError, RingBuffer,
        },
        ui_types::{MainWindow, SensorType},
    };
    use chrono::{DateTime, Utc};

    pub struct StubRtc;

    impl RealTimeClock for StubRtc {
        async fn get_datetime(&mut self) -> Result<DateTime<Utc>, RtcError> {
            Ok(DateTime::<Utc>::from_timestamp_millis(0).unwrap())
        }
        async fn set_datetime(&mut self, _datetime: DateTime<Utc>) -> Result<(), RtcError> {
            Ok(())
        }
    }

    pub struct StubSensors;

    impl SensorReadRaw for StubSensors {
        async fn turn_sensors_on(&mut self) -> Result<(), SensorError> {
            Ok(())
        }
        async fn turn_sensors_off(&mut self) -> Result<(), SensorError> {
            Ok(())
        }
        async fn read_sensor_raw(&mut self, sensor: SensorType) -> Result<f32, SensorError> {
            Err(SensorError::HardwareReadFailure(sensor))
        }
        fn adc_mv_to_sensor_value(&self, _sensor_type: SensorType, raw_adc_mv: f32) -> f32 {
            raw_adc_mv
        }
    }

    pub struct StubPumps;

    impl PumpController for StubPumps {
        async fn enable_pump(&mut self, _pump: &Pump) -> Result<(), PumpError> {
            Ok(())
        }
        async fn disable_pump(&mut self, _pump: &Pump) -> Result<(), PumpError> {
            Ok(())
        }
        async fn read_current(&mut self, _pump: &Pump) -> Result<f32, PumpError> {
            Ok(0.0)
        }
        async fn turn_off_all(&mut self) -> Result<(), PumpError> {
            Ok(())
        }
        fn is_pump_enabled(&mut self, _pump: &Pump) -> Result<bool, PumpError> {
            Ok(false)
        }
        fn enable_relay(&mut self) {}
        fn kill_relay(&mut self) {}
    }

    pub fn mock_ring_buffer(
    ) -> RingBuffer<DeviceConfig, EmptyMetadata, MockFlashStorage, MockFlashStorageError> {
        let start_address = 0x0000;
        let end_address = 0x4000;
        RingBuffer::<DeviceConfig, EmptyMetadata, MockFlashStorage, MockFlashStorageError>::new(
            start_address,
            end_address,
            MockFlashStorage::new(start_address, end_address, None),
        )
        .expect("test ring buffer addresses must be page-aligned")
    }

    pub type TestTreatmentController<'a> = TreatmentController<'a, StubSensors, StubPumps>;
    pub type TestMessageContext<'a, 'b> = MessageContext<
        'a,
        'b,
        StubRtc,
        StubSensors,
        StubPumps,
        MockFlashStorage,
        MockFlashStorageError,
    >;

    pub fn mock_treatment_controller<'a>() -> TestTreatmentController<'a> {
        TreatmentController::initialize(StubPumps, SensorController::new(StubSensors))
    }

    pub struct TestHarness {
        pub ui: MainWindow,
        pub messages: Arc<StdMutex<Vec<UiMessage>>>,
        last_pumps: crate::peripherals::DosingPumpStateList,
        last_outlets: crate::peripherals::OutletStateList,
    }

    impl TestHarness {
        pub fn new() -> Self {
            i_slint_backend_testing::init_no_event_loop();
            let ui = MainWindow::new().unwrap();
            let messages: Arc<StdMutex<Vec<UiMessage>>> = Arc::new(StdMutex::new(Vec::new()));
            let captured = messages.clone();
            super::register_all_callbacks_with_sender(&ui, move |msg| {
                captured.lock().unwrap().push(msg);
            });
            Self {
                ui,
                messages,
                last_pumps: crate::peripherals::DosingPumpStateList::default(),
                last_outlets: crate::peripherals::OutletStateList::default(),
            }
        }

        pub fn take_messages(&self) -> Vec<UiMessage> {
            std::mem::take(&mut *self.messages.lock().unwrap())
        }

        pub async fn dispatch_all(&self, ctx: &mut TestMessageContext<'_, '_>) {
            for msg in self.take_messages() {
                super::dispatch(msg, ctx).await;
            }
        }

        pub async fn sync_to_ui(&mut self) {
            crate::ui_backend::sync_runtime_state_to_ui(&self.ui).await;
            crate::ui_backend::sync_device_config_to_ui(
                &self.ui,
                &mut self.last_pumps,
                &mut self.last_outlets,
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use crate::{
        config::calibration::DosingPumpCalibration,
        peripherals::DosingPump,
        state::get_system_time,
        storage::get_device_config,
        ui_types::{
            AppUiState, ConductivityDisplayUnit, PumpUiState, SensorType, SensorUiState, Status,
            TemperatureDisplayUnit, TreatmentSolutionType, UiTreatmentSolution, WorkflowUiState,
        },
        units::Volume,
    };
    use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
    use slint::{ComponentHandle, Model};

    #[tokio::test]
    #[rstest::rstest]
    #[case(DosingPump::DoseOne, 1)]
    #[case(DosingPump::DoseTwo, 2)]
    #[case(DosingPump::DoseThree, 3)]
    #[case(DosingPump::DoseFour, 4)]
    #[case(DosingPump::DoseFive, 5)]
    #[case(DosingPump::DoseSix, 6)]
    async fn test_calibrate_dosing_pump(#[case] pump: DosingPump, #[case] pump_number: i32) {
        use crate::config::calibration::DoseCalibrationPoint;

        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        let (vol_3s, vol_10s, vol_30s) = (1.5, 5.0, 15.0);
        harness
            .ui
            .global::<WorkflowUiState>()
            .invoke_save_dosing_pump_calibration(pump_number, vol_3s, vol_10s, vol_30s);
        harness.dispatch_all(&mut ctx).await;

        let expected = DosingPumpCalibration::new(
            DoseCalibrationPoint::new(3000.0, vol_3s),
            DoseCalibrationPoint::new(10000.0, vol_10s),
            DoseCalibrationPoint::new(30000.0, vol_30s),
            get_system_time(0).await,
        );
        assert_eq!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(pump)
                .calibration,
            expected,
        );
    }

    #[tokio::test]
    async fn test_rename_dosing_pump() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<PumpUiState>()
            .invoke_rename_pump(0, "New Dose One".into());
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .name,
            Some(slint::SharedString::from("New Dose One"))
        );
    }

    #[tokio::test]
    async fn test_toggle_dosing_pump() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness.ui.global::<PumpUiState>().invoke_enable_pump(0);
        harness.dispatch_all(&mut ctx).await;
        assert!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .enabled
        );

        harness.ui.global::<PumpUiState>().invoke_enable_pump(0);
        harness.dispatch_all(&mut ctx).await;
        assert!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .enabled
        );

        harness.ui.global::<PumpUiState>().invoke_disable_pump(0);
        harness.dispatch_all(&mut ctx).await;
        assert!(
            !get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .enabled
        );

        harness.ui.global::<PumpUiState>().invoke_disable_pump(0);
        harness.dispatch_all(&mut ctx).await;
        assert!(
            !get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .enabled
        );
    }

    #[tokio::test]
    async fn test_set_dosing_pump_status() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<PumpUiState>()
            .invoke_set_dosing_pump_status(0, Status::Error);
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .status,
            Status::Error,
        );

        harness
            .ui
            .global::<PumpUiState>()
            .invoke_set_dosing_pump_status(0, Status::Ok);
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .status,
            Status::Ok,
        );
    }

    #[tokio::test]
    async fn test_set_treatment_solution() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<PumpUiState>()
            .invoke_update_treatment_solution(
                0,
                UiTreatmentSolution {
                    solution_type: TreatmentSolutionType::PhDown,
                    solution_strength: 4.0,
                },
            );
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config()
                .await
                .pumps
                .get_dosing_pump_state(DosingPump::DoseOne)
                .treatment_solution,
            UiTreatmentSolution {
                solution_type: TreatmentSolutionType::PhDown,
                solution_strength: 4.0,
            }
        );
    }

    #[tokio::test]
    async fn test_set_conductivity_display_unit() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<AppUiState>()
            .invoke_set_conductivity_display_unit(ConductivityDisplayUnit::UsPerCm);
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config().await.conductivity_display_unit,
            ConductivityDisplayUnit::UsPerCm
        );
    }

    #[tokio::test]
    async fn test_set_temperature_display_unit() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<AppUiState>()
            .invoke_set_temperature_display_unit(TemperatureDisplayUnit::Fahrenheit);
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config().await.temperature_display_unit,
            TemperatureDisplayUnit::Fahrenheit
        );
    }

    #[tokio::test]
    async fn test_set_tank_size() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness.ui.global::<AppUiState>().invoke_set_tank_size(10.0);
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config().await.tank_size,
            Volume::from_liters(10.0)
        );
    }

    #[tokio::test]
    async fn test_set_thermistor_beta() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<SensorUiState>()
            .invoke_set_thermistor_beta(4000.0);
        harness.dispatch_all(&mut ctx).await;

        assert_eq!(
            get_device_config().await.temperature.beta_value,
            Some(4000.0)
        );
    }

    #[tokio::test]
    async fn test_enable_disable_sensor() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<SensorUiState>()
            .invoke_enable_sensor(SensorType::Ph);
        harness.dispatch_all(&mut ctx).await;
        assert!(get_device_config().await.ph.enabled);

        harness
            .ui
            .global::<SensorUiState>()
            .invoke_disable_sensor(SensorType::Ph);
        harness.dispatch_all(&mut ctx).await;
        assert!(!get_device_config().await.ph.enabled);
    }

    #[tokio::test]
    async fn test_set_sensor_min_max() {
        let harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<SensorUiState>()
            .invoke_set_sensor_min_value(SensorType::Ph, 5.5);
        harness
            .ui
            .global::<SensorUiState>()
            .invoke_set_sensor_max_value(SensorType::Ph, 7.5);
        harness.dispatch_all(&mut ctx).await;

        let config = get_device_config().await;
        assert_eq!(config.ph.min_acceptable, 5.5);
        assert_eq!(config.ph.max_acceptable, 7.5);
    }

    #[tokio::test]
    async fn test_roundtrip_tank_size() {
        let mut harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness.ui.global::<AppUiState>().invoke_set_tank_size(42.0);
        harness.dispatch_all(&mut ctx).await;
        harness.sync_to_ui().await;

        assert_eq!(harness.ui.global::<AppUiState>().get_tank_size(), 42.0);
    }

    #[tokio::test]
    async fn test_roundtrip_temperature_display_unit() {
        let mut harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<AppUiState>()
            .invoke_set_temperature_display_unit(TemperatureDisplayUnit::Fahrenheit);
        harness.dispatch_all(&mut ctx).await;
        harness.sync_to_ui().await;

        assert_eq!(
            harness
                .ui
                .global::<AppUiState>()
                .get_temperature_display_unit(),
            TemperatureDisplayUnit::Fahrenheit
        );
    }

    #[tokio::test]
    async fn test_roundtrip_enable_pump() {
        let mut harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness.ui.global::<PumpUiState>().invoke_enable_pump(0);
        harness.dispatch_all(&mut ctx).await;
        harness.sync_to_ui().await;

        let pump_states = harness.ui.global::<PumpUiState>().get_dosing_pump_states();
        assert!(pump_states.row_data(0).unwrap().enabled);
    }

    #[tokio::test]
    async fn test_roundtrip_sensor_config() {
        let mut harness = TestHarness::new();
        let mut buffer = mock_ring_buffer();
        let mut rtc = StubRtc;
        let tc = mock_treatment_controller();
        let tc_mutex: Mutex<NoopRawMutex, _> = Mutex::new(tc);
        let mut ctx = super::MessageContext {
            current_timestamp: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(0).unwrap(),
            current_ticks: 0,
            rtc: &mut rtc,
            treatment_controller: &tc_mutex,
            config_buffer: &mut buffer,
        };

        harness
            .ui
            .global::<SensorUiState>()
            .invoke_enable_sensor(SensorType::Ph);
        harness
            .ui
            .global::<SensorUiState>()
            .invoke_set_sensor_min_value(SensorType::Ph, 5.5);
        harness
            .ui
            .global::<SensorUiState>()
            .invoke_set_sensor_max_value(SensorType::Ph, 7.5);
        harness.dispatch_all(&mut ctx).await;
        harness.sync_to_ui().await;

        let sensors = harness.ui.global::<SensorUiState>();
        assert!(sensors.get_ph_enabled());
        assert_eq!(sensors.get_ph_min_acceptable(), 5.5);
        assert_eq!(sensors.get_ph_max_acceptable(), 7.5);
    }
}
