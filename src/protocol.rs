#[derive(Deserialize, Serialize, Clone)]
enum CarrierProtocol {
    Login {
        name: String,
    },
    LoginSucessfull,
    LoginFailure(String),
    ConnectToPeer {
        name: String,
        connection_id: ConnectionId,
    },
    PeerNotFound,
    AlreadyConnected,
}
