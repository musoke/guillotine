extern crate url;
extern crate reqwest;

use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;
use self::url::Url;
use std::sync::mpsc::Sender;

// TODO: send errors to the frontend

macro_rules! get {
    ($url: expr, $attrs: expr, $resp: ident, $okcb: expr) => {
        query!(get, $url, $attrs, $resp, $okcb, |err| {
                println!("ERROR {:?}", err);
            });
    };
}

macro_rules! post {
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

impl From<url::ParseError> for Error {
    fn from(_: url::ParseError) -> Error {
        Error::BackendError
    }
}

pub struct BackendData {
    user_id: String,
    access_token: String,
    server_url: String,
}

pub struct Backend {
    tx: Sender<BKResponse>,
    data: Arc<Mutex<BackendData>>,
}

#[derive(Debug)]
pub enum BKResponse {
    Token(String, String),
    Name(String),
}

#[derive(Deserialize)]
#[derive(Debug)]
pub struct Response {
    user_id: String,
    access_token: String,
}

#[derive(Deserialize)]
#[derive(Debug)]
pub struct DisplayNameResponse {
    displayname: String,
}

impl Backend {
    pub fn new(tx: Sender<BKResponse>) -> Backend {
        let data = BackendData {
                    user_id: String::from("Guest"),
                    access_token: String::from(""),
                    server_url: String::from("https://matrix.org"),
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
        post!(url, map, Response,
            |r: Response| {
                let uid = r.user_id.clone();
                let tk = r.access_token.clone();
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
        post!(url, map, Response,
            |r: Response| {
                let uid = r.user_id.clone();
                let tk = r.access_token.clone();
                data.lock().unwrap().user_id = uid.clone();
                data.lock().unwrap().access_token = tk.clone();
                tx.send(BKResponse::Token(uid, tk)).unwrap();
            }
        );

        Ok(())
    }

    pub fn get_username(&self) -> Result<(), Error> {
        let s = self.data.lock().unwrap().server_url.clone();
        let id = self.data.lock().unwrap().user_id.clone() + "/";
        let url = Url::parse(&s)?.join("/_matrix/client/r0/profile/")?.join(&id)?.join("displayname")?;
        let map: HashMap<String, String> = HashMap::new();

        let tx = self.tx.clone();
        get!(url, map, DisplayNameResponse,
            |r: DisplayNameResponse| {
                tx.send(BKResponse::Name(r.displayname.clone())).unwrap();
            }
        );

        Ok(())
    }
}
