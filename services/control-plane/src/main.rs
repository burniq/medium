use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bind_addr =
        std::env::var("OVERLAY_CONTROL_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let state = control_plane::state::ControlState::from_env().await?;
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    let app = control_plane::app::build_router(state);

    match tls_config_from_env()? {
        Some(config) => serve_tls(listener, app, config).await,
        None => {
            axum::serve(listener, app).await?;
            Ok(())
        }
    }
}

async fn serve_tls(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    config: ServerConfig,
) -> anyhow::Result<()> {
    let acceptor = TlsAcceptor::from(Arc::new(config));
    loop {
        let (stream, _) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let app = app.clone();
        tokio::spawn(async move {
            let Ok(stream) = acceptor.accept(stream).await else {
                return;
            };
            let service = TowerToHyperService::new(app);
            let _ = Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(TokioIo::new(stream), service)
                .await;
        });
    }
}

fn tls_config_from_env() -> anyhow::Result<Option<ServerConfig>> {
    let cert_path = match std::env::var("MEDIUM_CONTROL_TLS_CERT_PATH") {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    let key_path = std::env::var("MEDIUM_CONTROL_TLS_KEY_PATH")?;
    let certs = load_certs(&cert_path)?;
    let key = load_key(&key_path)?;
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let config = ServerConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    Ok(Some(config))
}

fn load_certs(path: &str) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path)?);
    Ok(rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?)
}

fn load_key(path: &str) -> anyhow::Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::private_key(&mut reader)?.ok_or_else(|| anyhow::anyhow!("missing TLS key"))
}
