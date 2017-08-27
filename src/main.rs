#[macro_use]
extern crate serde_json;
#[macro_use]
mod util;
mod error;
mod types;
mod backend;
mod app;

use app::App;


fn main() {
    let app = App::new();
    app.run();
}
