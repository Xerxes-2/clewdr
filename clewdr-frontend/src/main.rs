// Almost every string here comes from the user or the server, and a panic in
// wasm takes the page down rather than one request, so byte-offset slicing is
// not worth the risk anywhere in this crate. Enabled here rather than in the
// workspace lint table because the server has slicing that is checked against
// its own parsing.
#![warn(clippy::string_slice)]

mod api;
mod components;
mod i18n;
mod storage;
mod types;
mod utils;

use components::app::App;
use leptos::prelude::*;

fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
