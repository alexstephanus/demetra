
use lib::{
    data::{
        ring_buffer::{MockFlashStorage, MockFlashStorageError, RingBuffer, EmptyMetadata},
        configuration::DeviceConfig,
    },
    ui_backend::{
        actions::{UI_ACTION_CHANNEL, UiMessage, MessageContext},
        ui_runner::{WINDOW_EVENT_CHANNEL, WINDOW_EVENT_CHANNEL_SIZE, FRAME_PIXELS},
    },
    ui_types::MainWindow,
    peripherals::{TreatmentController, SensorController},
};
use anyhow::Result;
use slint::{
    platform::{
        software_renderer::MinimalSoftwareWindow,
        WindowEvent,
    },
    ComponentHandle,
};

use crate::{
    display::Sdl2Renderer,
    mocks::{CliSensors, MockRtc},
};

use std::boxed::Box;

mod display;
mod mocks;
mod storage;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    lib::state::init_clock(|| chrono::Utc::now().timestamp_micros() as u64)
        .expect("Clock already initialized");

    println!("Demetra Simulation Backend Starting...");
    println!("This simulation will prompt for sensor readings via CLI");
    println!("UI will open in a separate window");
    println!();
    
    let _config_storage = storage::create_config_storage()?;
    
    
    
    println!("Showing UI");
    
    let slint_window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer
    );

    let unboxed_slint_backend = display::SimulationBackend::new(slint_window.clone());
    
    let slint_backend = Box::new(unboxed_slint_backend);
    
    slint::platform::set_platform(slint_backend).expect("backend already initialized");
    
    slint_window.set_size(slint::PhysicalSize::new(
        lib::ui_backend::ui_runner::DISPLAY_WIDTH as u32,
        lib::ui_backend::ui_runner::DISPLAY_HEIGHT as u32,
    ));

    let config_storage = MockFlashStorage::new(0x0000, 0x4000, None);
    let mut config_buffer = RingBuffer::new(0x0000, 0x4000, config_storage).expect("simulation config addresses must be page-aligned");

    let ui = start_ui(&mut config_buffer).await;
    ui.show().expect("Unable to show UI");

    let window_event_sender = WINDOW_EVENT_CHANNEL.sender();
    let window_event_receiver = WINDOW_EVENT_CHANNEL.receiver();

    let (sdl2_renderer, sdl2_window_event_dispatcher) = display::create_sdl2_renderer(
        window_event_sender,
    );

    let register_mouse_events_future = register_mouse_events(sdl2_window_event_dispatcher);
    let mut pixel_buffer = vec![slint::platform::software_renderer::Rgb565Pixel::default(); FRAME_PIXELS];
    let ui_task_future = lib::ui_backend::ui_runner::render_loop::<Sdl2Renderer>(
        slint_window, window_event_receiver, sdl2_renderer, &ui, &mut pixel_buffer
    );

    let ui_message_receiver = UI_ACTION_CHANNEL.receiver();
    let mut mock_rtc = MockRtc::new();
    let cli_sensors = CliSensors::new();

    let cli_pump_controller = crate::mocks::CliPumpController::new();
    let cli_sensor_controller = SensorController::new(cli_sensors.clone());
    let treatment_controller = TreatmentController::initialize(cli_pump_controller, cli_sensor_controller);
    let treatment_controller_mutex = embassy_sync::mutex::Mutex::new(treatment_controller);

    let process_ui_update_messages_future = process_ui_update_messages(
        ui_message_receiver,
        &mut mock_rtc,
        &treatment_controller_mutex,
        &mut config_buffer,
    );

    let clock_update_future = lib::tasks::update_clock_task(&ui, || chrono::Utc::now().timestamp_micros() as u64);

    let outlet_scheduler_future = lib::tasks::outlet_scheduler_task(
        &treatment_controller_mutex,
        || chrono::Utc::now().timestamp_micros() as u64,
    );

    let dosing_future = async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15 * 60));
        loop {
            interval.tick().await;
            lib::tasks::run_dosing_cycle(&treatment_controller_mutex).await;
        }
    };

    tokio::select! {
        _register_mouse_events_result = register_mouse_events_future => {
            println!("Mouse events handler finished");
        }
        _ui_result = ui_task_future => {
            println!("UI Finished");
        }
        _ui_messages_result = process_ui_update_messages_future => {
            println!("UI message processing finished");
        }
        _clock_result = clock_update_future => {
            println!("Clock update task finished");
        }
        _outlet_scheduler_result = outlet_scheduler_future => {
            println!("Outlet scheduler task finished");
        }
        _dosing_result = dosing_future => {
            println!("Dosing task finished");
        }
    }
    
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Ok(())
}

async fn initialize_simulation_state(ui: &lib::ui_types::MainWindow, config_buffer: &mut RingBuffer<DeviceConfig, EmptyMetadata, MockFlashStorage, MockFlashStorageError>) {
    use lib::peripherals::DosingPump;
    use chrono::Utc;

    let timestamp = Utc::now();

    lib::storage::update_device_config(config_buffer, timestamp, |device_config| {
        device_config.ph.enabled = true;
        device_config.ec.enabled = true;
        device_config.orp.enabled = true;
        device_config.temperature.enabled = true;

        // Pump 0: Unconfigured, enabled
        let pump0_state = device_config.pumps.get_dosing_pump_state_mut(DosingPump::DoseOne);
        pump0_state.enabled = true;
        pump0_state.name = None;
        pump0_state.treatment_solution.solution_type = lib::ui_types::TreatmentSolutionType::Unconfigured;
        pump0_state.treatment_solution.solution_strength = 0.0;

        // Pump 1: pH Down, enabled
        let pump1_state = device_config.pumps.get_dosing_pump_state_mut(DosingPump::DoseTwo);
        pump1_state.enabled = true;
        pump1_state.name = Some("pH Down".into());
        pump1_state.treatment_solution.solution_type = lib::ui_types::TreatmentSolutionType::PhDown;
        pump1_state.treatment_solution.solution_strength = 4.0;

        // Pump 2: Unconfigured, disabled
        let pump2_state = device_config.pumps.get_dosing_pump_state_mut(DosingPump::DoseThree);
        pump2_state.enabled = false;
        pump2_state.name = None;
        pump2_state.treatment_solution.solution_type = lib::ui_types::TreatmentSolutionType::Unconfigured;
        pump2_state.treatment_solution.solution_strength = 0.0;

        // Pump 3: pH Up, enabled
        let pump3_state = device_config.pumps.get_dosing_pump_state_mut(DosingPump::DoseFour);
        pump3_state.enabled = true;
        pump3_state.name = Some("pH Up".into());
        pump3_state.treatment_solution.solution_type = lib::ui_types::TreatmentSolutionType::PhUp;
        pump3_state.treatment_solution.solution_strength = 9.0;

        // Pump 4: ORP Treatment, enabled, error status
        let pump4_state = device_config.pumps.get_dosing_pump_state_mut(DosingPump::DoseFive);
        pump4_state.enabled = true;
        pump4_state.name = None;
        pump4_state.status = lib::ui_types::Status::Error;
        pump4_state.treatment_solution.solution_type = lib::ui_types::TreatmentSolutionType::OrpTreatment;
        pump4_state.treatment_solution.solution_strength = 400.0;

        // Pump 5: Nutrients, enabled
        let pump5_state = device_config.pumps.get_dosing_pump_state_mut(DosingPump::DoseSix);
        pump5_state.enabled = true;
        pump5_state.name = Some("Nutrients".into());
        pump5_state.treatment_solution.solution_type = lib::ui_types::TreatmentSolutionType::Nutrient;
        pump5_state.treatment_solution.solution_strength = 1.8;

        // Outlet 1: General Purpose (Fertigation) with schedule
        use lib::data::schedule::{OutletSchedule, ScheduledEvent, DaysOfWeek};
        use lib::ui_types::OutletMode;
        use chrono::{NaiveTime, Duration};

        let mut gp_schedule = OutletSchedule::new();
        gp_schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            Duration::minutes(15),
        ).with_days(DaysOfWeek::every_day()));
        gp_schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            Duration::minutes(30),
        ).with_days(DaysOfWeek::every_day()));
        gp_schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            Duration::minutes(20),
        ).with_days(DaysOfWeek::from_bools(false, true, true, true, true, true, false)));
        gp_schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(16, 30, 0).unwrap(),
            Duration::minutes(25),
        ).with_days(DaysOfWeek::from_bools(true, false, false, false, false, false, true)));
        gp_schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(20, 30, 0).unwrap(),
            Duration::hours(1),
        ).with_days(DaysOfWeek::from_bools(false, true, true, true, true, true, false)));
        gp_schedule.add_event(ScheduledEvent::new(
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            Duration::minutes(10),
        ).with_days(DaysOfWeek::every_day()));

        let outlet1 = device_config.outlets.get_outlet_state_mut(lib::ui_types::Outlet::One);
        outlet1.name = Some("Fertigation".into());
        outlet1.enabled = true;
        outlet1.mode = OutletMode::GeneralPurpose;
        outlet1.schedule = gp_schedule;

        // Outlet 2: Stir Pump (30 seconds)
        let outlet2 = device_config.outlets.get_outlet_state_mut(lib::ui_types::Outlet::Two);
        outlet2.name = Some("Stir Pump".into());
        outlet2.enabled = true;
        outlet2.mode = OutletMode::StirPump;
        outlet2.stir_seconds = Some(30);

        // Outlet 3: Auto-Fill Solenoid (60 second max)
        let outlet3 = device_config.outlets.get_outlet_state_mut(lib::ui_types::Outlet::Three);
        outlet3.name = Some("Auto-Fill".into());
        outlet3.enabled = true;
        outlet3.mode = OutletMode::Solenoid;
        outlet3.max_fill_seconds = Some(60);
    }).await;

    let mut device_config = lib::storage::get_device_config().await;
    device_config.populate_ui_from_backend(ui);

    seed_simulation_sensor_history();

    println!("Initialized simulation with realistic device config (all sensors enabled, various pump states)");
}

fn seed_simulation_sensor_history() {
    use lib::ui_types::SensorType;

    let now = chrono::Utc::now().timestamp();
    let interval_secs = 15 * 60;
    let num_points = 96;

    let ph_readings: Vec<(i64, f32)> = (0..num_points)
        .map(|i| {
            let t = now - (num_points - 1 - i) as i64 * interval_secs;
            let drift = (i as f32 * 0.065).sin() * 0.3;
            (t, 6.8 + drift)
        })
        .collect();

    let ec_readings: Vec<(i64, f32)> = (0..num_points)
        .map(|i| {
            let t = now - (num_points - 1 - i) as i64 * interval_secs;
            let drift = (i as f32 * 0.08).sin() * 150.0;
            (t, 1200.0 + drift)
        })
        .collect();

    let orp_readings: Vec<(i64, f32)> = (0..num_points)
        .map(|i| {
            let t = now - (num_points - 1 - i) as i64 * interval_secs;
            let drift = (i as f32 * 0.05).sin() * 40.0;
            (t, 650.0 + drift)
        })
        .collect();

    let temp_readings: Vec<(i64, f32)> = (0..num_points)
        .map(|i| {
            let t = now - (num_points - 1 - i) as i64 * interval_secs;
            let drift = (i as f32 * 0.04).sin() * 1.5;
            (t, 24.0 + drift)
        })
        .collect();

    lib::ui_backend::state::push_sensor_readings_bulk(SensorType::Ph, &ph_readings);
    lib::ui_backend::state::push_sensor_readings_bulk(SensorType::Conductivity, &ec_readings);
    lib::ui_backend::state::push_sensor_readings_bulk(SensorType::Orp, &orp_readings);
    lib::ui_backend::state::push_sensor_readings_bulk(SensorType::Temperature, &temp_readings);

    println!("Seeded sensor history with {} simulated readings per sensor (24h @ 15min intervals)", num_points);
}

async fn start_ui(config_buffer: &mut RingBuffer<DeviceConfig, EmptyMetadata, MockFlashStorage, MockFlashStorageError>) -> lib::ui_types::MainWindow {
    println!("Starting Slint UI...");

    let ui = lib::ui_types::MainWindow::new().unwrap();

    println!("UI initialized. Window should be visible now.");
    println!("Click on sensor status bars to test calibration workflows!");

    lib::ui_backend::register_ui_callbacks(&ui);

    let tab_init = ui.global::<lib::ui_types::TabInitState>();
    tab_init.set_status(true);
    tab_init.set_pumps(true);
    tab_init.set_outlets(true);
    tab_init.set_config(true);
    tab_init.set_logs(true);

    initialize_simulation_state(&ui, config_buffer).await;

    ui
}

async fn register_mouse_events(
    mut sdl2_window_event_dispatcher: display::Sdl2WindowEventDispatcher,
) {
    loop {
        sdl2_window_event_dispatcher.dispatch_events().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }
}


async fn process_ui_update_messages<'a>(
    ui_event_receiver: embassy_sync::channel::Receiver<'static, embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, UiMessage, { lib::ui_backend::actions::UI_ACTION_CHANNEL_SIZE }>,
    rtc: &mut MockRtc,
    treatment_controller: &lib::peripherals::TreatmentControllerMutex<'a, crate::mocks::CliSensors, crate::mocks::CliPumpController>,
    config_buffer: &mut RingBuffer<DeviceConfig, EmptyMetadata, MockFlashStorage, MockFlashStorageError>,
) {
    loop {
        println!("Waiting for UI update message...");
        let ui_event = ui_event_receiver.receive().await;
        println!("Received UI update message: {:?}", ui_event);

        let current_ticks = chrono::Utc::now().timestamp_micros() as u64;
        let current_timestamp = lib::state::get_system_time(current_ticks).await;
        let mut ctx = MessageContext {
            current_timestamp,
            current_ticks,
            rtc,
            treatment_controller,
            config_buffer,
        };
        lib::ui_backend::actions::dispatch(ui_event, &mut ctx).await;
        println!("Finished processing UI update message!");
    }
}

async fn process_cli_commands(ui: &MainWindow) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use std::io::{self, Write};

    println!("CLI commands available:");
    println!("   set-state ph-slope <value>     - Set pH calibration slope percentage");
    println!("   set-state ph-calibration-state <state> - Set pH calibration state (inactive, complete, etc.)");
    println!("   help                           - Show this help");
    println!("Type commands while the UI is running...\n");

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);

    loop {
        print!("CLI> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match reader.read_line(&mut input).await {
            Ok(_) => {
                let command = input.trim();
                if command.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = command.split_whitespace().collect();
                match parts.as_slice() {
                    ["help"] => {
                        println!("Available CLI commands:");
                        println!("   set-state ph-slope <value>     - Set pH calibration slope percentage (e.g., 100, 95, 110)");
                        println!("   set-state ph-calibration-state <state> - Set pH calibration state:");
                        println!("       inactive, getting-low, measuring-low, getting-mid, measuring-mid,");
                        println!("       getting-high, measuring-high, complete, cancelled");
                        println!("   help                           - Show this help");
                    }
                    ["set-state", "ph-slope", value] => {
                        match value.parse::<f32>() {
                            Ok(slope) => {
                                let workflow_state = ui.global::<lib::ui_types::WorkflowUiState>();
                                workflow_state.set_ph_calibration_slope(slope);
                                println!("Set pH slope to {:.1}%", slope);
                            }
                            Err(_) => {
                                println!("Invalid number: '{}'", value);
                            }
                        }
                    }
                    ["set-state", "ph-calibration-state", state] => {
                        let workflow_state = ui.global::<lib::ui_types::WorkflowUiState>();
                        let ph_state = match *state {
                            "inactive" => lib::ui_types::PhCalibrationWorkflowStep::Inactive,
                            "getting-low" => lib::ui_types::PhCalibrationWorkflowStep::GettingLowPhValue,
                            "measuring-low" => lib::ui_types::PhCalibrationWorkflowStep::MeasuringLowPh,
                            "getting-mid" => lib::ui_types::PhCalibrationWorkflowStep::GettingMidPhValue,
                            "measuring-mid" => lib::ui_types::PhCalibrationWorkflowStep::MeasuringMidPh,
                            "getting-high" => lib::ui_types::PhCalibrationWorkflowStep::GettingHighPhValue,
                            "measuring-high" => lib::ui_types::PhCalibrationWorkflowStep::MeasuringHighPh,
                            "complete" => lib::ui_types::PhCalibrationWorkflowStep::Complete,
                            "cancelled" => lib::ui_types::PhCalibrationWorkflowStep::Cancelled,
                            _ => {
                                println!("Invalid state: '{}'. Use help for valid states.", state);
                                continue;
                            }
                        };
                        workflow_state.set_ph_calibration_state(ph_state);
                        println!("Set pH calibration state to {:?}", ph_state);
                    }
                    _ => {
                        println!("Unknown command: '{}'. Type 'help' for available commands.", command);
                    }
                }
            }
            Err(e) => {
                println!("Error reading input: {}", e);
                break;
            }
        }
    }
}