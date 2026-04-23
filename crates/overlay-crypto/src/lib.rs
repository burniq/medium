use chrono::{DateTime, Duration, Utc};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

pub fn issue_bootstrap_code() -> String {
    format!("ovr-{}", uuid::Uuid::new_v4().simple())
}

pub fn issue_node_cert(
    device_id: &str,
    public_key_pem: &str,
) -> anyhow::Result<(String, DateTime<Utc>)> {
    let _ = public_key_pem;
    let mut params = CertificateParams::new(vec![device_id.to_string()])?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, device_id);
    params.distinguished_name = dn;
    let cert = params.self_signed(&KeyPair::generate()?)?;
    let expires_at = Utc::now() + Duration::hours(24);
    Ok((cert.pem(), expires_at))
}
