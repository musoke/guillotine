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
    pub id: String,
}

impl Clone for Message {
    fn clone(&self) -> Message {
        Message {
            sender: self.sender.clone(),
            mtype: self.mtype.clone(),
            body: self.body.clone(),
            date: self.date.clone(),
            room: self.room.clone(),
            thumb: self.thumb.clone(),
            url: self.url.clone(),
            id: self.id.clone(),
        }
    }
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
    pub topic: String,
    pub alias: String,
    pub guest_can_join: bool,
    pub world_readable: bool,
    pub members: i32,
    pub notifications: i32,
}

impl Room {
    pub fn new(id: String, name: String) -> Room {
        Room {
            id: id,
            name: name,
            avatar: String::new(),
            topic: String::new(),
            alias: String::new(),
            guest_can_join: true,
            world_readable: true,
            members: 0,
            notifications: 0,
        }
    }
}

impl Clone for Room {
    fn clone(&self) -> Room {
        Room {
            id: self.id.clone(),
            name: self.name.clone(),
            avatar: self.avatar.clone(),
            topic: self.topic.clone(),
            alias: self.alias.clone(),
            guest_can_join: self.guest_can_join,
            world_readable: self.world_readable,
            members: self.members,
            notifications: self.notifications,
        }
    }
}
