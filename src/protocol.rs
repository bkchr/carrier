use hole_punch::ConnectionId;

#[derive(Deserialize, Serialize, Clone)]
pub enum Protocol {
    Register {
        name: String,
    },
    RegisterSuccessFul,
    Login {
        name: String,
        password: String,
    },
    LoginSuccessful,
    LoginFailure(String),
    ConnectToPeer {
        name: String,
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
