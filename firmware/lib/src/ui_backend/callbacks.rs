mod numeric_keypad;

use numeric_keypad::register_global_keypad_callbacks;

pub fn register_ui_callbacks(app_window: &crate::ui_types::MainWindow) {
    super::actions::register_all_callbacks(app_window);
    register_global_keypad_callbacks(app_window);
}
