use alloc::{boxed::Box, rc::Rc};
use lib::ui_backend::ui_runner::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use slint::platform::software_renderer::MinimalSoftwareWindow;

pub struct EspBackend {
    window: Rc<MinimalSoftwareWindow>,
}

impl slint::platform::Platform for EspBackend {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_micros(
            esp_hal::time::Instant::now()
                .duration_since_epoch()
                .as_micros(),
        )
    }
}

pub fn setup_ui_backend() -> Rc<MinimalSoftwareWindow> {
    let slint_window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
    );
    slint_window.set_size(slint::PhysicalSize::new(
        DISPLAY_WIDTH as u32,
        DISPLAY_HEIGHT as u32,
    ));

    slint::platform::set_platform(Box::new(EspBackend {
        window: slint_window.clone(),
    }))
    .expect("Backend already initialized.  How'd this happen?");

    slint_window
}
