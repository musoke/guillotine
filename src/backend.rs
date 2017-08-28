extern crate url;
extern crate reqwest;
extern crate xdg;
extern crate serde_json;
extern crate chrono;
extern crate time;
extern crate cairo;


use self::serde_json::Value as JsonValue;

use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;
use self::url::Url;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc::channel;
use std::sync::mpsc::RecvError;

use util::*;
use error::Error;

use types::Message;
use types::Member;
use types::Protocol;
use types::Room;


pub struct BackendData {
    user_id: String,
    access_token: String,
    server_url: String,
    since: String,
    msgid: i32,
    msgs_batch_start: String,
    msgs_batch_end: String,
    rooms_since: String,
}

pub struct Backend {
    tx: Sender<BKResponse>,
    data: Arc<Mutex<BackendData>>,
}

#[derive(Debug)]
pub enum BKCommand {
    Login(String, String, String),
    Register(String, String, String),
    Guest(String),
    GetUsername,
    GetAvatar,
    Sync,
    GetRoomMessages(String),
    GetRoomMessagesTo(String),
    GetAvatarAsync(String, Sender<String>),
    GetThumbAsync(String, Sender<String>),
    SendMsg(String, String),
    SetRoom(String),
    ShutDown,
    DirectoryProtocols,
    DirectorySearch(String, String, bool),
}

#[derive(Debug)]
pub enum BKResponse {
    Token(String, String),
    Name(String),
    Avatar(String),
    Sync,
    Rooms(HashMap<String, String>),
    RoomDetail(String, String),
    RoomAvatar(String),
    RoomMessages(Vec<Message>),
    RoomMessagesTo(Vec<Message>),
    RoomMembers(Vec<Member>),
    SendMsg,
    DirectoryProtocols(Vec<Protocol>),
    DirectorySearch(Vec<Room>),

    //errors
    UserNameError(Error),
    AvatarError(Error),
    LoginError(Error),
    GuestLoginError(Error),
    SyncError(Error),
    RoomDetailError(Error),
    RoomAvatarError(Error),
    RoomMessagesError(Error),
    RoomMembersError(Error),
    SendMsgError(Error),
    SetRoomError(Error),
    CommandError(Error),
    DirectoryError(Error),
}


impl Backend {
    pub fn new(tx: Sender<BKResponse>) -> Backend {
        let data = BackendData {
                    user_id: String::from("Guest"),
                    access_token: String::from(""),
                    server_url: String::from("https://matrix.org"),
                    since: String::from(""),
                    msgid: 1,
                    msgs_batch_start: String::from(""),
                    msgs_batch_end: String::from(""),
                    rooms_since: String::from(""),
        };
        Backend { tx: tx, data: Arc::new(Mutex::new(data)) }
    }

    pub fn command_recv(&self, cmd: Result<BKCommand, RecvError>) -> bool {
        let tx = &self.tx;

        match cmd {
            Ok(BKCommand::Login(user, passwd, server)) => {
                let r = self.login(user, passwd, server);
                bkerror!(r, tx, BKResponse::LoginError);
            },
            Ok(BKCommand::Register(user, passwd, server)) => {
                let r = self.register(user, passwd, server);
                bkerror!(r, tx, BKResponse::LoginError);
            },
            Ok(BKCommand::Guest(server)) => {
                let r = self.guest(server);
                bkerror!(r, tx, BKResponse::GuestLoginError);
            },
            Ok(BKCommand::GetUsername) => {
                let r = self.get_username();
                bkerror!(r, tx, BKResponse::UserNameError);
            },
            Ok(BKCommand::GetAvatar) => {
                let r = self.get_avatar();
                bkerror!(r, tx, BKResponse::AvatarError);
            },
            Ok(BKCommand::Sync) => {
                let r = self.sync();
                bkerror!(r, tx, BKResponse::SyncError);
            },
            Ok(BKCommand::GetRoomMessages(room)) => {
                let r = self.get_room_messages(room, false);
                bkerror!(r, tx, BKResponse::RoomMessagesError);
            },
            Ok(BKCommand::GetRoomMessagesTo(room)) => {
                let r = self.get_room_messages(room, true);
                bkerror!(r, tx, BKResponse::RoomMessagesError);
            },
            Ok(BKCommand::GetAvatarAsync(sender, ctx)) => {
                let r = self.get_avatar_async(&sender, ctx);
                bkerror!(r, tx, BKResponse::CommandError);
            },
            Ok(BKCommand::GetThumbAsync(media, ctx)) => {
                let r = self.get_thumb_async(media, ctx);
                bkerror!(r, tx, BKResponse::CommandError);
            },
            Ok(BKCommand::SendMsg(room, msg)) => {
                let r = self.send_msg(room, msg);
                bkerror!(r, tx, BKResponse::SendMsgError);
            },
            Ok(BKCommand::SetRoom(room)) => {
                let r = self.set_room(room);
                bkerror!(r, tx, BKResponse::SetRoomError);
            },
            Ok(BKCommand::DirectoryProtocols) => {
                let r = self.protocols();
                bkerror!(r, tx, BKResponse::DirectoryError);
            },
            Ok(BKCommand::DirectorySearch(dq, dtp, more)) => {
                let q = match dq {
                    ref a if a.is_empty() => None,
                    b => Some(b),
                };

                let tp = match dtp {
                    ref a if a.is_empty() => None,
                    b => Some(b),
                };

                let r = self.room_search(q, tp, more);
                bkerror!(r, tx, BKResponse::DirectoryError);
            },
            Ok(BKCommand::ShutDown) => {
                return false;
            },
            Err(_) => {
                return false;
            }
        };

        true
    }

    pub fn run(self) -> Sender<BKCommand> {
        let (apptx, rx): (Sender<BKCommand>, Receiver<BKCommand>) = channel();

        thread::spawn(move || {
            loop {
                let cmd = rx.recv();
                if ! self.command_recv(cmd) {
                    break;
                }
            }
        });

        apptx
    }

    pub fn set_room(&self, roomid: String) -> Result<(), Error> {
        self.get_room_detail(roomid.clone(), String::from("m.room.topic"))?;
        self.get_room_avatar(roomid.clone())?;
        self.get_room_members(roomid.clone())?;

        Ok(())
    }

    pub fn guest(&self, server: String) -> Result<(), Error> {
        let s = server.clone();
        let url = Url::parse(&s).unwrap().join("/_matrix/client/r0/register?kind=guest")?;
        self.data.lock().unwrap().server_url = s;

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(&url,
            |r: JsonValue| {
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));
                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                data.lock().unwrap().since = String::from("");
                data.lock().unwrap().msgs_batch_end = String::from("");
                data.lock().unwrap().msgs_batch_start = String::from("");
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            },
            |err| { tx.send(BKResponse::GuestLoginError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn login(&self, user: String, password: String, server: String) -> Result<(), Error> {
        let s = server.clone();
        let url = Url::parse(&s)?.join("/_matrix/client/r0/login")?;
        self.data.lock().unwrap().server_url = s;

        let attrs = json!({
            "type": "m.login.password",
            "user": user,
            "password": password
        });

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(&url, &attrs,
            |r: JsonValue| {
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));

                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                data.lock().unwrap().since = String::from("");
                data.lock().unwrap().msgs_batch_end = String::from("");
                data.lock().unwrap().msgs_batch_start = String::from("");
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            },
            |err| { tx.send(BKResponse::LoginError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn register(&self, user: String, password: String, server: String) -> Result<(), Error> {
        let s = server.clone();
        let url = Url::parse(&s).unwrap().join("/_matrix/client/r0/register?kind=user")?;
        self.data.lock().unwrap().server_url = s;

        let attrs = json!({
            "auth": {"type": "m.login.password"},
            "username": user,
            "bind_email": false,
            "password": password
        });

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(&url, &attrs,
            |r: JsonValue| {
                println!("RESPONSE: {:#?}", r);
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));

                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                data.lock().unwrap().since = String::from("");
                data.lock().unwrap().msgs_batch_end = String::from("");
                data.lock().unwrap().msgs_batch_start = String::from("");
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            },
            |err| { tx.send(BKResponse::LoginError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_username(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let uid = self.data.lock().unwrap().user_id.clone();
        let id = uid.clone() + "/";
        let url = baseu.join("/_matrix/client/r0/profile/")?.join(&id)?.join("displayname")?;

        let tx = self.tx.clone();
        get!(&url,
            |r: JsonValue| {
                let name = String::from(r["displayname"].as_str().unwrap_or(&uid));
                tx.send(BKResponse::Name(name)).unwrap();
            },
            |err| { tx.send(BKResponse::UserNameError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_avatar(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let userid = self.data.lock().unwrap().user_id.clone();

        let tx = self.tx.clone();
        thread::spawn(move || {
            match get_user_avatar(&baseu, &userid) {
                Ok(fname) => {
                    tx.send(BKResponse::Avatar(fname)).unwrap();
                },
                Err(err) => {
                    tx.send(BKResponse::AvatarError(err)).unwrap();
                }
            }
        });

        Ok(())
    }

    pub fn sync(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let token = self.data.lock().unwrap().access_token.clone();
        let since = self.data.lock().unwrap().since.clone();
        let userid = self.data.lock().unwrap().user_id.clone();

        let mut params: String;

        if since.is_empty() {
            params = format!("?full_state=false&timeout=30000&access_token={}", token);
            params = params + "&filter={\
                                          \"room\": {\
                                            \"state\": {\
                                                \"types\": [\"m.room.*\"],\
                                            },\
                                            \"timeline\": {\"limit\":0},\
                                            \"ephemeral\": {\"types\": []}\
                                          },\
                                          \"presence\": {\"types\": []},\
                                          \"event_format\": \"client\",\
                                          \"event_fields\": [\"type\", \"content\", \"sender\"]\

                                      }";
        } else {
            params = format!("?full_state=false&timeout=30000&access_token={}&since={}", token, since);
        }

        let url = baseu.join("/_matrix/client/r0/sync")?.join(&params)?;

        let tx = self.tx.clone();
        let data = self.data.clone();
        get!(&url,
            |r: JsonValue| {
                let next_batch = String::from(r["next_batch"].as_str().unwrap_or(""));
                if since.is_empty() {
                    let rooms = get_rooms_from_json(r, &userid).unwrap();
                    tx.send(BKResponse::Rooms(rooms)).unwrap();
                } else {
                    match get_rooms_timeline_from_json(&baseu, r) {
                        Ok(msgs) => tx.send(BKResponse::RoomMessages(msgs)).unwrap(),
                        Err(err) => tx.send(BKResponse::RoomMessagesError(err)).unwrap(),
                    }
                    // TODO: treat all events
                    //println!("sync: {:#?}", r);
                }

                data.lock().unwrap().since = next_batch;

                tx.send(BKResponse::Sync).unwrap();
            },
            |err| { tx.send(BKResponse::SyncError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_room_detail(&self, roomid: String, key: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?;
        url = url.join(&format!("state/{}", key))?;
        url = url.join(&format!("?access_token={}", tk))?;

        let tx = self.tx.clone();
        let keys = key.clone();
        get!(&url,
            |r: JsonValue| {
                let mut value = String::from("");
                let k = keys.split('.').last().unwrap();

                match r[&k].as_str() {
                    Some(x) => { value = String::from(x); },
                    None => {}
                }
                tx.send(BKResponse::RoomDetail(key, value)).unwrap();
            },
            |err| { tx.send(BKResponse::RoomDetailError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_room_avatar(&self, roomid: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let userid = self.data.lock().unwrap().user_id.clone();
        let roomu = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid.clone() + "/"))?;
        let mut url = roomu.join("state/m.room.avatar")?;
        url = url.join(&format!("?access_token={}", tk))?;

        let tx = self.tx.clone();
        get!(&url,
            |r: JsonValue| {
                let avatar;

                match r["url"].as_str() {
                    Some(u) => {
                        avatar = thumb!(&baseu, u).unwrap_or(String::from(""));
                    },
                    None => {
                        avatar = get_room_avatar(&baseu, &tk, &userid, &roomid).unwrap_or(String::from(""));
                    }
                }
                tx.send(BKResponse::RoomAvatar(avatar)).unwrap();
            },
            |err| { tx.send(BKResponse::RoomAvatarError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_room_messages(&self, roomid: String, to: bool) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid.clone() + "/"))?.join("messages")?;

        let mut params = format!("?access_token={}&dir=b&limit=8", tk);

        if to {
            let msg_batch = self.data.lock().unwrap().msgs_batch_end.clone();
            params = params + &format!("&from={}", msg_batch);
        }
        url = url.join(&params)?;


        let tx = self.tx.clone();
        let data = self.data.clone();
        get!(&url,
            |r: JsonValue| {
                let mut ms: Vec<Message> = vec![];

                data.lock().unwrap().msgs_batch_start = String::from(r["start"].as_str().unwrap_or(""));
                data.lock().unwrap().msgs_batch_end = String::from(r["end"].as_str().unwrap_or(""));

                for msg in r["chunk"].as_array().unwrap().iter().rev() {
                    if msg["type"].as_str().unwrap_or("") != "m.room.message" {
                        continue;
                    }

                    let m = parse_room_message(&baseu, roomid.clone(), msg);
                    ms.push(m);
                }
                match to {
                    false => tx.send(BKResponse::RoomMessages(ms)).unwrap(),
                    true => tx.send(BKResponse::RoomMessagesTo(ms)).unwrap(),
                };
            },
            |err| { tx.send(BKResponse::RoomMessagesError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_room_members(&self, roomid: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?.join("members")?;
        url = url.join(&format!("?access_token={}", tk))?;

        let tx = self.tx.clone();
        get!(&url,
            |r: JsonValue| {
                //println!("{:#?}", r);
                let mut ms: Vec<Member> = vec![];
                for member in r["chunk"].as_array().unwrap().iter().rev() {
                    if member["type"].as_str().unwrap() != "m.room.member" {
                        continue;
                    }

                    let content = &member["content"];
                    if content["membership"].as_str().unwrap() != "join" {
                        continue;
                    }

                    let m = Member {
                        alias: String::from(content["displayname"].as_str().unwrap_or("")),
                        uid: String::from(member["sender"].as_str().unwrap()),
                        avatar: String::from(content["avatar_url"].as_str().unwrap_or("")),
                    };
                    ms.push(m);
                }
                tx.send(BKResponse::RoomMembers(ms)).unwrap();
            },
            |err| { tx.send(BKResponse::RoomMembersError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_base_url(&self) -> Result<Url, Error> {
        let s = self.data.lock().unwrap().server_url.clone();
        let url = Url::parse(&s)?;
        Ok(url)
    }

    pub fn get_avatar_async(&self, uid: &str, tx: Sender<String>) -> Result<(), Error> {
        let baseu = self.get_base_url()?;

        let u = String::from(uid);
        thread::spawn(move || {
            match get_user_avatar(&baseu, &u) {
                Ok(fname) => { tx.send(fname).unwrap(); },
                Err(_) => { tx.send(String::from("")).unwrap(); }
            };
        });

        Ok(())
    }

    pub fn get_thumb_async(&self, media: String, tx: Sender<String>) -> Result<(), Error> {
        let baseu = self.get_base_url()?;

        thread::spawn(move || {
            match thumb!(&baseu, &media) {
                Ok(fname) => { tx.send(fname).unwrap(); },
                Err(_) => { tx.send(String::from("")).unwrap(); }
            };
        });

        Ok(())
    }

    pub fn send_msg(&self, roomid: String, msg: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let msgid;

        {
            let mut data = self.data.lock().unwrap();
            data.msgid = data.msgid + 1;
            msgid = data.msgid;
        }

        let mut url = baseu.join("/_matrix/client/r0/rooms/")?;
        url = url.join(&(roomid + "/"))?.join("send/m.room.message/")?;
        url = url.join(&format!("{}", msgid))?;
        url = url.join(&format!("?access_token={}", tk))?;

        let attrs = json!({
            "body": msg,
            "msgtype": "m.text"
        });

        let tx = self.tx.clone();
        query!("put", &url, &attrs,
            move |_| {
                tx.send(BKResponse::SendMsg).unwrap();
            },
            |err| { tx.send(BKResponse::SendMsgError(err)).unwrap(); }
        );

        Ok(())
    }

    pub fn protocols(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/unstable/thirdparty/protocols")?;
        let params = format!("?access_token={}", tk);
        url = url.join(&params)?;

        let tx = self.tx.clone();
        let s = self.data.lock().unwrap().server_url.clone();
        query!("get", &url,
            move |r: JsonValue| {
                let mut protocols: Vec<Protocol> = vec![];

                protocols.push(Protocol {
                    id: String::from(""),
                    desc: String::from(s.split('/').last().unwrap_or("")),
                });

                let prs = r.as_object().unwrap();
                for k in prs.keys() {
                    let ins = prs[k]["instances"].as_array().unwrap();
                    for i in ins {
                        let p = Protocol{
                            id: String::from(i["instance_id"].as_str().unwrap()),
                            desc: String::from(i["desc"].as_str().unwrap()),
                        };
                        protocols.push(p);
                    }
                }

                tx.send(BKResponse::DirectoryProtocols(protocols)).unwrap();
            },
            |err| { tx.send(BKResponse::DirectoryError(err)).unwrap(); }
        );

        Ok(())
    }

    pub fn room_search(&self, query: Option<String>, third_party: Option<String>, more: bool) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/publicRooms")?;
        let params = format!("?access_token={}", tk);
        url = url.join(&params)?;

        let mut attrs = json!({"limit": 20});

        if let Some(q) = query {
            attrs["filter"] = json!({
                "generic_search_term": q
            });
        }

        if let Some(tp) = third_party {
            attrs["third_party_instance_id"] = json!(tp);
        }

        if more {
            let since = self.data.lock().unwrap().rooms_since.clone();
            attrs["since"] = json!(since);
        }

        let tx = self.tx.clone();
        let data = self.data.clone();
        query!("post", &url, &attrs,
            move |r: JsonValue| {
                data.lock().unwrap().rooms_since = String::from(r["next_batch"].as_str().unwrap_or(""));

                let mut rooms: Vec<Room> = vec![];
                for room in r["chunk"].as_array().unwrap() {
                    let alias = String::from(room["canonical_alias"].as_str().unwrap_or(""));
                    let r = Room {
                        alias: alias,
                        id: String::from(room["room_id"].as_str().unwrap_or("")),
                        avatar: String::from(room["avatar_url"].as_str().unwrap_or("")),
                        name: String::from(room["name"].as_str().unwrap_or("")),
                        topic: String::from(room["topic"].as_str().unwrap_or("")),
                        members: room["num_joined_members"].as_i64().unwrap_or(0) as i32,
                        world_readable: room["world_readable"].as_bool().unwrap_or(false),
                        guest_can_join: room["guest_can_join"].as_bool().unwrap_or(false),
                    };
                    rooms.push(r);
                }

                tx.send(BKResponse::DirectorySearch(rooms)).unwrap();
            },
            |err| { tx.send(BKResponse::DirectoryError(err)).unwrap(); }
        );

        Ok(())
    }
}
