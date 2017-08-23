extern crate gtk;
extern crate gio;
extern crate gdk_pixbuf;
extern crate chrono;

extern crate secret_service;
use self::secret_service::SecretService;
use self::secret_service::EncryptionType;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::channel;
use std::sync::mpsc::{Sender, Receiver};
use std::collections::HashMap;

use self::gio::ApplicationExt;
use self::gdk_pixbuf::Pixbuf;
use self::gtk::prelude::*;

use self::chrono::prelude::*;

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

        let mut array: Vec<(String, String)> = vec![];
        for (id, name) in rooms {
            array.push((name, id));
        }

        array.sort_by(|x, y| x.0.to_lowercase().cmp(&y.0.to_lowercase()));

        for v in array {
            store.insert_with_values(None, None,
                &[0, 1],
                &[&v.0, &v.1]);
        }
    }

    pub fn set_active_room(&mut self, room: String, name: String) {
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

        let name_label = self.gtk_builder
            .get_object::<gtk::Label>("room_name")
            .expect("Can't find room_name in ui file.");
        name_label.set_text(&name);

        // getting room details
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

    fn build_room_msg_avatar(&self, sender: &str) -> gtk::Image {
        let avatar = gtk::Image::new_from_icon_name("image-missing", 5);
        let a = avatar.clone();

        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        self.backend.get_avatar_async(sender, tx).unwrap();
        gtk::timeout_add(50, move || {
            match rx.try_recv() {
                Err(_) => gtk::Continue(true),
                Ok(fname) => {
                    if let Ok(pixbuf) = Pixbuf::new_from_file_at_scale(&fname, 32, 32, false) {
                        a.set_from_pixbuf(&pixbuf);
                    }
                    gtk::Continue(false)
                }
            }
        });
        avatar.set_alignment(0.5, 0.);

        avatar
    }

    fn build_room_msg_username(&self, sender: &str) -> gtk::Label {
        let uname = match self.members.get(sender) {
            Some(m) => m.get_alias(),
            None => String::from(sender)
        };

        let username = gtk::Label::new("");
        username.set_markup(&format!("<b>{}</b>", uname));
        username.set_justify(gtk::Justification::Left);
        username.set_halign(gtk::Align::Start);

        username
    }

    fn build_room_msg_body(&self, body: &str) -> gtk::Label {
        let msg = gtk::Label::new(body);
        msg.set_line_wrap(true);
        msg.set_justify(gtk::Justification::Left);
        msg.set_halign(gtk::Align::Start);
        msg.set_alignment(0 as f32, 0 as f32);

        msg
    }

    fn build_room_msg_date(&self, dt: &DateTime<Local>) -> gtk::Label {
        let d = dt.format("%d/%b/%y %H:%M").to_string();

        let date = gtk::Label::new("");
        date.set_markup(&format!("<span alpha=\"60%\">{}</span>", d));
        date.set_line_wrap(true);
        date.set_justify(gtk::Justification::Right);
        date.set_halign(gtk::Align::End);
        date.set_alignment(1 as f32, 0 as f32);

        date
    }

    fn build_room_msg_info(&self, msg: &backend::Message) -> gtk::Box {
        // info
        // +----------+------+
        // | username | date |
        // +----------+------+
        let info = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        let username = self.build_room_msg_username(&msg.sender);
        let date = self.build_room_msg_date(&msg.date);

        info.pack_start(&username, true, true, 0);
        info.pack_start(&date, false, false, 0);

        info
    }

    fn build_room_msg_content(&self, msg: &backend::Message) -> gtk::Box {
        // content
        // +------+
        // | info |
        // +------+
        // | body |
        // +------+
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let info = self.build_room_msg_info(msg);
        let body = self.build_room_msg_body(&msg.body);

        content.pack_start(&info, false, false, 0);
        content.pack_start(&body, true, true, 0);

        content

    }

    fn build_room_msg(&self, msg: backend::Message) -> gtk::Box {
        let avatar = self.build_room_msg_avatar(&msg.sender);

        // msg
        // +--------+---------+
        // | avatar | content |
        // +--------+---------+
        let msg_widget = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        let content = self.build_room_msg_content(&msg);

        msg_widget.pack_start(&avatar, false, false, 5);
        msg_widget.pack_start(&content, true, true, 0);

        msg_widget.show_all();

        msg_widget
    }

    pub fn add_room_message(&self, msg: backend::Message) {
        let messages = self.gtk_builder
            .get_object::<gtk::ListBox>("message_list")
            .expect("Can't find message_list in ui file.");

        let msg = self.build_room_msg(msg);

        messages.add(&msg);
    }

    pub fn add_room_member(&mut self, m: backend::Member) {
        if !m.avatar.is_empty() {
            self.backend.get_member_avatar(m.uid.clone(), m.avatar.clone()).unwrap();
        }

        let store: gtk::ListStore = self.gtk_builder.get_object("members_store")
            .expect("Couldn't find members_store in ui file.");

        let name = m.get_alias();

        store.insert_with_values(None,
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
                Ok(backend::BKResponse::RoomMessages(msgs)) => {
                    for msg in msgs {
                        theop.lock().unwrap().add_room_message(msg);
                    }
                    theop.lock().unwrap().scroll_down();
                },
                Ok(backend::BKResponse::RoomMembers(members)) => {
                    let mut ms = members;
                    ms.sort_by(|x, y| x.get_alias().to_lowercase().cmp(&y.get_alias().to_lowercase()));
                    for m in ms {
                        theop.lock().unwrap().add_room_member(m);
                    }
                },
                Ok(backend::BKResponse::RoomMemberAvatar(_, _)) => {
                },
                // errors
                Ok(err) => {
                    println!("Query error: {:?}", err);
                }
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
        // Set up shutdown callback
        let window: gtk::Window = self.gtk_builder.get_object("main_window")
            .expect("Couldn't find main_window in ui file.");

        window.set_title("Guillotine");
        window.show_all();

        let op = self.op.clone();
        window.connect_delete_event(move |_, _| {
            op.lock().unwrap().disconnect();
            gtk::main_quit();
            Inhibit(false)
        });

        self.gtk_app.connect_startup(move |app| {
            window.set_application(app);
        });

        self.connect_user_button();
        self.connect_login_button();

        self.connect_room_treeview();
        self.connect_member_treeview();
    }

    fn connect_user_button(&self) {
        // Set up user popover
        let user_button: gtk::Button = self.gtk_builder.get_object("user_button")
            .expect("Couldn't find user_button in ui file.");

        let user_menu: gtk::Popover = self.gtk_builder.get_object("user_menu")
            .expect("Couldn't find user_menu in ui file.");

        user_button.connect_clicked(move |_| user_menu.show_all());
    }

    fn connect_login_button(&self) {
        // Login click
        let login_btn: gtk::Button = self.gtk_builder.get_object("login_button")
            .expect("Couldn't find login_button in ui file.");

        let op = self.op.clone();
        login_btn.connect_clicked(move |_| op.lock().unwrap().login());
    }

    fn connect_room_treeview(&self) {
        // room selection
        let treeview: gtk::TreeView = self.gtk_builder.get_object("rooms_tree_view")
            .expect("Couldn't find rooms_tree_view in ui file.");

        let op = self.op.clone();
        treeview.set_activate_on_single_click(true);
        treeview.connect_row_activated(move |view, path, _| {
            let iter = view.get_model().unwrap().get_iter(path).unwrap();
            let id = view.get_model().unwrap().get_value(&iter, 1);
            let name = view.get_model().unwrap().get_value(&iter, 0);
            op.lock().unwrap().set_active_room(id.get().unwrap(), name.get().unwrap());
        });
    }

    fn connect_member_treeview(&self) {
        // member selection
        let members: gtk::TreeView = self.gtk_builder.get_object("members_treeview")
            .expect("Couldn't find members_treeview in ui file.");

        let op = self.op.clone();
        members.set_activate_on_single_click(true);
        members.connect_row_activated(move |view, path, _| {
            let iter = view.get_model().unwrap().get_iter(path).unwrap();
            let id = view.get_model().unwrap().get_value(&iter, 1);
            op.lock().unwrap().member_clicked(id.get().unwrap());
        });
    }

    pub fn run(self) {
        self.op.lock().unwrap().init();

        gtk::main();
    }
}
