use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SESSION_TOKEN_TTL_MINUTES: i64 = 10;
pub const SESSION_TOKEN_CLOCK_SKEW_MINUTES: i64 = 5;

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

#[derive(Debug, Clone)]
pub struct ControlTlsIdentity {
    pub cert_pem: String,
    pub key_pem: String,
    pub control_pin: String,
}

#[derive(Debug, Clone)]
pub struct MediumServiceCa {
    pub cert_pem: String,
    pub key_pem: String,
}

#[derive(Debug, Clone)]
pub struct ServiceTlsIdentity {
    pub cert_pem: String,
    pub key_pem: String,
}

pub fn issue_control_tls_identity(
    subject_alt_names: &[String],
) -> anyhow::Result<ControlTlsIdentity> {
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(subject_alt_names.to_vec())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "medium-control");
    params.distinguished_name = dn;
    let cert = params.self_signed(&key_pair)?;
    let digest = Sha256::digest(cert.der().as_ref());

    Ok(ControlTlsIdentity {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        control_pin: format!("sha256:{}", hex_lower(&digest)),
    })
}

pub fn issue_medium_service_ca() -> anyhow::Result<MediumServiceCa> {
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(Vec::new())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Medium Service CA");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    params.key_usages.push(KeyUsagePurpose::CrlSign);
    let cert = params.self_signed(&key_pair)?;

    Ok(MediumServiceCa {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

pub fn issue_service_tls_identity(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    subject_alt_names: &[String],
) -> anyhow::Result<ServiceTlsIdentity> {
    let ca_key_pair = KeyPair::from_pem(ca_key_pem)?;
    let issuer = Issuer::from_ca_cert_pem(ca_cert_pem, ca_key_pair)?;
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(subject_alt_names.to_vec())?;
    let common_name = subject_alt_names
        .first()
        .cloned()
        .unwrap_or_else(|| "service.medium".to_string());
    params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    params.use_authority_key_identifier_extension = true;
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    let cert = params.signed_by(&key_pair, &issuer)?;

    Ok(ServiceTlsIdentity {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTokenClaims {
    pub session_id: String,
    pub service_id: String,
    pub node_id: String,
    pub expires_at: DateTime<Utc>,
}

pub fn issue_session_token(
    shared_secret: &str,
    session_id: &str,
    service_id: &str,
    node_id: &str,
) -> anyhow::Result<String> {
    let claims = SessionTokenClaims {
        session_id: session_id.to_string(),
        service_id: service_id.to_string(),
        node_id: node_id.to_string(),
        expires_at: Utc::now() + Duration::minutes(SESSION_TOKEN_TTL_MINUTES),
    };
    let payload = serde_json::to_vec(&claims)?;
    let signature = sign_payload(shared_secret, &payload)?;
    let payload_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload);
    let signature_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, signature);
    Ok(format!("{payload_b64}.{signature_b64}"))
}

pub fn verify_session_token(
    shared_secret: &str,
    token: &str,
) -> anyhow::Result<SessionTokenClaims> {
    let (payload_b64, signature_b64) = token
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("invalid session token format"))?;
    let payload = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        payload_b64,
    )
    .map_err(|error| anyhow::anyhow!("invalid session token payload: {error}"))?;
    let signature = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        signature_b64,
    )
    .map_err(|error| anyhow::anyhow!("invalid session token signature encoding: {error}"))?;
    let expected_signature = sign_payload(shared_secret, &payload)?;

    if signature != expected_signature {
        anyhow::bail!("invalid session token signature");
    }

    let claims: SessionTokenClaims = serde_json::from_slice(&payload)?;
    let now = Utc::now();
    let expires_with_leeway =
        claims.expires_at + Duration::minutes(SESSION_TOKEN_CLOCK_SKEW_MINUTES);
    if expires_with_leeway < now {
        anyhow::bail!(
            "session token expired expires_at={} now={} leeway_minutes={}",
            claims.expires_at,
            now,
            SESSION_TOKEN_CLOCK_SKEW_MINUTES
        );
    }

    Ok(claims)
}

fn sign_payload(shared_secret: &str, payload: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(shared_secret.as_bytes())
        .map_err(|error| anyhow::anyhow!("invalid HMAC key: {error}"))?;
    mac.update(payload);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
