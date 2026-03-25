use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Sender};

use lib::ui_backend::ui_runner::{
    DisplayPixels, DISPLAY_HEIGHT, DISPLAY_WIDTH, WINDOW_EVENT_CHANNEL_SIZE,
};

fn get_scale_factor() -> f32 {
    std::env::var("GREGOR_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0)
}

use slint::{
    platform::{
        software_renderer::{MinimalSoftwareWindow, PhysicalRegion, Rgb565Pixel},
        Platform, PointerEventButton, WindowAdapter, WindowEvent,
    },
    PlatformError,
};

use sdl2::{
    event::Event, keyboard::Keycode, mouse::MouseButton, pixels::PixelFormatEnum, rect::Rect,
    render::Canvas, video::Window, EventPump,
};

use std::rc::Rc;

pub fn create_sdl2_renderer(
    window_event_sender: Sender<
        'static,
        CriticalSectionRawMutex,
        WindowEvent,
        WINDOW_EVENT_CHANNEL_SIZE,
    >,
) -> (Sdl2Renderer, Sdl2WindowEventDispatcher) {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem
        .window("Simulation", DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _)
        .position_centered()
        .resizable()
        .opengl()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();

    canvas.clear();

    let event_pump = sdl_context.event_pump().unwrap();

    (
        Sdl2Renderer::new(canvas),
        Sdl2WindowEventDispatcher::new(event_pump, window_event_sender),
    )
}

pub struct Sdl2Renderer {
    canvas: Canvas<Window>,
}

impl Sdl2Renderer {
    fn new(canvas: Canvas<Window>) -> Self {
        Self { canvas }
    }
}

impl DisplayPixels for Sdl2Renderer {
    async fn draw_pixels(&mut self, pixels: &mut [Rgb565Pixel], _redraw_region: PhysicalRegion) {
        let texture_creator = self.canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGB565,
                DISPLAY_WIDTH as _,
                DISPLAY_HEIGHT as _,
            )
            .unwrap();

        texture
            .with_lock(None, |buffer: &mut [u8], _pitch: usize| {
                let pixels_ptr = pixels.as_ptr() as *const u8;
                let pixels_slice =
                    unsafe { std::slice::from_raw_parts(pixels_ptr, pixels.len() * 2) };
                buffer.copy_from_slice(pixels_slice);
            })
            .unwrap();

        self.canvas
            .copy_ex(
                &texture,
                None,
                Some(Rect::new(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _)),
                0.0,
                None,
                false,
                false,
            )
            .unwrap();
        self.canvas.present();
        tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
    }
}

pub struct Sdl2WindowEventDispatcher {
    event_pump: EventPump,
    window_event_sender:
        Sender<'static, CriticalSectionRawMutex, WindowEvent, WINDOW_EVENT_CHANNEL_SIZE>,
}

impl Sdl2WindowEventDispatcher {
    pub fn new(
        event_pump: EventPump,
        window_event_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            WindowEvent,
            WINDOW_EVENT_CHANNEL_SIZE,
        >,
    ) -> Self {
        Self {
            event_pump,
            window_event_sender,
        }
    }

    pub async fn dispatch_events(&mut self) {
        for event in self.event_pump.poll_iter() {
            match event {
                Event::MouseButtonDown {
                    timestamp: _timestamp,
                    window_id: _window_id,
                    which: _which,
                    mouse_btn,
                    clicks: _clicks,
                    x,
                    y,
                } => {
                    if mouse_btn == MouseButton::Left {
                        println!("Event detected!: {:?}", event);
                        let button = PointerEventButton::Left;
                        let position = slint::PhysicalPosition::new(x, y).to_logical(1.0);
                        let event = WindowEvent::PointerPressed { position, button };
                        self.window_event_sender.send(event).await;
                    }
                }
                Event::MouseButtonUp {
                    timestamp: _timestamp,
                    window_id: _window_id,
                    which: _which,
                    mouse_btn,
                    clicks: _clicks,
                    x,
                    y,
                } => {
                    if mouse_btn == MouseButton::Left {
                        println!("Event detected!: {:?}", event);
                        let button = PointerEventButton::Left;
                        let position = slint::PhysicalPosition::new(x, y).to_logical(1.0);
                        let event = WindowEvent::PointerReleased { position, button };
                        self.window_event_sender.send(event).await;
                    }
                }
                Event::MouseMotion {
                    timestamp: _timestamp,
                    window_id: _window_id,
                    which: _which,
                    mousestate,
                    x,
                    y,
                    xrel: _xrel,
                    yrel: _yrel,
                } => {
                    if mousestate.is_mouse_button_pressed(MouseButton::Left) {
                        println!("Event detected!: {:?}", event);
                        let position = slint::PhysicalPosition::new(x, y).to_logical(1.0);
                        let event = WindowEvent::PointerMoved { position };
                        self.window_event_sender.send(event).await;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    println!("Escape key pressed - exiting simulation...");
                    std::process::exit(0);
                }
                Event::Quit { .. } => {
                    println!("Window close requested - exiting simulation...");
                    std::process::exit(0);
                }
                _ => {}
            };
        }
    }
}

pub struct SimulationBackend {
    window: Rc<MinimalSoftwareWindow>,
    start_time: std::time::Instant,
}

impl SimulationBackend {
    pub fn new(window: Rc<MinimalSoftwareWindow>) -> Self {
        Self {
            window,
            start_time: std::time::Instant::now(),
        }
    }
}

impl Platform for SimulationBackend {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = self.window.clone();
        println!("create_window_adapter called");
        Ok(window)
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.start_time.elapsed()
    }
}
