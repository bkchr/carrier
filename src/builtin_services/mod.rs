use peer::PeerBuilder;

mod lifeline;
pub use self::lifeline::Lifeline;

/// Registers the builtin services at the given `PeerBuilder`.
pub fn register(builder: PeerBuilder) -> PeerBuilder {
    builder.register_service(lifeline::Lifeline {})
}
