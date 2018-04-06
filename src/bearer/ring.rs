use error::*;
use peer_proof::{verify_proof_der, Proof};

use std::net::SocketAddr;

use hole_punch::PubKey;

use redis;

/// The `Ring` of `Bearer`s.
pub struct Ring {
    con: redis::Connection,
    bearer_addr: SocketAddr,
}

fn get_field_names(pub_key: &PubKey) -> (String, String, String) {
    let pub_key_str = format!("{}", pub_key);

    (
        format!("{}_proof", pub_key_str),
        format!("{}_pubkey", pub_key_str),
        format!("{}_bearer", pub_key_str),
    )
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

    /// Broadcast that the `Bearer` has a new connection.
    pub fn broadcast_new_connection(&mut self, pub_key: &PubKey, proof: Proof) {
        let orig_pub_key = pub_key
            .orig_public_key_der()
            .expect("We do not accept connections without full public keys");

        let (proof_field, pubkey_field, bearer_field) = get_field_names(pub_key);

        if let Err(e) = redis::pipe()
            .cmd("SET")
            .arg(proof_field)
            .arg(&*proof)
            .cmd("SET")
            .arg(pubkey_field)
            .arg(orig_pub_key)
            .cmd("SET")
            .arg(bearer_field)
            .arg(format!("{}", self.bearer_addr))
            .query::<()>(&self.con)
        {
            eprintln!("Ring broadcasting error: {:?}", e);
        }
    }

    /// Try to find a connection in the Carrier Ring.
    /// Returns the address of the `Bearer` that holds the connection.
    pub fn find_connection(&self, pub_key: &PubKey) -> Option<SocketAddr> {
        let (proof_field, pubkey_field, bearer_field) = get_field_names(pub_key);

        let (proof, orig_pub_key, bearer_addr): (Vec<u8>, Vec<u8>, String) = match redis::pipe()
            .cmd("GET")
            .arg(proof_field)
            .cmd("GET")
            .arg(pubkey_field)
            .cmd("GET")
            .arg(bearer_field)
            .query(&self.con)
        {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Ring find_connection error: {:?}", e);
                return None;
            }
        };

        let bearer_addr: SocketAddr = match bearer_addr.parse() {
            Ok(res) => res,
            Err(_) => {
                eprintln!("Error parsing bearer address: {}", bearer_addr);
                return None;
            }
        };

        let verified = match verify_proof_der(&orig_pub_key, &bearer_addr, &proof.into()) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Error at verifying client proof: {:?}", e);
                return None;
            }
        };

        if verified {
            Some(bearer_addr)
        } else {
            //TODO: Could be a hostile bearer?
            eprintln!("Client proof could not be verified!");
            None
        }
    }
}
