
use core::sync::atomic::Ordering;

use embedded_hal::i2c::I2c;

use embedded_hal_bus::spi::{
    ExclusiveDevice,
    NoDelay,
};

use esp_hal::{
    Async,
    gpio::Output,
    spi::master::SpiDmaBus,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Sender,
};
use embedded_hal_async::delay::DelayNs;

use ft6336u_driver::{FT6336U, TouchPoint, TouchStatus};

use lcd_async::{
    Builder,
    Display,
    interface::SpiInterface,
    models::ST7796,
    options::{ColorOrder, Orientation},
};
use slint::{
    PhysicalPosition,
    platform::{
        PointerEventButton,
        WindowEvent,
        software_renderer::{
            PhysicalRegion,
            Rgb565Pixel,
        },
    }
};

use lib::ui_backend::ui_runner::{
    DISPLAY_HEIGHT,
    DISPLAY_WIDTH,
    DisplayPixels,
    TouchInput,
};

use super::type_aliases::{
    SharedI2cDevice,
};

use crate::hardware_peripherals::storage::{ DMA_FLASH_STATE, DMA_FLASH_STATE_IDLE, DMA_FLASH_STATE_DMA_ACTIVE };

pub struct HardwareDisplay<'a> {
    display: Display<SpiInterface<ExclusiveDevice<SpiDmaBus<'a, Async>, Output<'a>, NoDelay>, Output<'a>>, ST7796, Output<'a>>,
}

impl<'a> HardwareDisplay<'a> {
    pub async fn new<Delay: DelayNs>(
        spi_bus: SpiDmaBus<'a, Async>,
        cs_pin: Output<'a>,
        dc_pin: Output<'a>,
        reset_pin: Output<'a>,
        delay: &mut Delay
    ) -> Self {
        let spi_device = ExclusiveDevice::new_no_delay(spi_bus, cs_pin).unwrap();
        let display_interface = SpiInterface::new(spi_device, dc_pin);
        let mut display = Builder::new(ST7796, display_interface)
            .reset_pin(reset_pin)
            .color_order(ColorOrder::Bgr)
            .display_size(DISPLAY_WIDTH, DISPLAY_HEIGHT)
            .init(delay).await.unwrap();
        display.set_orientation(Orientation::new().flip_horizontal()).await.unwrap();
        Self {
            display
        }
    }

    fn byte_swap_pixels(pixels: &mut [u8], update_region: PhysicalRegion) {
        for (origin, size) in update_region.iter() {
            for row in origin.y..origin.y + size.height as i32 {
                let row_start = (row * DISPLAY_WIDTH as i32 + origin.x) as usize * 2;
                let row_end = row_start + size.width as usize * 2;
                for chunk in pixels[row_start..row_end].chunks_exact_mut(2) {
                    chunk.swap(0, 1);
                }
            }
        }
    }
}

// TODO: Use multiple DMA transfers to send the buffer in chunks
// This should free up a good amount of CPU time, which
// would let us push a higher framerate or just use it for other stuff if needed.
impl<'a> DisplayPixels for HardwareDisplay<'a> {
    async fn draw_pixels(&mut self, buf: &mut [Rgb565Pixel], update_region: PhysicalRegion) {
        let cast_pixels: &mut [u8] = bytemuck::cast_slice_mut(buf);
        HardwareDisplay::byte_swap_pixels(cast_pixels, update_region);
        let show_time = esp_hal::time::Instant::now();
        loop {
            match DMA_FLASH_STATE.compare_exchange(
                DMA_FLASH_STATE_IDLE,
                DMA_FLASH_STATE_DMA_ACTIVE,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(_) => {
                    embassy_time::Timer::after(embassy_time::Duration::from_micros(1)).await;
                }
            }
        }

        let result = self.display.show_raw_data(
            0,
            0,
            DISPLAY_WIDTH,
            DISPLAY_HEIGHT,
            cast_pixels
        ).await;
        DMA_FLASH_STATE.store(DMA_FLASH_STATE_IDLE, Ordering::Release);
        if let Err(e) = result {
            lib::logging::flash_log_error(
                &lib::logging::LoggableError::Hardware(
                    alloc::format!("Display SPI transfer failed: {:?}", e)
                )
            );
            return;
        }
        log::info!("Showing pixels took {:?}", show_time.elapsed());
    }
}

pub struct HardwareTouchInput<'a> {
    touch_screen: FT6336U<SharedI2cDevice<'a>>,
    touch_interrupt: esp_hal::gpio::Input<'a>,
    window_event_tx: Sender<'static, CriticalSectionRawMutex, WindowEvent, 75>,
    last_touch_point: TouchPoint,
}

impl<'a> HardwareTouchInput<'a> {
    pub fn new(
        mut i2c_device: SharedI2cDevice<'a>,
        touch_interrupt: esp_hal::gpio::Input<'a>,
        window_event_tx: Sender<'static, CriticalSectionRawMutex, WindowEvent, 75>
    ) -> Self {
        i2c_device.write(ft6336u_driver::I2C_ADDR, &[ft6336u_driver::ADDR_ACTIVE_MODE_RATE as u8, 0x24 as u8]).unwrap();
        Self {
            touch_screen: FT6336U::new(i2c_device), 
            touch_interrupt,
            window_event_tx,
            last_touch_point: TouchPoint {
                status: TouchStatus::Release,
                x: 0,
                y: 0,
            },
        }
    }

    fn find_closest_touch_point(&mut self, touch_point_1: TouchPoint, touch_point_2: TouchPoint) -> TouchPoint {
        let touch_point_1_x: i32 = touch_point_1.x.into();
        let touch_point_1_y: i32 = touch_point_1.y.into();
        let touch_point_2_x: i32 = touch_point_2.x.into();
        let touch_point_2_y: i32 = touch_point_2.y.into();

        let last_touch_x: i32 = self.last_touch_point.x.into();
        let last_touch_y: i32 = self.last_touch_point.y.into();

        match (
            (last_touch_x - touch_point_1_x) * (last_touch_x - touch_point_1_x) +
            (last_touch_y - touch_point_1_y) * (last_touch_y - touch_point_1_y)
        ) < (
            (last_touch_x - touch_point_2_x) * (last_touch_x - touch_point_2_x) +
            (last_touch_y - touch_point_2_y) * (last_touch_y - touch_point_2_y)
        ){
            true => touch_point_1,
            false => touch_point_2,
        }
    }

    pub async fn process_one_point(&mut self, point: TouchPoint) -> () {
        match point.status {
            TouchStatus::Touch => {
                let position = PhysicalPosition::new(point.x.into(), point.y.into());
                self.window_event_tx.send(WindowEvent::PointerPressed{
                    position: position.to_logical(1.0),
                    button: PointerEventButton::Left
                }).await;
                self.last_touch_point = point;
            },
            TouchStatus::Stream => {
                let position = PhysicalPosition::new(point.x.into(), point.y.into());
                self.window_event_tx.send(WindowEvent::PointerMoved{
                    position: position.to_logical(1.0),
                }).await;
                self.last_touch_point = point;
            },
            TouchStatus::Release => {
                match self.last_touch_point.status {
                    TouchStatus::Release => {},
                    _ => {
                        let logical_position = PhysicalPosition::new(point.x.into(), point.y.into()); 
                        self.window_event_tx.send(WindowEvent::PointerReleased{
                            position: logical_position.to_logical(1.0),
                            button: PointerEventButton::Left
                        }).await;
                        self.last_touch_point = point;
                    }
                }
            }
        };
    }

    pub async fn wait_for_touch(&mut self) -> () {
        self.touch_interrupt.wait_for_falling_edge().await;
    }
}

impl TouchInput for HardwareTouchInput<'_> {
    async fn process_touch_events(&mut self) -> () {
        let data = match self.touch_screen.scan() {
            Ok(data) => data,
            Err(e) => {
                log::warn!("Touch scan failed: {:?}", e);
                return;
            }
        };
        match data.touch_count {
            0 => {
                self.process_one_point(data.points[0]).await;
            },
            1 => {
                self.process_one_point(data.points[0]).await;
            },
            _ => {
                let closest_point = self.find_closest_touch_point(data.points[0], data.points[1]);
                self.process_one_point(closest_point).await;
            },
        };
    }
}
