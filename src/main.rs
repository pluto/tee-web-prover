use errors::NotaryServerError;
use futures_util::StreamExt;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer},
};
use rustls_acme::caches::DirCache;
use std::convert::Infallible;
use std::fs;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod errors;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_line_number(true))
        .with(tracing_subscriber::EnvFilter::from_default_env()) // set via RUST_LOG=INFO etc
        .init();

    let c = config::read_config();

    // We create a TcpListener and bind it to 0.0.0.0:80
    let listener = TcpListener::bind(&c.listen).await?;

    info!("Listening on {}", &c.listen);

    if c.is_acme() {
        info!("Using ACME for domain: {}", c.acme_domain);
        let _ = acme_listen(listener, &c.acme_domain, &c.acme_email).await;
        todo!("")
    } else {
        info!("Using provided TLS certifcates");
        let _ = listen(listener, &c.server_cert, &c.server_key).await;
    };

    Ok(())
}

async fn acme_listen(
    listener: TcpListener,
    // router: Router,
    domain: &str,
    email: &str,
) -> Result<(), NotaryServerError> {
    let protocol = Arc::new(http1::Builder::new());

    let mut state = rustls_acme::AcmeConfig::new([domain])
        .contact_push(format!("mailto:{}", email))
        .cache(DirCache::new("./rustls_acme_cache")) // TODO make this a config
        .directory_lets_encrypt(true)
        .state();
    let challenge_rustls_config = state.challenge_rustls_config();

    let mut rustls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(state.resolver());
    rustls_config.alpn_protocols = vec![b"http/1.1".to_vec()];

    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(result) => match result {
                    Ok(ok) => info!("event: {:?}", ok),
                    Err(err) => error!("error: {:?}", err),
                },
                None => {
                    error!("ACME state stream ended unexpectedly");
                }
            }
        }
    });

    loop {
        let (tcp, _) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                continue;
            }
        };
        let challenge_rustls_config = challenge_rustls_config.clone();
        let rustls_config = rustls_config.clone();
        let protocol = protocol.clone();

        tokio::spawn(async move {
            let start_handshake =
                match tokio_rustls::LazyConfigAcceptor::new(Default::default(), tcp).await {
                    Ok(handshake) => handshake,
                    Err(e) => {
                        error!("Failed to initialize TLS handshake: {}", e);
                        return;
                    }
                };

            if rustls_acme::is_tls_alpn_challenge(&start_handshake.client_hello()) {
                info!("received TLS-ALPN-01 validation request");
                let mut tls = match start_handshake.into_stream(challenge_rustls_config).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        error!("Failed to establish TLS-ALPN challenge stream: {}", e);
                        return;
                    }
                };
                // Use AsyncWriteExt for shutdown()
                use tokio::io::AsyncWriteExt;
                if let Err(e) = tls.shutdown().await {
                    error!("Failed to shutdown TLS-ALPN challenge connection: {}", e);
                }
            } else {
                let tls = match start_handshake.into_stream(Arc::new(rustls_config)).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        error!("Failed to establish TLS stream: {}", e);
                        return;
                    }
                };
                let io = TokioIo::new(tls);
                let hyper_service = hyper::service::service_fn(router);
                // move |request: Request<Incoming>| {
                // tower_service.clone().call(request)
                // });
                if let Err(e) = protocol
                    .serve_connection(io, hyper_service)
                    .with_upgrades()
                    .await
                {
                    error!("Connection error: {}", e);
                }
            }
        });
    }
}

async fn listen(
    listener: TcpListener,
    // router: Router,
    server_cert_path: &str,
    server_key_path: &str,
) -> Result<(), NotaryServerError> {
    let protocol = Arc::new(http1::Builder::new());

    info!("Using {} and {}", server_cert_path, server_key_path);
    let certs = match load_certs(server_cert_path) {
        Ok(certs) => certs,
        Err(e) => {
            error!("Failed to load certificates: {}", e);
            return Err(NotaryServerError::CertificateError(e.to_string()));
        }
    };

    let key = match load_private_key(server_key_path) {
        Ok(key) => key,
        Err(e) => {
            error!("Failed to load private key: {}", e);
            return Err(NotaryServerError::CertificateError(e.to_string()));
        }
    };

    let server_config = match ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
    {
        Ok(config) => {
            let mut config = config;
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            config
        }
        Err(e) => {
            error!("Failed to create server config: {}", e);
            return Err(NotaryServerError::ServerConfigError(e.to_string()));
        }
    };

    let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

    loop {
        let (tcp_stream, _) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                continue;
            }
        };
        let tls_acceptor = tls_acceptor.clone();
        // let tower_service = router.clone();
        let protocol = protocol.clone();

        tokio::spawn(async move {
            match tls_acceptor.accept(tcp_stream).await {
                Ok(tls_stream) => {
                    let io = TokioIo::new(tls_stream);
                    let hyper_service = hyper::service::service_fn(router);
                    // move |request: Request<Incoming>| {
                    // tower_service.clone().call(request)
                    // });
                    if let Err(e) = protocol
                        .serve_connection(io, hyper_service)
                        .with_upgrades()
                        .await
                    {
                        error!("Connection error: {}", e);
                    }
                }
                Err(err) => {
                    error!("TLS acceptance error: {}", err);
                }
            }
        });
    }
}

fn load_certs(filename: &str) -> std::io::Result<Vec<CertificateDer<'static>>> {
    let certfile = fs::File::open(filename)
        .map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
    let mut reader = std::io::BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader).collect()
}

fn load_private_key(filename: &str) -> std::io::Result<PrivateKeyDer<'static>> {
    let keyfile = fs::File::open(filename)
        .map_err(|e| error(format!("failed to open {}: {}", filename, e)))?;
    let mut reader = std::io::BufReader::new(keyfile);
    rustls_pemfile::private_key(&mut reader).map(|key| key.unwrap())
}

fn error(err: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

async fn router(req: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&hyper::Method::GET, "/health") => health(req).await,
        _ => {
            let response = Response::builder()
                .status(404)
                .body(Full::new(Bytes::from("Not Found\n")))
                .unwrap();
            Ok(response)
        }
    }
}

async fn health(_: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let git_hash = match std::fs::read_to_string("/etc/tee/git_hash") {
        Ok(hash) => hash.trim().to_string(),
        Err(_) => "unknown".to_string(),
    };

    let git_branch = match std::fs::read_to_string("/etc/tee/git_branch") {
        Ok(branch) => branch.trim().to_string(),
        Err(_) => "unknown".to_string(),
    };

    let response_body = format!("GIT_HASH: {}\nGIT_BRANCH: {}\n", git_hash, git_branch);

    return Ok(Response::new(Full::new(Bytes::from(response_body))));
}
