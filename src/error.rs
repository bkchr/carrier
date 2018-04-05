use failure;
pub use failure::ResultExt;

use hole_punch;

use std::io;
use std::result;

use openssl;

use redis;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "hole_punch Error {}", _0)]
    HolePunch(#[cause] hole_punch::Error),
    #[fail(display = "IO Error {}", _0)]
    IoError(#[cause] io::Error),
    #[fail(display = "Openssl Error {}", _0)]
    OpenSslError(#[cause] openssl::error::ErrorStack),
    #[fail(display = "Error {}", _0)]
    Custom(failure::Error),
    #[fail(display = "Redis Error {}", _0)]
    Redis(redis::RedisError),
}

impl From<hole_punch::Error> for Error {
    fn from(err: hole_punch::Error) -> Error {
        Error::HolePunch(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Error {
        Error::Custom(err)
    }
}

impl From<openssl::error::ErrorStack> for Error {
    fn from(err: openssl::error::ErrorStack) -> Error {
        Error::OpenSslError(err)
    }
}

impl From<redis::RedisError> for Error {
    fn from(err: redis::RedisError) -> Error {
        Error::Redis(err)
    }
}

//FIXME: Remove when upstream provides a better bail macro
macro_rules! bail {
    ($e:expr) => {
        return Err(::failure::err_msg::<&'static str>($e).into());
    };
    ($fmt:expr, $($arg:tt)+) => {
        return Err(::failure::err_msg::<String>(format!($fmt, $($arg)+)).into());
    };
}
