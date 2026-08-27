//! Client networking. Quinn types stay in this module tree.
//!
//! The winit/render thread never blocks on async IO. Commands use bounded
//! `tokio::sync::mpsc` (`try_send` from UI, `recv` on Tokio). Events use a
//! second bounded mpsc (`try_send` from net, `try_recv` from UI).

mod cert;
mod config;
mod runtime;
mod state;

pub use config::ClientEndpointConfig;
pub use runtime::NetworkHandle;
pub use state::NetworkSnapshot;
