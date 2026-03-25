use alloc::vec;

use embassy_executor::task;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Receiver, Sender},
};
use embassy_time::{Duration, Ticker, Timer};
use esp_hal::{
    gpio::{Input, Output},
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::Rate,
    timer::{timg::TimerGroup, OneShotTimer},
};
use log::info;
use slint::{
    platform::{software_renderer::Rgb565Pixel, WindowEvent},
    ComponentHandle,
};

use lib::{
    config::device_config::DeviceConfig,
    peripherals::rtc::Mcp7940,
    storage::{EmptyMetadata, RingBuffer},
    ui_backend::{
        actions::{MessageContext, UiMessage, UI_ACTION_CHANNEL, UI_ACTION_CHANNEL_SIZE},
        ui_runner::{render_loop, TouchInput, DISPLAY_WIDTH, FRAME_PIXELS},
    },
    ui_types::MainWindow,
};

use crate::hardware_peripherals::{
    EspStorage, EspTreatmentControllerMutex, HardwareDisplay, HardwareTouchInput, SharedI2cDevice,
};

#[allow(unsafe_code)]
fn print_stack_usage() {
    use core::arch::asm;
    let sp: u32;
    unsafe { asm!("mov {}, a1", out(reg) sp) };
    log::info!("Stack pointer: 0x{:08x}", sp);
}

#[task]
pub async fn fill_reservoir(
    mut float_pin: Input<'static>,
    treatment_controller: &'static EspTreatmentControllerMutex<'static>,
) {
    loop {
        float_pin.wait_for_rising_edge().await;
        log::info!("Float pin: {:?}", float_pin.level());
        float_pin.wait_for_falling_edge().await;
        log::info!("Float pin: {:?}", float_pin.level());
    }
    #[allow(unreachable_code)]
    lib::tasks::run_fill_cycle(&mut float_pin, treatment_controller).await;
}

#[task]
pub async fn read_touch_input(
    touch_i2c_device: SharedI2cDevice<'static>,
    touch_interrupt_input: Input<'static>,
    window_event_tx: Sender<'static, CriticalSectionRawMutex, WindowEvent, 75>,
    mut touch_reset_pin: Output<'static>,
) {
    info!("Touch input loop spawned");

    // The touch chip to stabilize before we can start reading
    touch_reset_pin.set_low();
    embassy_time::Timer::after(Duration::from_millis(10)).await;
    touch_reset_pin.set_high();
    embassy_time::Timer::after(Duration::from_millis(10)).await;

    let mut touch_input =
        HardwareTouchInput::new(touch_i2c_device, touch_interrupt_input, window_event_tx);

    loop {
        // We get notified for touches and moves, but not for releases,
        // hence the select and timer wait.
        embassy_futures::select::select(
            touch_input.wait_for_touch(),
            embassy_time::Timer::after(embassy_time::Duration::from_millis(50)),
        )
        .await;
        touch_input.process_touch_events().await;
    }
}

#[task]
pub async fn render_ui(
    window_event_rx: Receiver<'static, CriticalSectionRawMutex, WindowEvent, 75>,
    spi: esp_hal::peripherals::SPI2<'static>,
    sck_pin: Output<'static>,
    mosi_pin: Output<'static>,
    miso_pin: Input<'static>,
    screen_cs_pin: Output<'static>,
    dc_pin: Output<'static>,
    reset_pin: Output<'static>,
    dma_channel: esp_hal::peripherals::DMA_CH0<'static>,
    timer: TimerGroup<'static, esp_hal::peripherals::TIMG1<'static>>,
) {
    print_stack_usage();
    info!("Setting up screen SPI");

    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = esp_hal::dma_buffers!(96, 16384);
    let dma_rx_buf = esp_hal::dma::DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let dma_tx_buf = esp_hal::dma::DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    let shared_spi_peripheral_mode_0 = Spi::new(spi, {
        let config = SpiConfig::default()
            .with_frequency(Rate::from_mhz(80))
            .with_mode(Mode::_0);
        info!("SPI Peripheral frequency: {:?}", config.frequency());
        config
    })
    .unwrap()
    .with_sck(sck_pin)
    .with_mosi(mosi_pin)
    .with_miso(miso_pin)
    .with_dma(dma_channel)
    .with_buffers(dma_rx_buf, dma_tx_buf)
    .into_async();

    let mut delay = OneShotTimer::new(timer.timer1).into_async();
    let display = HardwareDisplay::new(
        shared_spi_peripheral_mode_0,
        screen_cs_pin,
        dc_pin,
        reset_pin,
        &mut delay,
    )
    .await;

    info!("Setting up Slint window");
    let slint_window = crate::esp_ui_backend::setup_ui_backend();
    let main_window = crate::MAIN_WINDOW.init(MainWindow::new().unwrap());

    print_stack_usage();

    log::info!("Registering callbacks");
    lib::ui_backend::register_ui_callbacks(&main_window);

    print_stack_usage();

    // The pixel buffer we use.  Currently it buffers all pixels in the display.
    // Nice to have the spare memory to support this.
    let mut pixel_buffer = vec![Rgb565Pixel::default(); FRAME_PIXELS];
    let pixel_buf = &mut *pixel_buffer;

    let tab_init = main_window.global::<lib::ui_types::TabInitState>();
    tab_init.set_status(true);
    slint_window.draw_if_needed(|renderer| {
        renderer.render(pixel_buf, DISPLAY_WIDTH.into());
    });
    tab_init.set_pumps(true);
    slint_window.draw_if_needed(|renderer| {
        renderer.render(pixel_buf, DISPLAY_WIDTH.into());
    });
    tab_init.set_outlets(true);
    slint_window.draw_if_needed(|renderer| {
        renderer.render(pixel_buf, DISPLAY_WIDTH.into());
    });
    tab_init.set_config(true);
    slint_window.draw_if_needed(|renderer| {
        renderer.render(pixel_buf, DISPLAY_WIDTH.into());
    });
    tab_init.set_logs(true);
    slint_window.draw_if_needed(|renderer| {
        renderer.render(pixel_buf, DISPLAY_WIDTH.into());
    });

    info!("Render loop spawned successfully");

    render_loop(
        slint_window,
        window_event_rx,
        display,
        main_window,
        pixel_buf,
    )
    .await;

    loop {
        Timer::after_secs(1).await;
    }
}

#[task]
pub async fn process_ui_update_messages(
    ui_message_rx: Receiver<'static, CriticalSectionRawMutex, UiMessage, UI_ACTION_CHANNEL_SIZE>,
    mut rtc: Mcp7940<SharedI2cDevice<'static>>,
    treatment_controller: &'static EspTreatmentControllerMutex<'static>,
    mut config_buffer: RingBuffer<
        DeviceConfig,
        EmptyMetadata,
        EspStorage<'static>,
        esp_storage::FlashStorageError,
    >,
) {
    info!("UI message processing loop spawned");
    loop {
        let ui_message = ui_message_rx.receive().await;
        log::info!("UI message received: {:?}", ui_message);
        let current_ticks = esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_micros() as u64;
        let current_timestamp = lib::state::get_system_time(current_ticks).await;
        let mut ctx = MessageContext {
            current_timestamp,
            current_ticks,
            rtc: &mut rtc,
            treatment_controller,
            config_buffer: &mut config_buffer,
        };
        lib::ui_backend::actions::dispatch(ui_message, &mut ctx).await;
    }
}

#[task]
pub async fn outlet_scheduler(treatment_controller: &'static EspTreatmentControllerMutex<'static>) {
    lib::tasks::outlet_scheduler_task(treatment_controller, || {
        esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_micros() as u64
    })
    .await;
}

#[task]
pub async fn log_drain(
    mut logger: lib::logging::RingBufferLogger<EspStorage<'static>, esp_storage::FlashStorageError>,
) {
    use lib::logging::Logger;
    loop {
        let request = lib::logging::LOG_CHANNEL.receive().await;
        let level = request.level;
        let category = request.category;
        let timestamp_secs = request.micros as i64 / 1_000_000;
        let error_message = if level == lib::logging::LogLevel::Error {
            Some(request.message.clone())
        } else {
            None
        };
        let timestamp = lib::state::get_system_time(request.micros).await;
        let entry = match request.context {
            Some(ctx) => lib::logging::LogEntry::with_context(request.message, timestamp, ctx),
            None => lib::logging::LogEntry::new(request.message, timestamp),
        };
        if let Err(e) = logger.log(entry, level, category, request.error_type) {
            log::error!("Failed to write log to flash: {:?}", e);
        }
        if let Some(msg) = error_message {
            lib::ui_backend::state::push_recent_error(lib::ui_backend::state::RecentError {
                message: msg,
                category,
                timestamp_secs,
            });
        }
    }
}

const DOSING_INTERVAL_SECS: u64 = 10 * 60;

#[task]
pub async fn read_and_dose(treatment_controller: &'static EspTreatmentControllerMutex<'static>) {
    info!("Dosing task started");
    let mut ticker = Ticker::every(Duration::from_secs(DOSING_INTERVAL_SECS));
    loop {
        ticker.next().await;
        lib::tasks::run_dosing_cycle(treatment_controller).await;
    }
}

#[task]
pub async fn monitor_pump_current(
    treatment_controller: &'static EspTreatmentControllerMutex<'static>,
) {
    info!("Current monitor task started");
    let mut ticker = Ticker::every(Duration::from_secs(5));
    loop {
        ticker.next().await;
        lib::tasks::check_pump_currents(treatment_controller).await;
    }
}
