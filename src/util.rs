extern crate url;
extern crate reqwest;
extern crate regex;
extern crate xdg;
extern crate serde_json;
extern crate chrono;
extern crate time;
extern crate cairo;

use self::regex::Regex;

use self::serde_json::Value as JsonValue;

use std::collections::HashMap;
use self::url::Url;
use std::io::Read;
use std::path::Path;

use std::fs::File;
use std::io::prelude::*;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use self::chrono::prelude::*;
use self::time::Duration;

use error::Error;
use types::Message;


// from https://stackoverflow.com/a/43992218/1592377
#[macro_export]
macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

#[macro_export]
macro_rules! derror {
    ($from: path, $to: path) => {
        impl From<$from> for Error {
            fn from(_: $from) -> Error {
                $to
            }
        }
    };
}

#[macro_export]
macro_rules! bkerror {
    ($result: ident, $tx: ident, $type: expr) => {
        if let Err(e) = $result {
            $tx.send($type(e)).unwrap();
        }
    }
}

#[macro_export]
macro_rules! get {
    ($url: expr, $attrs: expr, $okcb: expr, $errcb: expr) => {
        query!("get", $url, $attrs, $okcb, $errcb)
    };
    ($url: expr, $okcb: expr, $errcb: expr) => {
        query!("get", $url, $okcb, $errcb)
    };
}

#[macro_export]
macro_rules! post {
    ($url: expr, $attrs: expr, $okcb: expr, $errcb: expr) => {
        query!("post", $url, $attrs, $okcb, $errcb)
    };
    ($url: expr, $okcb: expr, $errcb: expr) => {
        query!("post", $url, $okcb, $errcb)
    };
}

#[macro_export]
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
        let attrs = json!(null);
        query!($method, $url, &attrs, $okcb, $errcb)
    };
}

#[allow(unused_macros)]
#[macro_export]
macro_rules! media {
    ($base: expr, $url: expr, $dest: expr) => {
        dw_media($base, $url, false, $dest, 0, 0)
    };
    ($base: expr, $url: expr) => {
        dw_media($base, $url, false, None, 0, 0)
    };
}

#[macro_export]
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

pub fn get_rooms_from_json(r: JsonValue, userid: &str) -> Result<HashMap<String, String>, Error> {
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

pub fn get_rooms_timeline_from_json(baseu: &Url, r: JsonValue) -> Result<Vec<Message>, Error> {
    let rooms = &r["rooms"];
    let join = rooms["join"].as_object().ok_or(Error::BackendError)?;

    let mut msgs: Vec<Message> = vec![];
    for k in join.keys() {
        let room = join.get(k).ok_or(Error::BackendError)?;
        let timeline = room["timeline"]["events"].as_array();
        if timeline.is_none() {
            return Ok(msgs);
        }

        let events = timeline.unwrap().iter()
                      .filter(|x| x["type"] == "m.room.message");

        for ev in events {
            let msg = parse_room_message(baseu, k.clone(), ev);
            msgs.push(msg);
        }
    }

    Ok(msgs)
}

pub fn get_media(url: &str) -> Result<Vec<u8>, Error> {
    let client = reqwest::Client::new()?;
    let mut conn = client.get(url)?;
    let mut res = conn.send()?;

    let mut buffer = Vec::new();
    res.read_to_end(&mut buffer)?;

    Ok(buffer)
}

pub fn dw_media(base: &Url, url: &str, thumb: bool, dest: Option<&str>, w: i32, h: i32) -> Result<String, Error> {
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

    let pathname = fname.clone();
    let p = Path::new(&pathname);
    if p.is_file() {
        return Ok(fname);
    }

    let mut file = File::create(&fname)?;
    let buffer = get_media(url.as_str())?;
    file.write_all(&buffer)?;

    Ok(fname)
}

pub fn age_to_datetime(age: i64) -> DateTime<Local> {
    let now = Local::now();
    let diff = Duration::seconds(age / 1000);
    now - diff
}

pub fn json_q(method: &str, url: &Url, attrs: &JsonValue) -> Result<JsonValue, Error> {
    let client = reqwest::Client::new()?;

    let mut conn = match method {
        "post" => client.post(url.as_str())?,
        "put" => client.put(url.as_str())?,
        "delete" => client.delete(url.as_str())?,
        _ => client.get(url.as_str())?,
    };

    let conn2 = conn.json(attrs)?;
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
    let url = baseu.join("/_matrix/client/r0/profile/")?.join(userid)?;
    let attrs = json!(null);

    match json_q("get", &url, &attrs) {
        Ok(js) => {
            match js["avatar_url"].as_str() {
                Some(url) => Ok(thumb!(baseu, &url)?),
                None => {
                    let name = js["displayname"].as_str().unwrap_or("@");
                    Ok(draw_identicon(userid, String::from(name))?)
                },
            }
        },
        Err(_) => {
            Ok(draw_identicon(userid, String::from(&userid[1..2]))?)
        }
    }
}

pub fn get_room_st(base: &Url, tk: &str, roomid: &str) -> Result<JsonValue, Error> {
    let mut url = base.join("/_matrix/client/r0/rooms/")?
        .join(&(format!("{}/state", roomid)))?;
    url = url.join(&format!("?access_token={}", tk))?;
    let attrs = json!(null);
    let st = json_q("get", &url, &attrs)?;
    Ok(st)
}

pub fn get_room_avatar(base: &Url, tk: &str, userid: &str, roomid: &str) -> Result<String, Error> {
    let st = get_room_st(base, tk, roomid)?;
    let events = st.as_array().ok_or(Error::BackendError)?;

    // we look for members that aren't me
    let filter = |x: &&JsonValue| {
        (x["type"] == "m.room.member" &&
         x["content"]["membership"] == "join" &&
         x["sender"] != userid)
    };
    let members = events.iter().filter(&filter);
    let mut members2 = events.iter().filter(&filter);

    let m1 = match members2.nth(0) {
        Some(m) => m["content"]["avatar_url"].as_str().unwrap_or(""),
        None => ""
    };

    let mut fname = match members.count() {
        1 => thumb!(&base, m1).unwrap_or(String::new()),
        _ => {String::new()},
    };

    if fname.is_empty() {
        let roomname = calculate_room_name(&st, userid)?;
        fname = draw_identicon(roomid, roomname)?;
    }

    Ok(fname)
}

struct Color {
    r: i32,
    g: i32,
    b: i32,
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

pub fn draw_identicon(fname: &str, name: String) -> Result<String, Error> {
    let colors = vec![
        Color{r:  69, g: 189, b: 243},
        Color{r: 224, g: 143, b: 112},
        Color{r:  77, g: 182, b: 172},
        Color{r: 149, g: 117, b: 205},
        Color{r: 176, g: 133, b:  94},
        Color{r: 240, g:  98, b: 146},
        Color{r: 163, g: 211, b: 108},
        Color{r: 121, g: 134, b: 203},
        Color{r: 241, g: 185, b:  29},
    ];

    let xdg_dirs = xdg::BaseDirectories::with_prefix("guillotine").unwrap();
    let fname = String::from(xdg_dirs.place_cache_file(fname)?.to_str().ok_or(Error::BackendError)?);

    let image = cairo::ImageSurface::create(cairo::Format::ARgb32, 40, 40)?;
    let g = cairo::Context::new(&image);

    let c = &colors[calculate_hash(&fname) as usize % colors.len() as usize];
    g.set_source_rgba(c.r as f64 / 256.,
                      c.g as f64 / 256.,
                      c.b as f64 / 256., 1.);
    g.rectangle(0., 0., 40., 40.);
    g.fill();

    g.set_font_size(24.);
    g.set_source_rgb(1.0, 1.0, 1.0);

    let first = match &name.chars().nth(0) {
        &Some(f) if f == '#' => String::from(&name.to_uppercase()[1..2]),
        &Some(_) => String::from(&name.to_uppercase()[0..1]),
        &None => String::from("X"),
    };

    let te = g.text_extents(&first);
    g.move_to(20. - te.width / 2., 20. + te.height / 2.);
    g.show_text(&first);

    let mut buffer = File::create(&fname)?;
    image.write_to_png(&mut buffer)?;

    Ok(fname)
}

pub fn calculate_room_name(roomst: &JsonValue, userid: &str) -> Result<String, Error> {

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
    let filter = |x: &&JsonValue| {
        (x["type"] == "m.room.member" &&
         x["content"]["membership"] == "join" &&
         x["sender"] != userid)
    };
    let members = events.iter().filter(&filter);
    let mut members2 = events.iter().filter(&filter);

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

pub fn parse_room_message(baseu: &Url, roomid: String, msg: &JsonValue) -> Message {
    let sender = msg["sender"].as_str().unwrap_or("");
    let age = msg["age"].as_i64().unwrap_or(0);

    let c = &msg["content"];
    let mtype = c["msgtype"].as_str().unwrap_or("");
    let body = c["body"].as_str().unwrap_or("");

    let mut url = String::new();
    let mut thumb = String::new();

    match mtype {
        "m.image" => {
            url = String::from(c["url"].as_str().unwrap_or(""));
            let t = c["info"]["thumbnail_url"].as_str().unwrap_or("");
            thumb = media!(baseu, t).unwrap_or(String::from(""));
        },
        _ => {},
    };

    Message {
        sender: String::from(sender),
        mtype: String::from(mtype),
        body: String::from(body),
        date: age_to_datetime(age),
        room: roomid.clone(),
        url: url,
        thumb: thumb,
    }
}

pub fn markup(s: &str) -> String {
    let mut out = String::from(s);

    out = String::from(out.trim());
    out = out.replace('&', "&amp;");
    out = out.replace('<', "&lt;");
    out = out.replace('>', "&gt;");

    let re = Regex::new(r"(?P<url>https?://[^\s]+[-A-Za-z0-9+&@#/%=~_|])").unwrap();
    out = String::from(re.replace_all(&out, "<a href=\"$url\">$url</a>"));

    out
}
