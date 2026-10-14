//! Owned clipboard supplier. stdin EOF stops the process even inside Wayland calls.
use dipper_desktop::clipboard_helper;
fn main() {
    if clipboard_helper().is_err() {
        std::process::exit(1);
    }
}
