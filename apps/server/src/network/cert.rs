//! Development-only self-signed certificate. Not production PKI.

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// Generate an in-memory self-signed cert for local QUIC.
///
/// Named DevOnly so this cannot be mistaken for a production trust path.
pub fn generate_dev_only_self_signed()
-> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let certified =
        rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
            .map_err(|err| format!("dev-only cert: {err}"))?;
    let cert_der = CertificateDer::from(certified.cert.der().to_vec());
    let key_der =
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
    Ok((cert_der, key_der))
}
