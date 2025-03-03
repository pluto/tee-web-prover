use std::convert::Infallible;
use std::net::SocketAddr;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 80));

    // We create a TcpListener and bind it to 0.0.0.0:80
    let listener = TcpListener::bind(addr).await?;

    println!("Listening on {}", addr);

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(health))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}
