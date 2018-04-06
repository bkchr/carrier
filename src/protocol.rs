use peer_proof::Proof;

use hole_punch::{ConnectionId, PubKeyHash};

/// The carrier protocol that is used to communicate between the peers and the peers and
/// the bearers.
#[derive(Deserialize, Serialize, Clone)]
pub enum Protocol {
    /// Hello, I'm a peer.
    Hello { proof: Proof },
    /// An error occurred.
    Error { msg: String },
    /// Connect to this peer to the given peer.
    ConnectToPeer {
        pub_key: PubKeyHash,
        connection_id: ConnectionId,
    },
    /// The requested peer could not be found.
    PeerNotFound,
    /// Request a connection to the given service.
    /// If the service is available on the peer, a `ServiceConnectionEstablished` will be send and
    /// the connection will be forwarded to the service. The connection is afterwards only usable
    /// by the service.
    RequestService { name: String },
    /// The requested service could not be found on the peer.
    ServiceNotFound,
    /// The requested service is available on the peer and any further messages will be routed
    /// to this service. The peer that receives this message, should start the service client
    /// with the current connection.
    ServiceConnectionEstablished,
}
