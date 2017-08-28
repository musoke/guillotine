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
use backend::BKCommand;
use backend::BKResponse;
use backend;

use types::Member;
use types::Message;
use types::Protocol;
use types::Room;

use util;


#[derive(Debug)]
pub enum Error {
    SecretServiceError,
}

derror!(secret_service::SsError, Error::SecretServiceError);


// TODO: Is this the correct format for GApplication IDs?
const APP_ID: &'static str = "org.gnome.guillotine";


struct AppOp {
    gtk_builder: gtk::Builder,
    backend: Sender<backend::BKCommand>,
    active_room: String,
    members: HashMap<String, Member>,
    load_more_btn: gtk::Button,
}

#[derive(Debug)]
enum MsgPos {
    Top,
    Bottom,
}

#[derive(Debug)]
enum RoomPanel {
    Room,
    NoRoom,
    Loading,
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

    pub fn register(&self) {
        let user_entry: gtk::Entry = self.gtk_builder.get_object("register_username")
            .expect("Can't find register_username in ui file.");
        let pass_entry: gtk::Entry = self.gtk_builder.get_object("register_password")
            .expect("Can't find register_password in ui file.");
        let pass_conf: gtk::Entry = self.gtk_builder.get_object("register_password_confirm")
            .expect("Can't find register_password_confirm in ui file.");
        let server_entry: gtk::Entry = self.gtk_builder.get_object("register_server")
            .expect("Can't find register_server in ui file.");

        let username = match user_entry.get_text() { Some(s) => s, None => String::from("") };
        let password = match pass_entry.get_text() { Some(s) => s, None => String::from("") };
        let passconf = match pass_conf.get_text() { Some(s) => s, None => String::from("") };

        if password != passconf {
            let window: gtk::Window = self.gtk_builder.get_object("main_window")
                .expect("Couldn't find main_window in ui file.");
            let dialog = gtk::MessageDialog::new(Some(&window),
                                                 gtk::DIALOG_MODAL,
                                                 gtk::MessageType::Warning,
                                                 gtk::ButtonsType::Ok,
                                                 "Passwords didn't match, try again");
            dialog.show();

            dialog.connect_response(move |d, _| {
                d.destroy();
            });

            return;
        }

        let server_url = match server_entry.get_text() {
            Some(s) => s,
            None => String::from("https://matrix.org")
        };

        //self.store_pass(username.clone(), password.clone(), server_url.clone())
        //    .unwrap_or_else(|_| {
        //        // TODO: show an error
        //        println!("Error: Can't store the password using libsecret");
        //    });

        self.show_user_loading();
        let uname = username.clone();
        let pass = password.clone();
        let ser = server_url.clone();
        self.backend.send(BKCommand::Register(uname, pass, ser)).unwrap();
        self.hide_popup();
        self.clear_room_list();
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

        self.show_user_loading();
        let uname = username.clone();
        let pass = password.clone();
        let ser = server_url.clone();
        self.backend.send(BKCommand::Login(uname, pass, ser)).unwrap();
        self.hide_popup();
        self.clear_room_list();
    }

    pub fn connect_guest(&self, server: Option<String>) {
        let server_url = match server {
            Some(s) => s,
            None => String::from("https://matrix.org")
        };

        self.show_user_loading();
        self.backend.send(BKCommand::Guest(server_url)).unwrap();
        self.hide_popup();
        self.clear_room_list();
    }

    pub fn get_username(&self) {
        self.backend.send(BKCommand::GetUsername).unwrap();
        self.backend.send(BKCommand::GetAvatar).unwrap();
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

    pub fn show_user_loading(&self) {
        self.gtk_builder
            .get_object::<gtk::Stack>("user_button_stack")
            .expect("Can't find user_button_stack in ui file.")
            .set_visible_child_name("user_loading_page");

        self.room_panel(RoomPanel::Loading);
    }

    pub fn hide_popup(&self) {
        let user_menu: gtk::Popover = self.gtk_builder.get_object("user_menu")
            .expect("Couldn't find user_menu in ui file.");
        user_menu.hide();
    }

    pub fn disconnect(&self) {
        self.backend.send(BKCommand::ShutDown).unwrap();
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

    pub fn room_panel(&self, t: RoomPanel) {
        let s = self.gtk_builder
            .get_object::<gtk::Stack>("room_view_stack")
            .expect("Can't find room_view_stack in ui file.");

        let v = match t {
            RoomPanel::Loading => "loading",
            RoomPanel::Room => "room_view",
            RoomPanel::NoRoom => "noroom",
        };

        s.set_visible_child_name(v);
    }

    pub fn sync(&self) {
        self.backend.send(BKCommand::Sync).unwrap();
    }

    pub fn set_rooms(&mut self, rooms: HashMap<String, String>) {
        let store: gtk::TreeStore = self.gtk_builder.get_object("rooms_tree_store")
            .expect("Couldn't find rooms_tree_store in ui file.");

        let mut array: Vec<(String, String)> = vec![];
        for (id, name) in rooms {
            array.push((name, id));
        }

        array.sort_by(|x, y| x.0.to_lowercase().cmp(&y.0.to_lowercase()));

        let mut default: Option<(String, String)> = None;

        for v in array {
            if default.is_none() {
                default = Some((v.0.clone(), v.1.clone()));
            }

            store.insert_with_values(None, None,
                &[0, 1],
                &[&v.0, &v.1]);
        }

        if let Some(def) = default {
            self.set_active_room(def.1, def.0);
        } else {
            self.room_panel(RoomPanel::NoRoom);
        }
    }

    pub fn clear_room_list(&self) {
        let store: gtk::TreeStore = self.gtk_builder.get_object("rooms_tree_store")
            .expect("Couldn't find rooms_tree_store in ui file.");
        store.clear();
    }

    pub fn set_active_room(&mut self, room: String, name: String) {
        self.active_room = room;

        self.room_panel(RoomPanel::Loading);

        let messages = self.gtk_builder
            .get_object::<gtk::ListBox>("message_list")
            .expect("Can't find message_list in ui file.");
        for ch in messages.get_children().iter().skip(1) {
            messages.remove(ch);
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
        self.backend.send(BKCommand::SetRoom(self.active_room.clone())).unwrap();
    }

    pub fn get_room_messages(&self) {
        self.backend.send(BKCommand::GetRoomMessages(self.active_room.clone())).unwrap();
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
        self.backend.send(BKCommand::GetAvatarAsync(String::from(sender), tx)).unwrap();
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

    fn build_room_msg_body(&self, body: &str) -> gtk::Box {
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let msg = gtk::Label::new(body);
        msg.set_line_wrap(true);
        msg.set_justify(gtk::Justification::Left);
        msg.set_halign(gtk::Align::Start);
        msg.set_alignment(0 as f32, 0 as f32);

        bx.add(&msg);
        bx
    }

    fn build_room_msg_image(&self, msg: &Message) -> gtk::Box {
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let image = gtk::Image::new();

        if let Ok(pixbuf) = Pixbuf::new_from_file_at_size(&msg.thumb, 200, 200) {
            image.set_from_pixbuf(&pixbuf);
        }

        let viewbtn = gtk::Button::new();
        let url = msg.url.clone();
        viewbtn.connect_clicked(move |_| {
            println!("Download and show a dialog: {}", url);
        });

        viewbtn.set_image(&image);

        bx.add(&viewbtn);
        bx
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

    fn build_room_msg_info(&self, msg: &Message) -> gtk::Box {
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

    fn build_room_msg_content(&self, msg: &Message) -> gtk::Box {
        // content
        // +------+
        // | info |
        // +------+
        // | body |
        // +------+
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let info = self.build_room_msg_info(msg);

        content.pack_start(&info, false, false, 0);

        let body: gtk::Box;

        if msg.mtype == "m.image" {
            body = self.build_room_msg_image(msg);
        } else {
            body = self.build_room_msg_body(&msg.body);
        }

        content.pack_start(&body, true, true, 0);

        content

    }

    fn build_room_msg(&self, msg: &Message) -> gtk::Box {
        let avatar = self.build_room_msg_avatar(&msg.sender);

        // msg
        // +--------+---------+
        // | avatar | content |
        // +--------+---------+
        let msg_widget = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        let content = self.build_room_msg_content(msg);

        msg_widget.pack_start(&avatar, false, false, 5);
        msg_widget.pack_start(&content, true, true, 0);

        msg_widget.show_all();

        msg_widget
    }

    pub fn add_room_message(&self, msg: &Message, msgpos: MsgPos) {
        let messages = self.gtk_builder
            .get_object::<gtk::ListBox>("message_list")
            .expect("Can't find message_list in ui file.");

        if msg.room == self.active_room {
            let msg = self.build_room_msg(msg);


            match msgpos {
                MsgPos::Bottom => messages.add(&msg),
                MsgPos::Top => messages.insert(&msg, 1),
            };

        } else {
            // TODO: update the unread messages count in room list
        }
    }

    pub fn add_room_member(&mut self, m: Member) {
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

    pub fn send_message(&self, msg: String) {
        let room = self.active_room.clone();
        self.backend.send(BKCommand::SendMsg(room, msg)).unwrap();
    }

    pub fn hide_members(&self) {
        self.gtk_builder
            .get_object::<gtk::Stack>("sidebar_stack")
            .expect("Can't find sidebar_stack in ui file.")
            .set_visible_child_name("sidebar_hidden");
    }

    pub fn show_members(&self) {
        self.gtk_builder
            .get_object::<gtk::Stack>("sidebar_stack")
            .expect("Can't find sidebar_stack in ui file.")
            .set_visible_child_name("sidebar_members");
    }

    pub fn load_more_messages(&self) {
        let room = self.active_room.clone();
        self.load_more_btn.set_label("loading...");
        self.backend.send(BKCommand::GetRoomMessagesTo(room)).unwrap();
    }

    pub fn load_more_normal(&self) {
        self.load_more_btn.set_label("load more messages");
    }

    pub fn init_protocols(&self) {
        self.backend.send(BKCommand::DirectoryProtocols).unwrap();
    }

    pub fn set_protocols(&self, protocols: Vec<Protocol>) {
        let combo = self.gtk_builder
            .get_object::<gtk::ListStore>("protocol_model")
            .expect("Can't find protocol_model in ui file.");
        combo.clear();

        for p in protocols {
            combo.insert_with_values(None,
                &[0, 1],
                &[&p.desc, &p.id]);
        }

        self.gtk_builder
            .get_object::<gtk::ComboBox>("directory_combo")
            .expect("Can't find directory_combo in ui file.")
            .set_active(0);
    }

    pub fn search_rooms(&self, more: bool) {
        let combo_store = self.gtk_builder
            .get_object::<gtk::ListStore>("protocol_model")
            .expect("Can't find protocol_model in ui file.");
        let combo = self.gtk_builder
            .get_object::<gtk::ComboBox>("directory_combo")
            .expect("Can't find directory_combo in ui file.");

        let active = combo.get_active();
        let protocol: String = match combo_store.iter_nth_child(None, active) {
            Some(it) => {
                let v = combo_store.get_value(&it, 1);
                v.get().unwrap()
            },
            None => String::from(""),
        };

        let q = self.gtk_builder
            .get_object::<gtk::Entry>("directory_search_entry")
            .expect("Can't find directory_search_entry in ui file.");

        let btn = self.gtk_builder
            .get_object::<gtk::Button>("directory_search_button")
            .expect("Can't find directory_search_button in ui file.");
        btn.set_label("Searching...");
        btn.set_sensitive(false);

        if !more {
            let directory = self.gtk_builder
                .get_object::<gtk::ListBox>("directory_room_list")
                .expect("Can't find directory_room_list in ui file.");
            for ch in directory.get_children() {
                directory.remove(&ch);
            }
        }

        self.backend.send(BKCommand::DirectorySearch(q.get_text().unwrap(), protocol, more)).unwrap();
    }

    pub fn build_widget_for_room(&self, r: &Room) -> gtk::Box {
        let h = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let w = gtk::Box::new(gtk::Orientation::Horizontal, 5);

        let mname = match r.name {
            ref n if n.is_empty() => r.alias.clone(),
            ref n => n.clone(),
        };

        let avatar = gtk::Image::new_from_icon_name("image-missing", 5);
        let a = avatar.clone();
        let id = r.id.clone();
        let name = mname.clone();
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        self.backend.send(BKCommand::GetThumbAsync(r.avatar.clone(), tx)).unwrap();
        gtk::timeout_add(50, move || {
            match rx.try_recv() {
                Err(_) => gtk::Continue(true),
                Ok(fname) => {
                    let mut f = fname.clone();
                    if f.is_empty() {
                        f = util::draw_identicon(&id, name.clone()).unwrap();
                    }
                    if let Ok(pixbuf) = Pixbuf::new_from_file_at_scale(&f, 32, 32, false) {
                        a.set_from_pixbuf(&pixbuf);
                    }
                    gtk::Continue(false)
                }
            }
        });
        w.pack_start(&avatar, false, false, 0);

        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let msg = gtk::Label::new("");
        msg.set_line_wrap(true);
        msg.set_markup(&format!("<b>{}</b>", mname));
        msg.set_justify(gtk::Justification::Left);
        msg.set_halign(gtk::Align::Start);
        msg.set_alignment(0 as f32, 0 as f32);

        let topic = gtk::Label::new("");
        topic.set_line_wrap(true);
        topic.set_markup(&util::markup(&r.topic));
        topic.set_justify(gtk::Justification::Left);
        topic.set_halign(gtk::Align::Start);
        topic.set_alignment(0 as f32, 0 as f32);

        let idw = gtk::Label::new("");
        idw.set_markup(&format!("<span alpha=\"60%\">{}</span>", r.alias));
        idw.set_justify(gtk::Justification::Left);
        idw.set_halign(gtk::Align::Start);
        idw.set_alignment(0 as f32, 0 as f32);

        //TODO add join button

        b.add(&msg);
        b.add(&topic);
        b.add(&idw);
        w.pack_start(&b, true, true, 0);

        let members = gtk::Label::new(&format!("{}", r.members)[..]);
        w.pack_start(&members, false, false, 5);

        h.add(&w);
        h.add(&gtk::Separator::new(gtk::Orientation::Horizontal));
        h.show_all();
        h
    }

    pub fn load_more_rooms(&self) {
        self.search_rooms(true);
    }

    pub fn set_directory_room(&self, room: Room) {
        let directory = self.gtk_builder
            .get_object::<gtk::ListBox>("directory_room_list")
            .expect("Can't find directory_room_list in ui file.");

        let room_widget = self.build_widget_for_room(&room);
        directory.add(&room_widget);

        let btn = self.gtk_builder
            .get_object::<gtk::Button>("directory_search_button")
            .expect("Can't find directory_search_button in ui file.");
        btn.set_label("Search");
        btn.set_sensitive(true);
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

        let (tx, rx): (Sender<BKResponse>, Receiver<BKResponse>) = channel();

        let bk = Backend::new(tx);
        let apptx = bk.run();

        let gtk_builder = gtk::Builder::new_from_file("res/main_window.glade");
        let op = Arc::new(Mutex::new(
            AppOp{
                gtk_builder: gtk_builder.clone(),
                load_more_btn: gtk::Button::new_with_label("Load more messages"),
                backend: apptx,
                active_room: String::from(""),
                members: HashMap::new(),
            }
        ));

        let theop = op.clone();
        gtk::timeout_add(50, move || {
            let recv = rx.try_recv();
            match recv {
                Ok(BKResponse::Token(uid, _)) => {
                    theop.lock().unwrap().set_username(&uid);
                    theop.lock().unwrap().get_username();
                    theop.lock().unwrap().sync();

                    theop.lock().unwrap().init_protocols();
                },
                Ok(BKResponse::Name(username)) => {
                    theop.lock().unwrap().set_username(&username);
                },
                Ok(BKResponse::Avatar(path)) => {
                    theop.lock().unwrap().set_avatar(&path);
                },
                Ok(BKResponse::Sync) => {
                    println!("SYNC");
                    theop.lock().unwrap().sync();
                },
                Ok(BKResponse::Rooms(rooms)) => {
                    theop.lock().unwrap().set_rooms(rooms);
                },
                Ok(BKResponse::RoomDetail(key, value)) => {
                    theop.lock().unwrap().set_room_detail(key, value);
                },
                Ok(BKResponse::RoomAvatar(avatar)) => {
                    theop.lock().unwrap().set_room_avatar(avatar);
                },
                Ok(BKResponse::RoomMessages(msgs)) => {
                    for msg in msgs.iter() {
                        theop.lock().unwrap().add_room_message(msg, MsgPos::Bottom);
                    }

                    if !msgs.is_empty() {
                        theop.lock().unwrap().scroll_down();
                    }

                    theop.lock().unwrap().room_panel(RoomPanel::Room);
                },
                Ok(BKResponse::RoomMessagesTo(msgs)) => {
                    for msg in msgs.iter().rev() {
                        theop.lock().unwrap().add_room_message(msg, MsgPos::Top);
                    }
                    theop.lock().unwrap().load_more_normal();
                },
                Ok(BKResponse::RoomMembers(members)) => {
                    let mut ms = members;
                    ms.sort_by(|x, y| x.get_alias().to_lowercase().cmp(&y.get_alias().to_lowercase()));
                    for m in ms {
                        theop.lock().unwrap().add_room_member(m);
                    }
                    theop.lock().unwrap().get_room_messages();
                },
                Ok(BKResponse::SendMsg) => { },
                Ok(BKResponse::DirectoryProtocols(protocols)) => {
                    theop.lock().unwrap().set_protocols(protocols);
                },
                Ok(BKResponse::DirectorySearch(rooms)) => {
                    for room in rooms {
                        theop.lock().unwrap().set_directory_room(room);
                    }
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
        let _ = window.set_icon_from_file("res/icon.svg");
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

        self.create_load_more_btn();

        self.connect_user_button();
        self.connect_login_button();
        self.connect_register_button();
        self.connect_guest_button();

        self.connect_room_treeview();
        self.connect_member_treeview();

        self.connect_msg_scroll();

        self.connect_send();

        self.connect_directory();
    }

    fn connect_directory(&self) {
        let btn = self.gtk_builder
            .get_object::<gtk::Button>("directory_search_button")
            .expect("Can't find directory_search_button in ui file.");
        let q = self.gtk_builder
            .get_object::<gtk::Entry>("directory_search_entry")
            .expect("Can't find directory_search_entry in ui file.");

        let scroll = self.gtk_builder
            .get_object::<gtk::ScrolledWindow>("directory_scroll")
            .expect("Can't find directory_scroll in ui file.");

        let mut op = self.op.clone();
        btn.connect_clicked(move |_| {
            op.lock().unwrap().search_rooms(false);
        });

        op = self.op.clone();
        scroll.connect_edge_reached(move |_, dir| {
            if dir == gtk::PositionType::Bottom {
                op.lock().unwrap().load_more_rooms();
            }
        });

        op = self.op.clone();
        q.connect_activate(move |_| {
            op.lock().unwrap().search_rooms(false);
        });
    }

    fn create_load_more_btn(&self) {
        let messages = self.gtk_builder
            .get_object::<gtk::ListBox>("message_list")
            .expect("Can't find message_list in ui file.");

        let btn = self.op.lock().unwrap().load_more_btn.clone();
        btn.show();
        messages.add(&btn);

        let op = self.op.clone();
        btn.connect_clicked(move |_| {
            op.lock().unwrap().load_more_messages();
        });
    }

    fn connect_msg_scroll(&self) {
        let s = self.gtk_builder
            .get_object::<gtk::ScrolledWindow>("messages_scroll")
            .expect("Can't find message_scroll in ui file.");

        let op = self.op.clone();
        s.connect_edge_overshot(move |_, dir| {
            if dir == gtk::PositionType::Top {
                op.lock().unwrap().load_more_messages();
            }
        });
    }

    fn connect_send(&self) {
        let send_button: gtk::ToolButton = self.gtk_builder.get_object("send_button")
            .expect("Couldn't find send_button in ui file.");
        let msg_entry: gtk::Entry = self.gtk_builder.get_object("msg_entry")
            .expect("Couldn't find msg_entry in ui file.");

        let entry = msg_entry.clone();
        let mut op = self.op.clone();
        send_button.connect_clicked(move |_| {
            if let Some(text) = entry.get_text() {
                op.lock().unwrap().send_message(text);
                entry.set_text("");
            }
        });

        op = self.op.clone();
        msg_entry.connect_activate(move |entry| {
            if let Some(text) = entry.get_text() {
                op.lock().unwrap().send_message(text);
                entry.set_text("");
            }
        });
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

    fn connect_register_button(&self) {
        let btn: gtk::Button = self.gtk_builder.get_object("register_button")
            .expect("Couldn't find register_button in ui file.");

        let op = self.op.clone();
        btn.connect_clicked(move |_| op.lock().unwrap().register());
    }

    fn connect_guest_button(&self) {
        let btn: gtk::Button = self.gtk_builder.get_object("guest_button")
            .expect("Couldn't find guest_button in ui file.");

        let op = self.op.clone();
        let builder = self.gtk_builder.clone();
        btn.connect_clicked(move |_| {
            let server: gtk::Entry = builder.get_object("guest_server")
                .expect("Can't find guest_server in ui file.");
            op.lock().unwrap().connect_guest(server.get_text());
        });
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

        let mbutton: gtk::Button = self.gtk_builder.get_object("members_hide_button")
            .expect("Couldn't find members_hide_button in ui file.");
        let mbutton2: gtk::Button = self.gtk_builder.get_object("members_show_button")
            .expect("Couldn't find members_show_button in ui file.");

        let op = self.op.clone();
        mbutton.connect_clicked(move |_| {
            op.lock().unwrap().hide_members();
        });
        let op = self.op.clone();
        mbutton2.connect_clicked(move |_| {
            op.lock().unwrap().show_members();
        });

    }

    pub fn run(self) {
        self.op.lock().unwrap().init();

        gtk::main();
    }
}
