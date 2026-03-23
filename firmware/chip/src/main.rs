#![deny(unsafe_code, dead_code)]


#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(asm_experimental_arch)]


extern crate alloc;

use core::cell::RefCell;

use lib::{
    config::device_config::DeviceConfig,
    peripherals::{
        HardwarePumpController,
        SensorController,
        rtc::Mcp7940,
    },
    storage::{
        EmptyMetadata,
        RingBuffer,
    },
    ui_backend::ui_runner::WINDOW_EVENT_CHANNEL,
    ui_types::MainWindow,
};

mod hardware_peripherals;
use hardware_peripherals::{EspTreatmentController, EspTreatmentControllerMutex, HardwareTouchInput, Sensors_1_0_0};

mod tasks; 

mod esp_ui_backend;

use embassy_executor::Spawner;

use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;

use embassy_sync::{
    blocking_mutex::{
        raw::NoopRawMutex,
        Mutex as BlockingMutex
    },
    mutex::Mutex,
};
use embassy_time::Timer;
use log::info;
use esp_backtrace as _;
use esp_hal::{
    Blocking,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    psram,
    rmt::{Rmt, RxChannelConfig, RxChannelCreator},
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_storage::FlashStorage;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static ESP_PERIPHERALS_MUTEX: StaticCell<EspTreatmentControllerMutex> = StaticCell::new();

static LSTIMER0: StaticCell<esp_hal::ledc::timer::Timer<'static, LowSpeed>> = StaticCell::new();

static MCP23017: StaticCell<port_expander::dev::mcp23x17::Mcp23x17<RefCell<port_expander::dev::mcp23x17::Driver<port_expander::dev::mcp23x17::Mcp23017Bus<hardware_peripherals::SharedI2cDevice<'static>>>>>> = StaticCell::new();

static MAIN_WINDOW: StaticCell<MainWindow> = StaticCell::new();

static SHARED_I2C_BUS: StaticCell<
    BlockingMutex<
        NoopRawMutex,
        RefCell<I2c<'static, Blocking>>
    >
> = StaticCell::new();

static FLASH_STORAGE_INTERNALS: StaticCell<
    BlockingMutex<
        NoopRawMutex,
        hardware_peripherals::EspStorageInternals<'static>,
    >
> = StaticCell::new();

#[allow(dead_code, unsafe_code)]
fn print_stack_usage() {
    use core::arch::asm;
    let sp: u32;
    unsafe { asm!("mov {}, a1", out(reg) sp) };
    log::info!("Stack pointer: 0x{:08x}", sp);
}

#[esp_rtos::main()]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    info!("Booting...");

    let psram_config = psram::PsramConfig {
        ram_frequency: psram::SpiRamFreq::Freq80m,
        ..Default::default()
    };
    
    let peripherals = esp_hal::init(esp_hal::Config::default().with_psram(psram_config));
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);
    
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    use esp_hal::interrupt::software::SoftwareInterruptControl;
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

    lib::state::init_clock(|| esp_hal::time::Instant::now().duration_since_epoch().as_micros() as u64)
        .expect("Clock already initialized");

    let freq = Rate::from_mhz(80);

    info!("Initialized");

    ///////////////  Break out hardware pins  ///////////////
    //
    // Pin definitions -- we don't do anything with the pins here.
    // This is just to have a centralized spot where all of the pin mappings
    // live, in case things get moved around on the board between iterations.
    // It's also nice to be passing around named pins rather than just numbers.

    let i2c_data_pin = peripherals.GPIO4;
    let i2c_clk_pin = peripherals.GPIO5;

    let spi_0_sck_pin = peripherals.GPIO7;
    let spi_0_copi_pin = peripherals.GPIO15;
    let spi_0_cipo_pin = peripherals.GPIO6;

    let relay_pin = Output::new(peripherals.GPIO9, Level::High, OutputConfig::default());

    let screen_cs_pin = peripherals.GPIO21;
    let screen_reset_pin = peripherals.GPIO48;
    let screen_dc_pin = peripherals.GPIO47;

    let touch_interrupt_pin = peripherals.GPIO41;
    let touch_reset_pin = peripherals.GPIO42; 

    let temp_ec_sel_pin = peripherals.GPIO46;
    let temp_ec_sqw_pin = peripherals.GPIO3;

    let float_sensor_pin = peripherals.GPIO1;

    let adc_clock_pin = peripherals.GPIO14;
    let adc_data_pin = peripherals.GPIO13;
    let adc_diag_pin = peripherals.GPIO12;

    // Currently no software support for these interfaces, but it's nice
    // to have the pin definitions already built in for future reference.
    let _rs435_d_data_pin = peripherals.GPIO16;
    let _rs435_r_data_pin = peripherals.GPIO17;
    let _rs435_drive_pin =  peripherals.GPIO10;
    let _can_rx_pin = peripherals.GPIO18;
    let _can_tx_pin = peripherals.GPIO8;
    let _can_stb_pin = peripherals.GPIO45;
    let _external_i2c_sda_pin = peripherals.GPIO38;
    let _external_i2c_scl_pin = peripherals.GPIO39;
    let _free_pin = peripherals.GPIO40;
    let _external_digital_pin = peripherals.GPIO2;


    ///////////////  Instantiate I2C  ///////////////

    let shared_i2c_peripheral = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(400)),
    )
        .unwrap()
        .with_sda(i2c_data_pin)
        .with_scl(i2c_clk_pin);

    let shared_i2c_bus = &*SHARED_I2C_BUS.init(BlockingMutex::new(RefCell::new(shared_i2c_peripheral)));

    ///////////////  Instantiate MCP23017  ///////////////

    let mcp_i2c = I2cDevice::new(shared_i2c_bus);
    let mcp23017 = MCP23017.init(port_expander::Mcp23x17::new_mcp23017(mcp_i2c, false, false, false));
    let mcp_pins = mcp23017.split();

    let mcp_dose_0_pin = mcp_pins.gpb7.into_output().unwrap();
    let mcp_dose_1_pin = mcp_pins.gpa0.into_output().unwrap();
    let mcp_dose_2_pin = mcp_pins.gpb6.into_output().unwrap();
    let mcp_dose_3_pin = mcp_pins.gpa1.into_output().unwrap();
    let mcp_dose_4_pin = mcp_pins.gpb5.into_output().unwrap();
    let mcp_dose_5_pin = mcp_pins.gpa2.into_output().unwrap();
    let mcp_outlet_0_pin = mcp_pins.gpa6.into_output().unwrap();
    let mcp_outlet_1_pin = mcp_pins.gpa5.into_output().unwrap();
    let mcp_outlet_2_pin = mcp_pins.gpb0.into_output().unwrap();
    let mcp_outlet_3_pin = mcp_pins.gpa7.into_output().unwrap();
    let mcp_adc_sel_0_pin = mcp_pins.gpb1.into_output().unwrap();
    let mcp_adc_sel_1_pin = mcp_pins.gpb2.into_output().unwrap();
    
    let mut mcp_screen_led_pin = mcp_pins.gpb4.into_output().unwrap();

    let mcp_current_sense_mux_a_pin = mcp_pins.gpa3.into_output().unwrap();
    let mcp_current_sense_mux_b_pin = mcp_pins.gpa4.into_output().unwrap();

    mcp_screen_led_pin.set_high().unwrap();

    ///////////// Instantiate RTC //////////////

    let mut rtc = Mcp7940::new(I2cDevice::new(shared_i2c_bus)).unwrap();
    info!("RTC instantiated");

    match rtc.get_datetime() {
        Ok(boot_time) => {
            let boot_micros = esp_hal::time::Instant::now().duration_since_epoch().as_micros();
            let time_info = lib::peripherals::rtc::SystemTimeInfo::new(boot_time, boot_micros as i64);
            lib::state::set_system_time_info(time_info).await;
            info!("System time initialized from RTC: {}", boot_time);
        },
        Err(e) => {
            log::error!("Failed to read RTC on boot: {}", e);
        },
    }

    ///////////// Spawn UI on CPU1 //////////////
    
    // Slint needs more stack space to set up than we can provide on the actual stack.
    // So the solution is to just leak 
    #[allow(unsafe_code)]
    let second_core_stack: &'static mut esp_hal::system::Stack<524288> = {
        let b = unsafe {
            alloc::boxed::Box::<esp_hal::system::Stack<524288>>::new_zeroed().assume_init()
        };
        alloc::boxed::Box::leak(b)
    };
    
    use esp_rtos::embassy::Executor;

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        software_interrupt.software_interrupt1,
        second_core_stack,
        move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());

            executor.run(|spawner| {
                spawner.spawn( crate::tasks::render_ui(
                    WINDOW_EVENT_CHANNEL.receiver(),
                    peripherals.SPI2,
                    Output::new(spi_0_sck_pin, Level::Low, OutputConfig::default()),
                    Output::new(spi_0_copi_pin, Level::Low, OutputConfig::default()),
                    Input::new(spi_0_cipo_pin, InputConfig::default()),
                    Output::new(screen_cs_pin, Level::High, OutputConfig::default()),
                    Output::new(screen_dc_pin, Level::Low, OutputConfig::default()),
                    Output::new(screen_reset_pin, Level::High, OutputConfig::default()),
                    peripherals.DMA_CH0,
                    TimerGroup::new(peripherals.TIMG1),
                )).ok();
            });
        },
    );

    ///////////////  Instantiate Flash and Ring Buffers  ///////////////  

    // esp_rtos::start_second_core takes our CPU_CTRL as input, but only needs it in the
    // context of core startup.  By the time this function steals the CPU_CTRL, start_second_core
    // is finished with it, so stealing it here doesn't cause problems.
    #[allow(unsafe_code)]
    let stolen_cpu_control = unsafe {
        esp_hal::system::CpuControl::new(esp_hal::peripherals::CPU_CTRL::steal())
    };

    let flash_storage_internals = FLASH_STORAGE_INTERNALS.init(BlockingMutex::new(
        hardware_peripherals::EspStorageInternals::new(
            FlashStorage::new(peripherals.FLASH),
            stolen_cpu_control,
        ))
    );

    let mut config_ring_buffer = RingBuffer::<DeviceConfig, EmptyMetadata, hardware_peripherals::EspStorage, esp_storage::FlashStorageError>::new(
        lib::storage::CONFIGS_START_ADDRESS,
        lib::storage::CONFIGS_END_ADDRESS,
        hardware_peripherals::EspStorage::new(flash_storage_internals),
    ).expect("config partition addresses must be page-aligned");
    info!("Ring buffer instantiated");

    let log_ring_buffer = lib::logging::LogRingBuffer::<hardware_peripherals::EspStorage, esp_storage::FlashStorageError>::new(
        lib::storage::LOGS_START_ADDRESS,
        lib::storage::LOGS_END_ADDRESS,
        hardware_peripherals::EspStorage::new(flash_storage_internals),
    ).expect("log partition addresses must be page-aligned");
    let log_logger = lib::logging::RingBufferLogger::new(log_ring_buffer);
    info!("Log ring buffer instantiated");

    match config_ring_buffer.read_latest_record() {
        Ok(None) => {},
        Ok(Some((config, _))) => {
            log::info!("Read config from flash storage: {:?}", config);
            lib::storage::set_device_config(config).await;
        },
        Err(_) => {
            panic!("Failed to read latest config from flash storage")
        }
    };

    ///////////////  Instantiate EspTreatmentController  ///////////////

    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0.configure(timer::config::Config {
        duty: timer::config::Duty::Duty2Bit,
        clock_source: timer::LSClockSource::APBClk,
        frequency: Rate::from_hz(80_000_000 / 8),
    }).unwrap();

    let lstimer0_static = LSTIMER0.init(lstimer0);

    let mut adc_ledc_channel = ledc.channel::<LowSpeed>(channel::Number::Channel0, adc_clock_pin);
    adc_ledc_channel.configure(channel::config::Config {
        timer: lstimer0_static,
        duty_pct: 50,
        drive_mode: esp_hal::gpio::DriveMode::PushPull,
    }).unwrap();

    let rmt = Rmt::new(peripherals.RMT, freq).unwrap().into_async();

    let rmt_channel = rmt
        .channel4
        .configure_rx(
            adc_data_pin,
            RxChannelConfig::default()
                .with_clk_divider(1)
                .with_carrier_modulation(false)
                .with_filter_threshold(0)
                // During normal functionality, the AMC3336 can only emit
                // a max of 127 pulses of the same value in a row.
                .with_idle_threshold(130)
        )
        .unwrap();

    let sensors_raw = Sensors_1_0_0::init(
        rmt_channel,
        mcp_adc_sel_0_pin,
        mcp_adc_sel_1_pin,
        adc_ledc_channel,
        Output::new(temp_ec_sel_pin, Level::Low, OutputConfig::default()),
        Output::new(temp_ec_sqw_pin, Level::Low, OutputConfig::default()),
        Input::new(adc_diag_pin, InputConfig::default().with_pull(Pull::Up)),
    );
    info!("Sensors initialized");

    let current_sense_adc_i2c = I2cDevice::new(shared_i2c_bus);

    let pump_controller = HardwarePumpController::new(
        [
            mcp_dose_0_pin,
            mcp_dose_1_pin,
            mcp_dose_2_pin,
            mcp_dose_3_pin,
            mcp_dose_4_pin,
            mcp_dose_5_pin,
        ],
        [
            mcp_outlet_0_pin,
            mcp_outlet_1_pin,
            mcp_outlet_2_pin,
            mcp_outlet_3_pin,
        ],
        current_sense_adc_i2c,
        mcp_current_sense_mux_a_pin,
        mcp_current_sense_mux_b_pin,
        relay_pin,
    ).await.unwrap();
    info!("Pump controller initialized");

    let sensor_controller = SensorController::new(sensors_raw);
    info!("Sensor controller initialized");

    let tc = EspTreatmentController::initialize(
        pump_controller,
        sensor_controller,
    );
    info!("EspTreatmentController initialized");

    let esp_peripherals_mutex = ESP_PERIPHERALS_MUTEX.init(Mutex::new(tc));
    info!("Peripherals instantiated");

    ///////////////  Spawn Tasks  ///////////////

    let hardware_touch_i2c = I2cDevice::new(shared_i2c_bus);

    spawner.spawn(tasks::log_drain(log_logger)).unwrap();
    spawner.spawn(tasks::read_touch_input(
        hardware_touch_i2c,
        Input::new(touch_interrupt_pin, InputConfig::default().with_pull(Pull::Up)),
        WINDOW_EVENT_CHANNEL.sender(),
        Output::new(touch_reset_pin, Level::Low, OutputConfig::default()),
    )).unwrap();
    spawner.spawn(tasks::process_ui_update_messages(
        lib::ui_backend::actions::UI_ACTION_CHANNEL.receiver(),
        rtc,
        esp_peripherals_mutex,
        config_ring_buffer,
    )).unwrap();
    spawner.spawn(tasks::outlet_scheduler(esp_peripherals_mutex)).unwrap();
    spawner.spawn(tasks::read_and_dose(esp_peripherals_mutex)).unwrap();
    spawner.spawn(tasks::monitor_pump_current(esp_peripherals_mutex)).unwrap();
    spawner.spawn(tasks::fill_reservoir(
        Input::new(float_sensor_pin, InputConfig::default().with_pull(Pull::Up)),
        esp_peripherals_mutex,
    )).unwrap();

    loop {
        let _stats: esp_alloc::HeapStats = esp_alloc::HEAP.stats();
        Timer::after_secs(10).await;
        log::info!("Heap stats: {:?}", _stats);
    }
}
