extern crate gtk;
extern crate gdk_pixbuf;
extern crate chrono;

use self::gdk_pixbuf::Pixbuf;
use self::gtk::prelude::*;

use types::Message;
use types::Member;
use types::Room;

use self::chrono::prelude::*;

use backend::BKCommand;

use util;

use std::sync::mpsc::channel;
use std::sync::mpsc::{Sender, Receiver};

use app::AppOp;

// Room Message item
pub struct MessageBox<'a> {
    msg: &'a Message,
    op: &'a AppOp,
}

// Room Search item
pub struct RoomBox<'a> {
    room: &'a Room,
    op: &'a AppOp,
}

impl<'a> MessageBox<'a> {
    pub fn new(msg: &'a Message, op: &'a AppOp) -> MessageBox<'a> {
        MessageBox { msg, op }
    }

    pub fn widget(&self) -> gtk::Box {
        let avatar = self.build_room_msg_avatar();

        // msg
        // +--------+---------+
        // | avatar | content |
        // +--------+---------+
        let msg_widget = gtk::Box::new(gtk::Orientation::Horizontal, 5);

        let content = self.build_room_msg_content();

        msg_widget.pack_start(&avatar, false, false, 5);
        msg_widget.pack_start(&content, true, true, 0);

        msg_widget.show_all();

        msg_widget
    }

    fn build_room_msg_content(&self) -> gtk::Box {
        // content
        // +------+
        // | info |
        // +------+
        // | body |
        // +------+
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let msg = self.msg;

        let info = self.build_room_msg_info(self.msg);

        content.pack_start(&info, false, false, 0);

        let body: gtk::Box;

        if msg.mtype == "m.image" {
            body = self.build_room_msg_image();
        } else {
            body = self.build_room_msg_body(&msg.body);
        }

        content.pack_start(&body, true, true, 0);

        content
    }

    fn build_room_msg_avatar(&self) -> gtk::Image {
        let sender = self.msg.sender.clone();
        let backend = self.op.backend.clone();
        let avatar = gtk::Image::new_from_icon_name("image-missing", 5);
        let a = avatar.clone();

        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        backend.send(BKCommand::GetAvatarAsync(sender, tx)).unwrap();
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

    fn build_room_msg_username(&self, sender: &str, member: Option<&Member>) -> gtk::Label {
        let uname = match member {
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
        let msg = gtk::Label::new("");
        msg.set_markup(&util::markup(body));
        msg.set_line_wrap(true);
        msg.set_justify(gtk::Justification::Left);
        msg.set_halign(gtk::Align::Start);
        msg.set_alignment(0 as f32, 0 as f32);

        bx.add(&msg);
        bx
    }

    fn build_room_msg_image(&self) -> gtk::Box {
        let msg = self.msg;
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

        let member = self.op.members.get(&msg.sender);
        let username = self.build_room_msg_username(&msg.sender, member);
        let date = self.build_room_msg_date(&msg.date);

        info.pack_start(&username, true, true, 0);
        info.pack_start(&date, false, false, 0);

        info
    }
}


impl<'a> RoomBox<'a> {
    pub fn new(room: &'a Room, op: &'a AppOp) -> RoomBox<'a> {
        RoomBox { room, op }
    }

    pub fn widget(&self) -> gtk::Box {
        let r = self.room;
        let op = self.op;

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
        op.backend.send(BKCommand::GetThumbAsync(r.avatar.clone(), tx)).unwrap();
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
}
