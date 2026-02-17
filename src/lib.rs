pub mod oracle;
pub mod pyth;
pub mod sign;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sign::Signer;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

/// Application state shared across handlers.
pub struct AppState {
    signer: Signer,
    pyth_price_feed_id: String,
    expiry_seconds: u64,
}

impl AppState {
    pub fn new(
        private_key: &str,
        pyth_price_feed_id: &str,
        expiry_seconds: u64,
    ) -> anyhow::Result<Self> {
        let signer = Signer::new(private_key)?;
        Ok(Self {
            signer,
            pyth_price_feed_id: pyth_price_feed_id.to_string(),
            expiry_seconds,
        })
    }

    pub fn signer_address(&self) -> alloy::primitives::Address {
        self.signer.address()
    }
}

pub fn create_app(state: AppState) -> Router {
    let shared_state = Arc::new(state);
    Router::new()
        .route("/", get(health))
        .route("/context", post(post_signed_context))
        .layer(CorsLayer::permissive())
        .with_state(shared_state)
}

async fn health() -> &'static str {
    "ok"
}

/// POST handler — receives ABI-encoded (OrderV4, uint256, uint256, address)
/// from the SDK. For now we ignore the body and return the same price data,
/// but oracle implementations can use the order details to tailor responses.
async fn post_signed_context(
    State(state): State<Arc<AppState>>,
    _body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // TODO: decode ABI body for order-aware oracle responses
    // For now, return the same signed context regardless of order details
    build_signed_context_response(&state).await
}

async fn build_signed_context_response(
    state: &AppState,
) -> Result<impl IntoResponse, AppError> {
    let price_data = pyth::fetch_price(&state.pyth_price_feed_id).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expiry = now + state.expiry_seconds;

    let context = oracle::build_context(price_data.price, price_data.expo, expiry)?;

    let (signature, signer) = state.signer.sign_context(&context).await?;

    let response = oracle::OracleResponse {
        signer,
        context,
        signature,
    };

    Ok(Json(response))
}

/// Application error type for axum handlers.
pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("Handler error: {:?}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal error: {}", self.0),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self(err)
    }
}
