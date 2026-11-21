//! Health probes over HTTP (ADR-019), as the Elixir server answers them:
//! `GET /health/live` is 200 while the process runs, `GET /health/ready` is
//! 200 while the node is ready for clients and 503 otherwise, any other
//! path is 404. A probe sends one request per connection; only its first
//! line is read.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// How long a probe may take to send its request line.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Whether the node is ready.
pub type Ready = Arc<dyn Fn() -> bool + Send + Sync>;

/// Answers probes on `listener` until it fails.
pub async fn serve(listener: TcpListener, ready: Ready) {
    while let Ok((stream, _peer)) = listener.accept().await {
        tokio::spawn(answer(stream, Arc::clone(&ready)));
    }
}

/// The status line and body for a request line.
fn reply(request: &[u8], ready: &dyn Fn() -> bool) -> (&'static str, &'static str) {
    let line = request
        .split(|byte| *byte == b'\r' || *byte == b'\n')
        .next();
    let mut parts = line.unwrap_or_default().split(|byte| *byte == b' ');
    match (parts.next(), parts.next()) {
        (Some(b"GET"), Some(b"/health/live")) => ("200 OK", "live"),
        (Some(b"GET"), Some(b"/health/ready")) if ready() => ("200 OK", "ready"),
        (Some(b"GET"), Some(b"/health/ready")) => ("503 Service Unavailable", "not ready"),
        _ => ("404 Not Found", "not found"),
    }
}

async fn answer(mut stream: TcpStream, ready: Ready) {
    let mut request = [0_u8; 1_024];
    let Ok(Ok(read)) = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut request)).await else {
        return;
    };
    let (status, body) = reply(request.get(..read).unwrap_or_default(), &*ready);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    // A probe that went away needs no answer.
    let _written = stream.write_all(response.as_bytes()).await;
}

#[cfg(test)]
mod tests {
    use super::reply;

    #[test]
    fn probes_get_elixir_s_answers() {
        let up = || true;
        let down = || false;
        assert_eq!(reply(b"GET /health/live HTTP/1.1\r\n", &down).0, "200 OK");
        assert_eq!(reply(b"GET /health/ready HTTP/1.1\r\n", &up).0, "200 OK");
        assert_eq!(
            reply(b"GET /health/ready HTTP/1.1\r\n", &down).0,
            "503 Service Unavailable"
        );
        assert_eq!(reply(b"GET /other HTTP/1.1\r\n", &up).0, "404 Not Found");
        assert_eq!(
            reply(b"POST /health/live HTTP/1.1\r\n", &up).0,
            "404 Not Found"
        );
    }
}
