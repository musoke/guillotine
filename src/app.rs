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
use std::collections::HashMap;

use self::gdk_pixbuf::Pixbuf;
use self::gtk::prelude::*;

use backend::Backend;
use backend;


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
    active_room: String,
    members: HashMap<String, backend::Member>,
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
            image.set_from_icon_name("image-missing", 2);
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

        // deleting previous items
        let allpass = collection.get_all_items()?;
        let passwds = allpass.iter()
            .filter(|x| x.get_label().unwrap_or(String::from("")) == "guillotine");
        for p in passwds {
            p.delete()?;
        }

        // create new item
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

    pub fn sync(&self) {
        self.backend.sync().unwrap();
    }

    pub fn set_rooms(&mut self, rooms: HashMap<String, String>) {
        let store: gtk::TreeStore = self.gtk_builder.get_object("rooms_tree_store")
            .expect("Couldn't find rooms_tree_store in ui file.");

        for (id, name) in rooms {
            let iter = store.insert_with_values(None, None,
                &[0, 1],
                &[&name, &id]);
        }
    }

    pub fn set_active_room(&mut self, room: String) {
        self.active_room = room;

        let messages = self.gtk_builder
            .get_object::<gtk::ListBox>("message_list")
            .expect("Can't find message_list in ui file.");
        for ch in messages.get_children() {
            messages.remove(&ch);
        }

        self.members.clear();
        let members = self.gtk_builder
            .get_object::<gtk::ListStore>("members_store")
            .expect("Can't find members_store in ui file.");
        members.clear();

        // getting room details
        self.backend.get_room_detail(self.active_room.clone(), String::from("m.room.name")).unwrap();
        self.backend.get_room_detail(self.active_room.clone(), String::from("m.room.topic")).unwrap();
        self.backend.get_room_avatar(self.active_room.clone()).unwrap();
        self.backend.get_room_messages(self.active_room.clone()).unwrap();
        self.backend.get_room_members(self.active_room.clone()).unwrap();
    }

    pub fn set_room_detail(&self, key: String, value: String) {
        let k: &str = &key;
        match k {
            "m.room.name" => {
                let name_label = self.gtk_builder
                    .get_object::<gtk::Label>("room_name")
                    .expect("Can't find room_name in ui file.");
                name_label.set_text(&value);
            },
            "m.room.topic" => {
                let topic_label = self.gtk_builder
                    .get_object::<gtk::Label>("room_topic")
                    .expect("Can't find room_topic in ui file.");
                topic_label.set_tooltip_text(&value[..]);
                topic_label.set_text(&value);
            }
            _ => { println!("no key {}", key) }
        };
    }

    pub fn set_room_avatar(&self, avatar: String) {
        let image = self.gtk_builder
            .get_object::<gtk::Image>("room_image")
            .expect("Can't find room_image in ui file.");

        if !avatar.is_empty() {
            if let Ok(pixbuf) = Pixbuf::new_from_file_at_size(&avatar, 40, 40) {
                image.set_from_pixbuf(&pixbuf);
            }
        } else {
            image.set_from_icon_name("image-missing", 5);
        }
    }

    pub fn scroll_down(&self) {
        let s = self.gtk_builder
            .get_object::<gtk::ScrolledWindow>("messages_scroll")
            .expect("Can't find message_scroll in ui file.");

        if let Some(adj) = s.get_vadjustment() {
            println!("adj: {:?}", adj);
            adj.set_value(adj.get_upper());
        }
    }

    pub fn add_room_message(&self, msg: backend::Message) {
        let messages = self.gtk_builder
            .get_object::<gtk::ListBox>("message_list")
            .expect("Can't find message_list in ui file.");

        let body = msg.b;
        let sender = msg.s;

        let mut msg = gtk::Box::new(gtk::Orientation::Horizontal, 5);

        let mut vert = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let mut label = gtk::Label::new(&body[..]);
        label.set_line_wrap(true);
        label.set_justify(gtk::Justification::Left);
        label.set_halign(gtk::Align::Start);
        label.set_alignment(0 as f32, 0 as f32);

        let mut fname = sender.clone();
        let mut avatar_url = String::new();
        if let Some(m) = self.members.get(&sender) {
            fname = m.get_alias();
            avatar_url = m.avatar.clone();
        }

        let avatar = gtk::Image::new_from_icon_name("image-missing", 5);

        if !avatar_url.is_empty() {
            let a = avatar.clone();
            let fname = self.backend.get_media_async(avatar_url).unwrap();
            let mut tries = 0;
            gtk::timeout_add(50, move || {
                match Pixbuf::new_from_file_at_size(&fname, 32, 32) {
                    Ok(pixbuf) => {
                        a.set_from_pixbuf(&pixbuf);
                        gtk::Continue(false)
                    },
                    Err(err) => {
                        match tries {
                            i if i < 200 => gtk::Continue(true),
                            _ => gtk::Continue(false),
                        }
                    }
                }
            });
        }

        let mut username = gtk::Label::new("");
        username.set_markup(&format!("<span color=\"gray\">{}</span>", fname));
        username.set_justify(gtk::Justification::Left);
        username.set_halign(gtk::Align::Start);

        vert.pack_start(&username, false, false, 0);
        vert.pack_start(&label, true, true, 0);

        msg.pack_start(&avatar, false, true, 5);
        msg.pack_start(&vert, true, true, 0);
        msg.show_all();

        messages.add(&msg);
    }

    pub fn add_room_member(&mut self, m: backend::Member) {
        if !m.avatar.is_empty() {
            self.backend.get_member_avatar(m.uid.clone(), m.avatar.clone()).unwrap();
        }

        let store: gtk::ListStore = self.gtk_builder.get_object("members_store")
            .expect("Couldn't find members_store in ui file.");

        let name = m.get_alias();

        let iter = store.insert_with_values(None,
            &[0, 1],
            &[&name, &(m.uid)]);

        self.members.insert(m.uid.clone(), m);
    }

    pub fn member_clicked(&self, uid: String) {
        println!("member clicked: {}, {:?}", uid, self.members.get(&uid));
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
                active_room: String::from(""),
                members: HashMap::new(),
            }
        ));

        let theop = op.clone();
        gtk::timeout_add(300, move || {
            let recv = rx.try_recv();
            match recv {
                Ok(backend::BKResponse::Token(uid, _)) => {
                    theop.lock().unwrap().set_username(&uid);
                    theop.lock().unwrap().get_username();
                    theop.lock().unwrap().sync();
                },
                Ok(backend::BKResponse::Name(username)) => {
                    theop.lock().unwrap().set_username(&username);
                },
                Ok(backend::BKResponse::Avatar(path)) => {
                    theop.lock().unwrap().set_avatar(&path);
                },
                Ok(backend::BKResponse::Sync) => {
                    println!("SYNC");
                    theop.lock().unwrap().sync();
                },
                Ok(backend::BKResponse::Rooms(rooms)) => {
                    theop.lock().unwrap().set_rooms(rooms);
                },
                Ok(backend::BKResponse::RoomDetail(key, value)) => {
                    theop.lock().unwrap().set_room_detail(key, value);
                },
                Ok(backend::BKResponse::RoomAvatar(avatar)) => {
                    theop.lock().unwrap().set_room_avatar(avatar);
                },
                Ok(backend::BKResponse::RoomMessage(msg)) => {
                    theop.lock().unwrap().add_room_message(msg);
                    theop.lock().unwrap().scroll_down();
                },
                Ok(backend::BKResponse::RoomMessages(msgs)) => {
                    for msg in msgs {
                        theop.lock().unwrap().add_room_message(msg);
                    }
                    theop.lock().unwrap().scroll_down();
                },
                Ok(backend::BKResponse::RoomMember(member)) => {
                    theop.lock().unwrap().add_room_member(member);
                },
                Ok(backend::BKResponse::RoomMembers(members)) => {
                    for m in members {
                        theop.lock().unwrap().add_room_member(m);
                    }
                },
                Ok(backend::BKResponse::RoomMemberAvatar(uid, avatar)) => {
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

            let mut op_c = op.clone();
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

            // room selection
            let treeview: gtk::TreeView = gtk_builder.get_object("rooms_tree_view")
                .expect("Couldn't find rooms_tree_view in ui file.");

            op_c = op.clone();
            treeview.set_activate_on_single_click(true);
            treeview.connect_row_activated(move |view, path, column| {
                let iter = view.get_model().unwrap().get_iter(path).unwrap();
                let id = view.get_model().unwrap().get_value(&iter, 1);
                op_c.lock().unwrap().set_active_room(id.get().unwrap());
            });

            // member selection
            let members: gtk::TreeView = gtk_builder.get_object("members_treeview")
                .expect("Couldn't find members_treeview in ui file.");

            op_c = op.clone();
            members.set_activate_on_single_click(true);
            members.connect_row_activated(move |view, path, column| {
                let iter = view.get_model().unwrap().get_iter(path).unwrap();
                let id = view.get_model().unwrap().get_value(&iter, 1);
                op_c.lock().unwrap().member_clicked(id.get().unwrap());
            });

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
