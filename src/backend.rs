extern crate url;
extern crate reqwest;
extern crate regex;
extern crate xdg;
extern crate serde_json;

use self::regex::Regex;

use self::serde_json::Value as JsonValue;

use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;
use self::url::Url;
use std::sync::mpsc::Sender;
use std::io::Read;

use std::fs::File;
use std::io::prelude::*;
use std::io;

// TODO: send errors to the frontend

macro_rules! get {
    ($url: expr, $attrs: expr, $okcb: expr) => {
        query!(get, $url, $attrs, JsonValue, $okcb, |err| {
                println!("ERROR {:?}", err);
            });
    };

    ($url: expr, $attrs: expr, $resp: ident, $okcb: expr) => {
        query!(get, $url, $attrs, $resp, $okcb, |err| {
                println!("ERROR {:?}", err);
            });
    };
}

macro_rules! post {
    ($url: expr, $attrs: expr, $okcb: expr) => {
        query!(post, $url, $attrs, JsonValue, $okcb, |err| {
                println!("ERROR {:?}", err);
            });
    };

    ($url: expr, $attrs: expr, $resp: ident, $okcb: expr) => {
        query!(post, $url, $attrs, $resp, $okcb, |err| {
                println!("ERROR {:?}", err);
            });
    };
}

macro_rules! query {
    ($method: ident, $url: expr, $attrs: expr, $resp: ident, $okcb: expr, $errcb: expr) => {
        // TODO: remove unwrap and manage errors
        thread::spawn(move || {
            let client = reqwest::Client::new().unwrap();
            let mut conn = client.$method($url.as_str()).unwrap();
            let conn2 = conn.json(&$attrs).unwrap();
            let mut res = conn2.send().unwrap();

            let js: Result<$resp, _> = res.json();

            match js {
                Ok(r) => {
                    $okcb(r)
                },
                Err(err) => {
                    $errcb(err)
                }
            }
            //let mut content = String::new();
            //res.read_to_string(&mut content);
            //cb(content);
        });
    };
}

macro_rules! media {
    ($base: expr, $url: expr, $dest: expr) => {
        dw_media($base, $url, false, $dest, 0, 0)
    };
    ($base: expr, $url: expr) => {
        dw_media($base, $url, false, None, 0, 0)
    };
}

macro_rules! thumb {
    ($base: expr, $url: expr) => {
        dw_media($base, $url, true, None, 64, 64)
    };
    ($base: expr, $url: expr, $size: expr) => {
        dw_media($base, $url, true, None, $size, $size)
    };
    ($base: expr, $url: expr, $w: expr, $h: expr) => {
        dw_media($base, $url, true, None, $w, $h)
    };
}

#[derive(Debug)]
pub enum Error {
    BackendError,
    ReqwestError(reqwest::Error),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Error {
        Error::ReqwestError(err)
    }
}

derror!(url::ParseError, Error::BackendError);
derror!(io::Error, Error::BackendError);
derror!(regex::Error, Error::BackendError);

pub struct BackendData {
    user_id: String,
    access_token: String,
    server_url: String,
    since: String,
}

pub struct Backend {
    tx: Sender<BKResponse>,
    data: Arc<Mutex<BackendData>>,
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
    RoomMessage(Message),
    RoomMessages(Vec<Message>),
    RoomMember(Member),
    RoomMembers(Vec<Member>),
    RoomMemberAvatar(String, String),
}

#[derive(Debug)]
pub struct Message {
    /// the sender
    pub s: String,
    /// the message type
    pub t: String,
    /// the message body
    pub b: String,
    /// the message age
    pub a: i64,
}

#[derive(Debug)]
pub struct Member {
    pub alias: String,
    pub uid: String,
    pub avatar: String,
}

impl Member {
    pub fn get_alias(&self) -> String {
        match self.alias {
            ref a if a.is_empty() => self.uid.clone(),
            ref a => a.clone(),
        }
    }
}


impl Backend {
    pub fn new(tx: Sender<BKResponse>) -> Backend {
        let data = BackendData {
                    user_id: String::from("Guest"),
                    access_token: String::from(""),
                    server_url: String::from("https://matrix.org"),
                    since: String::from(""),
        };
        Backend { tx: tx, data: Arc::new(Mutex::new(data)) }
    }

    pub fn guest(&self, server: String) -> Result<(), Error> {
        let s = server.clone();
        let url = Url::parse(&s).unwrap().join("/_matrix/client/r0/register?kind=guest")?;
        self.data.lock().unwrap().server_url = s;

        let map: HashMap<String, String> = HashMap::new();

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(url, map,
            |r: JsonValue| {
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));
                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            }
        );

        Ok(())
    }

    pub fn login(&self, user: String, password: String, server: String) -> Result<(), Error> {
        let s = server.clone();
        let url = Url::parse(&s)?.join("/_matrix/client/r0/login")?;
        self.data.lock().unwrap().server_url = s;

        let mut map = HashMap::new();
        map.insert("type", String::from("m.login.password"));
        map.insert("user", user);
        map.insert("password", password);

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(url, map,
            |r: JsonValue| {
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));

                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            }
        );

        Ok(())
    }

    pub fn get_username(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let id = self.data.lock().unwrap().user_id.clone() + "/";
        let url = baseu.join("/_matrix/client/r0/profile/")?.join(&id)?.join("displayname")?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        get!(url, map,
            |r: JsonValue| {
                let name = String::from(r["displayname"].as_str().unwrap_or(""));
                tx.send(BKResponse::Name(name)).unwrap();
            }
        );

        Ok(())
    }

    pub fn get_avatar(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let id = self.data.lock().unwrap().user_id.clone() + "/";
        let url = baseu.join("/_matrix/client/r0/profile/")?.join(&id)?.join("avatar_url")?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        get!(url, map,
            |r: JsonValue| {
                let url = String::from(r["avatar_url"].as_str().unwrap_or(""));
                let fname = thumb!(baseu, &url).unwrap();

                tx.send(BKResponse::Avatar(fname)).unwrap();
        });

        Ok(())
    }

    pub fn sync(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let token = self.data.lock().unwrap().access_token.clone();
        let since = self.data.lock().unwrap().since.clone();

        let mut params: String;

        if since.is_empty() {
            params = format!("?full_state=false&timeout=30000&access_token={}", token);
            params = params + "&filter={\
                                          \"room\": {\
                                            \"state\": {\
                                                \"types\": [\"m.room.*\"],\
                                                \"not_types\": [\"m.room.member\"]\
                                            },\
                                            \"timeline\": {\"limit\":0},\
                                            \"ephemeral\": {\"types\": []}\
                                          },\
                                          \"presence\": {\"types\": []}\
                                      }";
        } else {
            params = format!("?full_state=false&timeout=30000&access_token={}&since={}", token, since);
        }

        let url = baseu.join("/_matrix/client/r0/sync")?.join(&params)?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        let data = self.data.clone();
        get!(url, map,
            |r: JsonValue| {
                let next_batch = String::from(r["next_batch"].as_str().unwrap_or(""));
                if since.is_empty() {
                    let rooms = get_rooms_from_json(r).unwrap();
                    tx.send(BKResponse::Rooms(rooms)).unwrap();
                } else {
                    // TODO: treat all events
                }

                data.lock().unwrap().since = next_batch;

                tx.send(BKResponse::Sync).unwrap();
        });

        Ok(())
    }

    pub fn get_room_detail(&self, roomid: String, key: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?;
        url = url.join(&format!("state/{}", key))?;
        url = url.join(&format!("?access_token={}", tk))?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        let keys = key.clone();
        get!(url, map,
            |r: JsonValue| {
                let mut value = String::from("");
                let k = keys.split('.').last().unwrap();

                match r[&k].as_str() {
                    Some(x) => { value = String::from(x); },
                    None => {}
                }
                tx.send(BKResponse::RoomDetail(key, value)).unwrap();
        });

        Ok(())
    }

    pub fn get_room_avatar(&self, roomid: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?.join("state/m.room.avatar")?;
        url = url.join(&format!("?access_token={}", tk))?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        get!(url, map,
            |r: JsonValue| {
                let mut avatar = String::from("");

                match r["url"].as_str() {
                    Some(u) => { avatar = thumb!(baseu.clone(), u).unwrap(); },
                    None => {}
                }
                tx.send(BKResponse::RoomAvatar(avatar)).unwrap();
        });

        Ok(())
    }

    pub fn get_room_messages(&self, roomid: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?.join("messages")?;
        url = url.join(&format!("?access_token={}&dir=b&limit=40", tk))?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        get!(url, map,
            |r: JsonValue| {
                let mut ms: Vec<Message> = vec![];
                for msg in r["chunk"].as_array().unwrap().iter().rev() {
                    //println!("messages: {:#?}", msg);
                    let m = Message {
                        s: String::from(msg["sender"].as_str().unwrap()),
                        t: String::from(msg["content"]["msgtype"].as_str().unwrap()),
                        b: String::from(msg["content"]["body"].as_str().unwrap()),
                        a: msg["age"].as_i64().unwrap(),
                    };
                    ms.push(m);
                }
                tx.send(BKResponse::RoomMessages(ms)).unwrap();
        });

        Ok(())
    }

    pub fn get_room_members(&self, roomid: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?.join("members")?;
        url = url.join(&format!("?access_token={}", tk))?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        get!(url, map,
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
                    if (ms.len() > 20) {
                        tx.send(BKResponse::RoomMembers(ms)).unwrap();
                        ms = vec![];
                    }
                }
                if (!ms.is_empty()) {
                    tx.send(BKResponse::RoomMembers(ms)).unwrap();
                }
        });

        Ok(())
    }

    pub fn get_member_avatar(&self, memberid: String, avatar_url: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;

        let tx = self.tx.clone();
        thread::spawn(move || {
            let fname = thumb!(baseu, &avatar_url).unwrap();
            tx.send(BKResponse::RoomMemberAvatar(memberid, fname)).unwrap();
        });

        Ok(())
    }

    pub fn get_base_url(&self) -> Result<Url, Error> {
        let s = self.data.lock().unwrap().server_url.clone();
        let url = Url::parse(&s)?;
        Ok(url)
    }

    pub fn get_media_async(&self, url: String, tx: Sender<String>) -> Result<(), Error> {
        let base = self.get_base_url()?;

        let u = url.clone();
        thread::spawn(move || {
            let fname = thumb!(base, &u).unwrap();
            tx.send(fname).unwrap();
        });

        Ok(())
    }
}

fn get_rooms_from_json(r: JsonValue) -> Result<HashMap<String, String>, Error> {
    let rooms = &r["rooms"];
    // TODO: do something with invite and leave
    //let invite = rooms["invite"].as_object().ok_or(Error::BackendError)?;
    //let leave = rooms["leave"].as_object().ok_or(Error::BackendError)?;

    let join = rooms["join"].as_object().ok_or(Error::BackendError)?;

    let mut rooms_map: HashMap<String, String> = HashMap::new();
    for k in join.keys() {
        let room = join.get(k).ok_or(Error::BackendError)?;
        let events = room["state"]["events"].as_array().ok_or(Error::BackendError)?;
        let name = events.iter().find(|x| x["type"] == "m.room.name");
        let n = match name {
            None => k.clone(),
            Some(o) => String::from(o["content"]["name"].as_str().ok_or(Error::BackendError)?),
        };
        rooms_map.insert(k.clone(), n);
    }

    Ok(rooms_map)
}

fn get_media(url: &str) -> Result<Vec<u8>, Error> {
    let client = reqwest::Client::new()?;
    let mut conn = client.get(url)?;
    let mut res = conn.send()?;

    let mut buffer = Vec::new();
    res.read_to_end(&mut buffer)?;

    Ok(buffer)
}

fn dw_media(base: Url, url: &str, thumb: bool, dest: Option<&str>, w: i32, h: i32) -> Result<String, Error> {
    // TODO, don't download if exists

    let xdg_dirs = xdg::BaseDirectories::with_prefix("guillotine").unwrap();

    let re = Regex::new(r"mxc://(?P<server>[^/]+)/(?P<media>.+)")?;
    let caps = re.captures(url).ok_or(Error::BackendError)?;
    let server = String::from(&caps["server"]);
    let media = String::from(&caps["media"]);

    let mut url: Url;

    if thumb {
        url = base.join("/_matrix/media/r0/thumbnail/")?;
        url = url.join(&(server + "/"))?;
        let f = format!("?width={}&height={}&method=scale", w, h);
        url = url.join(&(media.clone() + &f))?;
    } else {
        url = base.join("/_matrix/media/r0/download/")?;
        url = url.join(&(server + "/"))?;
        url = url.join(&(media))?;
    }

    let fname = match dest {
        None => String::from(xdg_dirs.place_cache_file(&media)?.to_str().ok_or(Error::BackendError)?),
        Some(d) => String::from(d) + &media
    };

    let mut file = File::create(&fname)?;
    let buffer = get_media(url.as_str())?;
    file.write_all(&buffer)?;

    Ok(fname)
}
