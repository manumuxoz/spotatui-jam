use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let config = match party_relay::Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("party-relay: {error}");
            std::process::exit(2);
        }
    };

    if let Err(error) = party_relay::run(config).await {
        eprintln!("party-relay: {error}");
        std::process::exit(1);
    }
}
