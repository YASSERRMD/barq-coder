use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn start_daemon(port: u16) -> anyhow::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("BarqCoder Daemon listening on port {}", port);

    loop {
        let (mut socket, _) = listener.accept().await?;
        
        tokio::spawn(async move {
            let mut buf = vec![0; 1024];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => return, // Connection closed
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        tracing::info!("Daemon received: {}", msg);
                        // Future: Dispatch to orchestrator, stream JSONL events back
                        let response = format!("{{\"status\": \"received\", \"msg\": \"{}\"}}\n", msg.trim());
                        if socket.write_all(response.as_bytes()).await.is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }
}
