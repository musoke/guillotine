extern crate gtk;
extern crate gio;
extern crate gdk_pixbuf;

extern crate secret_service;
use self::secret_service::SecretService;
use self::secret_service::EncryptionType;

use std::env;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;
use std::sync::mpsc::{Sender, Receiver};

use self::gdk_pixbuf::Pixbuf;
use self::gtk::prelude::*;

use backend::Backend;
use backend;


macro_rules! derror {
    ($from: path, $to: path) => {
        impl From<$from> for Error {
            fn from(_: $from) -> Error {
                $to
            }
        }
    };
}


#[derive(Debug)]
pub enum Error {
    SecretServiceError,
}

derror!(secret_service::SsError, Error::SecretServiceError);


// TODO: Is this the correct format for GApplication IDs?
const APP_ID: &'static str = "org.gnome.guillotine";


struct AppOp {
    gtk_builder: gtk::Builder,
    backend: Backend,
}

impl AppOp {
    pub fn login(&self) {
        let user_entry: gtk::Entry = self.gtk_builder.get_object("login_username")
            .expect("Can't find login_username in ui file.");
        let pass_entry: gtk::Entry = self.gtk_builder.get_object("login_password")
            .expect("Can't find login_password in ui file.");
        let server_entry: gtk::Entry = self.gtk_builder.get_object("login_server")
            .expect("Can't find login_server in ui file.");

        let username = match user_entry.get_text() { Some(s) => s, None => String::from("") };
        let password = match pass_entry.get_text() { Some(s) => s, None => String::from("") };

        self.connect(username, password, server_entry.get_text());
    }

    pub fn connect(&self, username: String, password: String, server: Option<String>) {
        let server_url = match server {
            Some(s) => s,
            None => String::from("https://matrix.org")
        };

        self.store_pass(username.clone(), password.clone(), server_url.clone())
            .unwrap_or_else(|_| {
                // TODO: show an error
                println!("Error: Can't store the password using libsecret");
            });

        self.show_loading();
        self.backend.login(username.clone(), password.clone(), server_url.clone())
            .unwrap_or_else(move |_| {
                // TODO: show an error
                println!("Error: Can't login with {} in {}", username, server_url);
            });
        self.hide_popup();
    }

    pub fn connect_guest(&self, server: Option<String>) {
        let server_url = match server {
            Some(s) => s,
            None => String::from("https://matrix.org")
        };

        self.show_loading();
        self.backend.guest(server_url).unwrap();
    }

    pub fn get_username(&self) {
        self.backend.get_username().unwrap();
        self.backend.get_avatar().unwrap();
    }

    pub fn set_username(&self, username: &str) {
        self.gtk_builder
            .get_object::<gtk::Label>("display_name_label")
            .expect("Can't find display_name_label in ui file.")
            .set_text(username);
        self.show_username();
    }

    pub fn set_avatar(&self, fname: &str) {
        let image = self.gtk_builder
            .get_object::<gtk::Image>("profile_image")
            .expect("Can't find profile_image in ui file.");

        if let Ok(pixbuf) = Pixbuf::new_from_file_at_size(fname, 20, 20) {
            image.set_from_pixbuf(&pixbuf);
        } else {
            image.set_from_stock("image-missing", 20);
        }

        self.show_username();
    }

    pub fn show_username(&self) {
        self.gtk_builder
            .get_object::<gtk::Stack>("user_button_stack")
            .expect("Can't find user_button_stack in ui file.")
            .set_visible_child_name("user_connected_page");
    }

    pub fn show_loading(&self) {
        self.gtk_builder
            .get_object::<gtk::Stack>("user_button_stack")
            .expect("Can't find user_button_stack in ui file.")
            .set_visible_child_name("user_loading_page");
    }

    pub fn hide_popup(&self) {
        let user_menu: gtk::Popover = self.gtk_builder.get_object("user_menu")
            .expect("Couldn't find user_menu in ui file.");
        user_menu.hide();
    }

    pub fn disconnect(&self) {
        println!("Disconnecting");
    }

    pub fn store_pass(&self, username: String, password: String, server: String) -> Result<(), Error> {
        let ss = SecretService::new(EncryptionType::Dh)?;
        let collection = ss.get_default_collection()?;

        //create new item
        collection.create_item(
            "guillotine", // label
            vec![
                ("username", &username),
                ("server", &server),
            ], // properties
            password.as_bytes(), //secret
            true, // replace item with same attributes
            "text/plain" // secret content type
        )?;

        Ok(())
    }

    pub fn get_pass(&self) -> Result<(String, String, String), Error> {
        let ss = SecretService::new(EncryptionType::Dh)?;
        let collection = ss.get_default_collection()?;
        let allpass = collection.get_all_items()?;

        let passwd = allpass.iter()
            .find(|x| x.get_label().unwrap_or(String::from("")) == "guillotine");

        if passwd.is_none() {
            return Err(Error::SecretServiceError);
        }

        let p = passwd.unwrap();
        let attrs = p.get_attributes()?;
        let secret = p.get_secret()?;

        let mut attr = attrs.iter().find(|&ref x| x.0 == "username")
            .ok_or(Error::SecretServiceError)?;
        let username = attr.1.clone();
        attr = attrs.iter().find(|&ref x| x.0 == "server")
            .ok_or(Error::SecretServiceError)?;
        let server = attr.1.clone();

        let tup = (
            username,
            String::from_utf8(secret).unwrap(),
            server,
        );

        Ok(tup)
    }

    pub fn init(&self) {
        if let Ok(pass) = self.get_pass() {
            self.connect(pass.0, pass.1, Some(pass.2));
        } else {
            self.connect_guest(None);
        }
    }
}

/// State for the main thread.
///
/// It takes care of starting up the application and for loading and accessing the
/// UI.
pub struct App {
    /// GTK Application which runs the main loop.
    gtk_app: gtk::Application,

    /// Used to access the UI elements.
    gtk_builder: gtk::Builder,

    op: Arc<Mutex<AppOp>>,
}

impl App {
    /// Create an App instance
    pub fn new() -> App {
        let gtk_app = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::empty())
            .expect("Failed to initialize GtkApplication");

        let (tx, rx): (Sender<backend::BKResponse>, Receiver<backend::BKResponse>) = channel();

        let gtk_builder = gtk::Builder::new_from_file("res/main_window.glade");
        let op = Arc::new(Mutex::new(
            AppOp{
                gtk_builder: gtk_builder.clone(),
                backend: Backend::new(tx),
            }
        ));

        let theop = op.clone();
        gtk::timeout_add(500, move || {
            let recv = rx.try_recv();
            match recv {
                Ok(backend::BKResponse::Token(uid, _)) => {
                    theop.lock().unwrap().set_username(&uid);
                    theop.lock().unwrap().get_username();
                },
                Ok(backend::BKResponse::Name(username)) => {
                    theop.lock().unwrap().set_username(&username);
                },
                Ok(backend::BKResponse::Avatar(path)) => {
                    theop.lock().unwrap().set_avatar(&path);
                },
                Err(_) => { },
            };

            gtk::Continue(true)
        });

        let app = App {
            gtk_app,
            gtk_builder,
            op: op.clone(),
        };

        app.connect_gtk();
        app
    }

    pub fn connect_gtk(&self) {
        let gtk_builder = self.gtk_builder.clone();
        let op = self.op.clone();
        self.gtk_app.connect_activate(move |app| {
            // Set up shutdown callback
            let window: gtk::Window = gtk_builder.get_object("main_window")
                .expect("Couldn't find main_window in ui file.");

            window.set_title("Guillotine");

            let op_c = op.clone();
            window.connect_delete_event(clone!(app => move |_, _| {
                op_c.lock().unwrap().disconnect();
                app.quit();
                Inhibit(false)
            }));

            // Set up user popover
            let user_button: gtk::Button = gtk_builder.get_object("user_button")
                .expect("Couldn't find user_button in ui file.");

            let user_menu: gtk::Popover = gtk_builder.get_object("user_menu")
                .expect("Couldn't find user_menu in ui file.");

            user_button.connect_clicked(move |_| user_menu.show_all());

            // Login click
            let login_btn: gtk::Button = gtk_builder.get_object("login_button")
                .expect("Couldn't find login_button in ui file.");
            let op_c = op.clone();
            login_btn.connect_clicked(move |_| op_c.lock().unwrap().login());

            // Associate window with the Application and show it
            window.set_application(Some(app));
            window.show_all();
        });
    }

    pub fn run(self) {
        // Convert the args to a Vec<&str>. Application::run requires argv as &[&str]
        // and envd::args() returns an iterator of Strings.
        let args = env::args().collect::<Vec<_>>();
        let args_refs = args.iter().map(|x| &x[..]).collect::<Vec<_>>();

        self.op.lock().unwrap().init();

        // Run the main loop.
        self.gtk_app.run(args_refs.len() as i32, &args_refs);
    }
}
