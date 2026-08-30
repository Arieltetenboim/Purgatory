//! Client networking. Quinn types stay in this module tree.
//!
//! The winit/render thread never blocks on async IO.
//!
//! **Commands:** Connect uses a bounded mpsc (`try_send`, cap 8). Disconnect
//! and Shutdown use a watch control plane (disconnect epoch + shutdown flag)
//! so they cannot be stranded behind Connect pressure. Gameplay `InputCommand`
//! uses a separate bounded mpsc (cap 8). There is no unbounded fallback queue.
//!
//! **Events:** two bounded channels. Lifecycle (Connecting / Handshaking /
//! Connected / Rejected / Disconnected) is reliable (`try_send`, then
//! `send` interruptible by shutdown). Telemetry (`RttUpdated`) is `try_send`
//! and may be dropped.
//!
//! One session task at a time on the `purgatory-net` thread. Extra Connect
//! commands during a session are consumed and ignored (no parallel QUIC).

mod cert;
mod config;
mod diagnostics;
mod failure;
mod runtime;
pub mod state;

pub use config::ClientEndpointConfig;
pub use diagnostics::NETWORK_HISTORY_CAP;
pub use failure::NetworkFailureKind;
pub use purgatory_common::impairment::{
    ImpairmentMetricsSnapshot, ImpairmentProfile, NetworkImpairmentConfig,
};
pub use runtime::NetworkHandle;
pub use state::{NetworkCommand, NetworkSnapshot};
