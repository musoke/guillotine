#[macro_use]
mod util;
mod backend;
mod app;

use app::App;


fn main() {
    let app = App::new();
    app.run();
}
