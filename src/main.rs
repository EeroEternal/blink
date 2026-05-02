mod protocol;
mod hypervisor;
mod vsock;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Blink-Core (Rust) V-Hub starting on port 10000...");

    let listener = vsock::VsockListener::bind(10000)?;

    loop {
        let fd = listener.accept().await?;
        println!("New Agent connection accepted.");

        tokio::spawn(async move {
            if let Err(e) = vsock::handle_agent(fd).await {
                eprintln!("Agent handler error: {}", e);
            }
        });
    }
}
