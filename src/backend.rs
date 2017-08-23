extern crate url;
extern crate reqwest;
extern crate regex;
extern crate xdg;
extern crate serde_json;
extern crate chrono;
extern crate time;

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

use self::chrono::prelude::*;
use self::time::Duration;

macro_rules! get {
    ($url: expr, $attrs: expr, $okcb: expr, $errcb: expr) => {
        query!("get", $url, $attrs, $okcb, $errcb)
    };
    ($url: expr, $okcb: expr, $errcb: expr) => {
        query!("get", $url, $okcb, $errcb)
    };
}

macro_rules! post {
    ($url: expr, $attrs: expr, $okcb: expr, $errcb: expr) => {
        query!("post", $url, $attrs, $okcb, $errcb)
    };
    ($url: expr, $okcb: expr, $errcb: expr) => {
        query!("post", $url, $okcb, $errcb)
    };
}

macro_rules! query {
    ($method: expr, $url: expr, $attrs: expr, $okcb: expr, $errcb: expr) => {
        thread::spawn(move || {
            let js = json_q($method, $url, $attrs);

            match js {
                Ok(r) => {
                    $okcb(r)
                },
                Err(err) => {
                    $errcb(err)
                }
            }
        });
    };
    ($method: expr, $url: expr, $okcb: expr, $errcb: expr) => {
        let attrs: HashMap<String, String> = HashMap::new();
        query!($method, $url, &attrs, $okcb, $errcb)
    };
}

#[allow(unused_macros)]
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
    RoomMessages(Vec<Message>),
    RoomMembers(Vec<Member>),
    RoomMemberAvatar(String, String),

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
    RoomMemberAvatarError(Error),
}

#[derive(Debug)]
pub struct Message {
    pub sender: String,
    pub mtype: String,
    pub body: String,
    pub date: DateTime<Local>,
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

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(&url,
            |r: JsonValue| {
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));
                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
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

        let mut map: HashMap<String, String> = HashMap::new();
        map.insert(String::from("type"), String::from("m.login.password"));
        map.insert(String::from("user"), user);
        map.insert(String::from("password"), password);

        let data = self.data.clone();
        let tx = self.tx.clone();
        post!(&url, &map,
            |r: JsonValue| {
                let uid = String::from(r["user_id"].as_str().unwrap_or(""));
                let tk = String::from(r["access_token"].as_str().unwrap_or(""));

                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            },
            |err| { tx.send(BKResponse::LoginError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_username(&self) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let id = self.data.lock().unwrap().user_id.clone() + "/";
        let url = baseu.join("/_matrix/client/r0/profile/")?.join(&id)?.join("displayname")?;

        let tx = self.tx.clone();
        get!(&url,
            |r: JsonValue| {
                let name = String::from(r["displayname"].as_str().unwrap_or(""));
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
                    // TODO: treat all events
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
        let roomu = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?;
        let mut url = roomu.join("state/m.room.avatar")?;
        url = url.join(&format!("?access_token={}", tk))?;

        let tx = self.tx.clone();
        get!(&url,
            |r: JsonValue| {
                let mut avatar = String::from("");

                match r["url"].as_str() {
                    Some(u) => {
                        avatar = thumb!(&baseu, u).unwrap();
                    },
                    None => {
                        // TODO: use identicon API
                        // /_matrix/media/v1/identicon/$ident
                    }
                }
                tx.send(BKResponse::RoomAvatar(avatar)).unwrap();
            },
            |err| { tx.send(BKResponse::RoomAvatarError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_room_messages(&self, roomid: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;
        let tk = self.data.lock().unwrap().access_token.clone();
        let mut url = baseu.join("/_matrix/client/r0/rooms/")?.join(&(roomid + "/"))?.join("messages")?;
        url = url.join(&format!("?access_token={}&dir=b&limit=40", tk))?;

        let tx = self.tx.clone();
        get!(&url,
            |r: JsonValue| {
                let mut ms: Vec<Message> = vec![];
                for msg in r["chunk"].as_array().unwrap().iter().rev() {
                    //println!("messages: {:#?}", msg);
                    let age = msg["age"].as_i64().unwrap_or(0);

                    let m = Message {
                        sender: String::from(msg["sender"].as_str().unwrap()),
                        mtype: String::from(msg["content"]["msgtype"].as_str().unwrap()),
                        body: String::from(msg["content"]["body"].as_str().unwrap()),
                        date: age_to_datetime(age),
                    };
                    ms.push(m);
                }
                tx.send(BKResponse::RoomMessages(ms)).unwrap();
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
                    if ms.len() > 20 {
                        tx.send(BKResponse::RoomMembers(ms)).unwrap();
                        ms = vec![];
                    }
                }
                if !ms.is_empty() {
                    tx.send(BKResponse::RoomMembers(ms)).unwrap();
                }
            },
            |err| { tx.send(BKResponse::RoomMembersError(err)).unwrap() }
        );

        Ok(())
    }

    pub fn get_member_avatar(&self, memberid: String, avatar_url: String) -> Result<(), Error> {
        let baseu = self.get_base_url()?;

        let tx = self.tx.clone();
        thread::spawn(move || {
            match thumb!(&baseu, &avatar_url) {
                Ok(fname) => {
                    tx.send(BKResponse::RoomMemberAvatar(memberid, fname)).unwrap();
                },
                Err(err) => {
                    tx.send(BKResponse::RoomMemberAvatarError(err)).unwrap();
                }
            }
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
            let fname = thumb!(&base, &u).unwrap();
            tx.send(fname).unwrap();
        });

        Ok(())
    }
}

fn get_rooms_from_json(r: JsonValue, userid: &str) -> Result<HashMap<String, String>, Error> {
    let rooms = &r["rooms"];
    // TODO: do something with invite and leave
    //let invite = rooms["invite"].as_object().ok_or(Error::BackendError)?;
    //let leave = rooms["leave"].as_object().ok_or(Error::BackendError)?;

    let join = rooms["join"].as_object().ok_or(Error::BackendError)?;

    let mut rooms_map: HashMap<String, String> = HashMap::new();
    for k in join.keys() {
        let room = join.get(k).ok_or(Error::BackendError)?;
        let name = calculate_room_name(&room["state"]["events"], userid)?;
        rooms_map.insert(k.clone(), name);
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

fn dw_media(base: &Url, url: &str, thumb: bool, dest: Option<&str>, w: i32, h: i32) -> Result<String, Error> {
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

fn age_to_datetime(age: i64) -> DateTime<Local> {
    let now = Local::now();
    let diff = Duration::seconds(age / 1000);
    now - diff
}

fn json_q(method: &str, url: &Url, attrs: &HashMap<String, String>) -> Result<JsonValue, Error> {
    let client = reqwest::Client::new()?;

    let mut conn = match method {
        "post" => client.post(url.as_str())?,
        "put" => client.put(url.as_str())?,
        "delete" => client.delete(url.as_str())?,
        _ => client.get(url.as_str())?,
    };

    let conn2 = conn.json(&attrs)?;
    let mut res = conn2.send()?;

    //let mut content = String::new();
    //res.read_to_string(&mut content);
    //cb(content);

    match res.json() {
        Ok(js) => Ok(js),
        Err(_) => Err(Error::BackendError),
    }
}

pub fn get_user_avatar(baseu: &Url, userid: &str) -> Result<String, Error> {
    let id = format!("{}/", userid);
    let url = baseu.join("/_matrix/client/r0/profile/")?.join(&id)?.join("avatar_url")?;
    let attrs: HashMap<String, String> = HashMap::new();

    match json_q("get", &url, &attrs) {
        Ok(js) => {
            let url = String::from(js["avatar_url"].as_str().unwrap_or(""));
            let fname = thumb!(baseu, &url)?;
            Ok(fname)
        },
        Err(_) => { Err(Error::BackendError) }
    }
}

fn get_room_st(base: &Url, tk: &str, roomid: &str) -> Result<JsonValue, Error> {
    let mut url = base.join("/_matrix/client/r0/rooms/")?
        .join(&(format!("{}/state", roomid)))?;
    url = url.join(&format!("?access_token={}", tk))?;
    let attrs: HashMap<String, String> = HashMap::new();
    let st = json_q("get", &url, &attrs)?;
    Ok(st)
}

fn get_room_avatar(base: &Url, tk: &str, userid: &str, roomid: &str) -> Result<String, Error> {
    Ok(String::from("TODO"))
}


fn get_room_name(base: &Url, tk: &str, userid: &str, roomid: &str) -> Result<String, Error> {
    let st = get_room_st(base, tk, roomid)?;
    let rname = calculate_room_name(&st, userid)?;
    Ok(rname)
}

fn calculate_room_name(roomst: &JsonValue, userid: &str) -> Result<String, Error> {

    // looking for "m.room.name" event
    let events = roomst.as_array().ok_or(Error::BackendError)?;
    if let Some(name) = events.iter().find(|x| x["type"] == "m.room.name") {
        return Ok(String::from(name["content"]["name"].as_str().unwrap_or("WRONG NAME")))
    }
    // looking for "m.room.canonical_alias" event
    if let Some(name) = events.iter().find(|x| x["type"] == "m.room.canonical_alias") {
        return Ok(String::from(name["content"]["alias"].as_str().unwrap_or("WRONG ALIAS")))
    }

    // we look for members that aren't me
    let mut members = events.iter()
        .filter(|x| {
            (x["type"] == "m.room.member" &&
             x["content"]["membership"] == "join" &&
             x["sender"] != userid)
        });

    let mut members2 = events.iter()
        .filter(|x| {
            (x["type"] == "m.room.member" &&
             x["content"]["membership"] == "join" &&
             x["sender"] != userid)
        });

    let m1 = match members2.nth(0) {
        Some(m) => m["content"]["displayname"].as_str().unwrap_or(""),
        None => ""
    };
    let m2 = match members2.nth(1) {
        Some(m) => m["content"]["displayname"].as_str().unwrap_or(""),
        None => ""
    };

    let name = match members.count() {
        0 => String::from("EMPTY ROOM"),
        1 => String::from(m1),
        2 => format!("{} and {}", m1, m2),
        _ => format!("{} and Others", m1)
    };

    Ok(name)
}
