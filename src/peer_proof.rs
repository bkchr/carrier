use error::*;

use std::mem;
use std::net::{IpAddr, SocketAddr};

use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private, Public};
use openssl::sign::{Signer, Verifier};

const PROOF_SALT: &[u8] = b"CARRIER";

#[derive(Serialize, Deserialize, Clone)]
pub struct Proof {
    data: Vec<u8>,
}

fn u16_to_bytes(data: u16) -> [u8; 2] {
    unsafe { mem::transmute(data) }
}

fn ip_address_to_bytes<F>(ip: &IpAddr, mut bytes: F) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    match *ip {
        IpAddr::V4(ip) => bytes(&ip.octets()),
        IpAddr::V6(ip) => bytes(&ip.octets()),
    }
}

/// Create a proof for a Peer.
/// The proof can be used to verify, that a given Peer is connected to a Bearer.
/// Takes the Peer private key and the server address as input arguments.
pub fn create_proof(pkey: &PKey<Private>, bearer_address: &SocketAddr) -> Result<Proof> {
    let mut signer = Signer::new(MessageDigest::sha256(), pkey)?;

    signer.update(PROOF_SALT)?;
    ip_address_to_bytes(&bearer_address.ip(), |b| Ok(signer.update(b)?))?;
    signer.update(&u16_to_bytes(bearer_address.port()))?;

    Ok(Proof {
        data: signer.sign_to_vec()?,
    })
}

/// Verifies a given proof.
/// Takes the Peer public key, the server address and the that should be verified.
/// Returns `Ok(true)`, if the proof matches.
pub fn verify_proof(
    pkey: &PKey<Public>,
    bearer_address: &SocketAddr,
    proof: &Proof,
) -> Result<bool> {
    let mut verifier = Verifier::new(MessageDigest::sha256(), pkey)?;

    verifier.update(PROOF_SALT)?;
    ip_address_to_bytes(&bearer_address.ip(), |b| Ok(verifier.update(b)?))?;
    verifier.update(&u16_to_bytes(bearer_address.port()))?;

    Ok(verifier.verify(&proof.data)?)
}
