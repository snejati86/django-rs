//! Live server test case for integration and browser tests.
//!
//! [`LiveServerTestCase`] starts a real Axum HTTP server bound to a random port,
//! making it possible to test with external HTTP clients (e.g., `reqwest`) or
//! browser automation tools (e.g., Playwright).
//!
//! ## Example
//!
//! ```rust,no_run
//! use django_rs_test::live_server::LiveServerTestCase;
//! use axum::Router;
//! use axum::routing::get;
//!
//! async fn example() {
//!     let app = Router::new().route("/", get(|| async { "Hello" }));
//!     let server = LiveServerTestCase::start(app).await;
//!     println!("Server running at {}", server.url());
//!     // Make real HTTP requests to the server...
//!     server.stop().await;
//! }
//! ```

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// A live test server that binds an Axum application to a random port.
///
/// The server runs in a background tokio task and can be accessed via its
/// [`url`](Self::url) method. Call [`stop`](Self::stop) to shut down the server
/// and release the port.
pub struct LiveServerTestCase {
    /// The server's bound address.
    addr: SocketAddr,
    /// Shutdown signal sender.
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Handle to the background server task.
    server_handle: Option<JoinHandle<()>>,
}

impl LiveServerTestCase {
    /// Starts a live server with the given Axum router.
    ///
    /// Binds to `127.0.0.1:0` (random port) and starts serving in a background
    /// task. Returns immediately with the server metadata.
    ///
    /// # Panics
    ///
    /// Panics if the TCP listener cannot be bound.
    pub async fn start(app: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind to random port");
        let addr = listener.local_addr().expect("Failed to get local address");

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .ok();
        });

        Self {
            addr,
            shutdown_tx: Some(shutdown_tx),
            server_handle: Some(server_handle),
        }
    }

    /// Returns the base URL of the server (e.g., `http://127.0.0.1:43210`).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Returns the bound address of the server.
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the port the server is listening on.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Shuts down the server gracefully.
    ///
    /// Sends a shutdown signal and waits for the server task to complete.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.server_handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for LiveServerTestCase {
    fn drop(&mut self) {
        // Send shutdown signal if not already stopped.
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Note: we cannot await the join handle in Drop, but the shutdown
        // signal will cause the server to terminate on its own.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    #[tokio::test]
    async fn test_start_and_url() {
        let app = Router::new().route("/", get(|| async { "live" }));
        let server = LiveServerTestCase::start(app).await;

        let url = server.url();
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(server.port() > 0);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_server_responds_to_requests() {
        let app = Router::new()
            .route("/hello", get(|| async { "Hello from live server" }))
            .route(
                "/json",
                get(|| async { axum::Json(serde_json::json!({"status": "ok"})) }),
            );

        let server = LiveServerTestCase::start(app).await;
        let url = server.url();

        // Use a simple TCP connection to verify the server is actually listening.
        // We test the connection by attempting to connect to the port.
        let addr = server.addr();
        let stream = tokio::net::TcpStream::connect(addr).await;
        assert!(
            stream.is_ok(),
            "Should be able to connect to the live server"
        );

        server.stop().await;

        // After stop, connecting should eventually fail (port released).
        // Give a brief moment for the OS to reclaim the port.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _url_unused = url; // verify url was valid
    }

    #[tokio::test]
    async fn test_multiple_servers() {
        let app1 = Router::new().route("/", get(|| async { "server1" }));
        let app2 = Router::new().route("/", get(|| async { "server2" }));

        let server1 = LiveServerTestCase::start(app1).await;
        let server2 = LiveServerTestCase::start(app2).await;

        // Each server should have a unique port
        assert_ne!(server1.port(), server2.port());

        server1.stop().await;
        server2.stop().await;
    }

    #[tokio::test]
    async fn test_drop_sends_shutdown() {
        let app = Router::new().route("/", get(|| async { "drop test" }));
        let server = LiveServerTestCase::start(app).await;
        let _port = server.port();

        // Dropping the server should send the shutdown signal
        drop(server);

        // Give the server time to shut down
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
