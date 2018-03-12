use hole_punch::{ConnectionId, PubKey};

#[derive(Deserialize, Serialize, Clone)]
pub enum Protocol {
    Hello,
    Error(String),
    ConnectToPeer {
        pub_key: PubKey,
        connection_id: ConnectionId,
    },
    PeerNotFound,
    AlreadyConnected,
    RequestService {
        name: String,
    },
    ServiceNotFound,
    ServiceConBuild,
}
