use error::*;
use peer_proof::Proof;

use std::net::SocketAddr;

use hole_punch::PubKey;

use redis;

/// The `Ring` of `Bearer`s.
pub struct Ring {
    con: redis::Connection,
    bearer_addr: SocketAddr,
}

impl Ring {
    pub fn new(redis_addr: SocketAddr, passwd: String, bearer_addr: SocketAddr) -> Result<Ring> {
        let con_info = redis::ConnectionInfo {
            addr: Box::new(redis::ConnectionAddr::Tcp(
                format!("{}", redis_addr.ip()),
                redis_addr.port(),
            )),
            db: 0,
            passwd: Some(passwd),
        };

        let client = redis::Client::open(con_info)?;
        let con = client.get_connection()?;

        Ok(Ring { con, bearer_addr })
    }

    pub fn broadcast_new_connection(&mut self, pub_key: &PubKey, proof: Proof) {
        let pub_key_str = format!("{}", pub_key);
        let orig_pub_key = pub_key
            .orig_public_key_der()
            .expect("We do not accept connections without full public keys");

        if let Err(e) = redis::pipe()
            .cmd("SET")
            .arg(format!("{}_proof", pub_key_str))
            .arg(&*proof)
            .cmd("SET")
            .arg(format!("{}_pubkey", pub_key_str))
            .arg(orig_pub_key)
            .cmd("SET")
            .arg(format!("{}_server", pub_key_str))
            .arg(format!("{}", self.bearer_addr))
            .query::<()>(&self.con)
        {
            eprintln!("Ring broadcasting error: {:?}", e);
        }
    }
}
