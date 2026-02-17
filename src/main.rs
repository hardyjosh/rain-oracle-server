use clap::Parser;
use rain_oracle_server::{create_app, AppState, TokenPairConfig};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "rain-oracle-server")]
#[command(about = "Reference signed context oracle server for Raindex")]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "3000", env = "PORT")]
    port: u16,

    /// Private key for EIP-191 signing (hex, with or without 0x prefix)
    #[arg(long, env = "SIGNER_PRIVATE_KEY")]
    signer_private_key: String,

    /// Pyth price feed ID (the feed returns base/quote, e.g. ETH/USD)
    #[arg(
        long,
        default_value = "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace",
        env = "PYTH_PRICE_FEED_ID"
    )]
    pyth_price_feed_id: String,

    /// Base token address (the asset priced BY the feed, e.g. WETH for ETH/USD)
    #[arg(long, env = "BASE_TOKEN")]
    base_token: String,

    /// Quote token address (the denomination of the feed, e.g. USDC for ETH/USD)
    #[arg(long, env = "QUOTE_TOKEN")]
    quote_token: String,

    /// Signed context expiry in seconds
    #[arg(long, default_value = "5", env = "EXPIRY_SECONDS")]
    expiry_seconds: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();

    let token_pair = TokenPairConfig::new(&cli.base_token, &cli.quote_token)?;

    let state = AppState::new(
        &cli.signer_private_key,
        &cli.pyth_price_feed_id,
        cli.expiry_seconds,
        token_pair,
    )?;

    tracing::info!("Signer address: {}", state.signer_address());

    let app = create_app(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], cli.port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
