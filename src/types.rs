extern crate chrono;

use self::chrono::prelude::*;

#[derive(Debug)]
pub struct Message {
    pub sender: String,
    pub mtype: String,
    pub body: String,
    pub date: DateTime<Local>,
    pub room: String,
    pub thumb: String,
    pub url: String,
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

#[derive(Debug)]
pub struct Protocol {
    pub id: String,
    pub desc: String,
}

#[derive(Debug)]
pub struct Room {
    pub id: String,
    pub avatar: String,
    pub name: String,
    pub guest_can_join: bool,
    pub topic: String,
    pub members: i32,
    pub world_readable: bool,
    pub alias: String,
}
