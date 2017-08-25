extern crate cairo;
extern crate url;
extern crate regex;
extern crate reqwest;

use std::io;

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
derror!(cairo::Status, Error::BackendError);
derror!(cairo::IoError, Error::BackendError);
