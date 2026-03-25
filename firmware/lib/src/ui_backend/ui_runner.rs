cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::rc::Rc;
    } else {
        use alloc::rc::Rc;
    }
}
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver},
};
use embassy_time::{Duration, Ticker};

use slint::platform::{
    software_renderer::{MinimalSoftwareWindow, PhysicalRegion, Rgb565Pixel},
    WindowEvent,
};

pub const WINDOW_EVENT_CHANNEL_SIZE: usize = 75;
pub static WINDOW_EVENT_CHANNEL: Channel<
    CriticalSectionRawMutex,
    WindowEvent,
    WINDOW_EVENT_CHANNEL_SIZE,
> = Channel::new();

pub const DISPLAY_WIDTH: u16 = 320;
pub const DISPLAY_HEIGHT: u16 = 480;
pub const FRAME_PIXELS: usize = DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize;
pub const FPS_CAP: u64 = 60;

#[allow(async_fn_in_trait)]
pub trait DisplayPixels {
    async fn draw_pixels(&mut self, buf: &mut [Rgb565Pixel], redraw_region: PhysicalRegion);
}

#[allow(async_fn_in_trait)]
pub trait TouchInput {
    async fn process_touch_events(&mut self) -> ();
}

pub async fn render_loop<S: DisplayPixels>(
    window: Rc<MinimalSoftwareWindow>,
    window_event_rx: Receiver<'static, CriticalSectionRawMutex, WindowEvent, 75>,
    mut screen: S,
    ui: &crate::ui_types::MainWindow,
    pixel_buf: &mut [Rgb565Pixel],
) {
    let mut last_pumps = crate::peripherals::DosingPumpStateList::default();
    let mut last_outlets = crate::peripherals::OutletStateList::default();

    let fps_micros = 1_000_000 / FPS_CAP;
    let mut fps_limit_ticker = Ticker::every(Duration::from_micros(fps_micros));

    loop {
        crate::ui_backend::sync_runtime_state_to_ui(ui).await;
        crate::ui_backend::sync_device_config_to_ui(ui, &mut last_pumps, &mut last_outlets).await;

        slint::platform::update_timers_and_animations();

        // We do touch management separately, since it functions
        // differently for the simulation backend and the esp32 backend.
        'event: loop {
            match window_event_rx.try_receive() {
                Ok(e) => {
                    log::debug!("Window event received: {:?}", e);
                    window.dispatch_event(e);
                }
                Err(_) => break 'event,
            }
        }

        let redraw_start = embassy_time::Instant::now();
        let mut redraw_region: PhysicalRegion = PhysicalRegion::default();
        let display_needs_flush = window.draw_if_needed(|renderer| {
            redraw_region = renderer.render(pixel_buf, DISPLAY_WIDTH.into());
        });
        if display_needs_flush {
            log::info!(
                "draw_if_needed took {:?} micros",
                redraw_start.elapsed().as_micros()
            );
        }

        let flush_start = embassy_time::Instant::now();
        if display_needs_flush {
            screen.draw_pixels(pixel_buf, redraw_region).await;
            log::info!(
                "draw_pixels took {:?} micros",
                flush_start.elapsed().as_micros()
            );
        }
        if flush_start.elapsed() < Duration::from_micros(fps_micros) {
            fps_limit_ticker.next().await;
        }
    }
}
