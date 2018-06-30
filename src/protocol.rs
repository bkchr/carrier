use service::ServiceId;

/// The carrier protocol that is used to communicate between the peers.
#[derive(Deserialize, Serialize, Clone)]
pub enum Protocol {
    /// Request to start a the given service on the peer.
    /// If the service is available on the peer, a `ServiceStarted` will be send. The stream is
    /// afterwards only usable by the service. If the service is not available, a `ServiceNotFound`
    /// will be send.
    RequestServiceStart { name: String, local_id: ServiceId },
    /// The requested service could not be found on the peer.
    ServiceNotFound,
    /// The requested Service was started on the peer with the given id.
    ServiceStarted { id: ServiceId },
    /// Connect a stream to the given service instance. Will response with `ServiceNotFound`, when
    /// a service with the given id is not available or with `ServiceConnected` when the given
    /// service instance could be found.
    ConnectToService { id: ServiceId },
    /// The stream could be connected to the given service.
    ServiceConnected,
}
