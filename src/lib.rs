pub mod oracle;
pub mod pyth;
pub mod sign;

use alloy::primitives::Address;
use alloy::sol;
use alloy::sol_types::SolValue;
use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use sign::Signer;
use std::str::FromStr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

// Minimal OrderV4 definition for ABI decoding — avoids pulling in rain_orderbook_bindings.
sol! {
    struct IO {
        address token;
        uint8 decimals;
        uint256 vaultId;
    }

    struct EvaluableV3 {
        address interpreter;
        address store;
        bytes bytecode;
    }

    struct OrderV4 {
        address owner;
        EvaluableV3 evaluable;
        IO[] validInputs;
        IO[] validOutputs;
        bytes32 nonce;
    }
}

/// Decoded POST body: (OrderV4, uint256 inputIOIndex, uint256 outputIOIndex, address counterparty)
type OracleRequestBody = (OrderV4, alloy::primitives::U256, alloy::primitives::U256, Address);

/// Token pair config — maps token addresses to base/quote roles for a Pyth feed.
///
/// The Pyth feed returns price as base/quote (e.g. ETH/USD = ~1900).
/// - base_token: the token priced by the feed (e.g. WETH)
/// - quote_token: the denomination (e.g. USDC)
#[derive(Clone)]
pub struct TokenPairConfig {
    pub base_token: Address,
    pub quote_token: Address,
}

impl TokenPairConfig {
    pub fn new(base_token: &str, quote_token: &str) -> anyhow::Result<Self> {
        Ok(Self {
            base_token: Address::from_str(base_token)
                .map_err(|e| anyhow::anyhow!("Invalid base token address: {}", e))?,
            quote_token: Address::from_str(quote_token)
                .map_err(|e| anyhow::anyhow!("Invalid quote token address: {}", e))?,
        })
    }
}

/// Whether to return the price as-is or inverted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PriceDirection {
    /// Input is quote, output is base → return price as-is
    /// e.g. input=USDC, output=WETH → "how many USDC per WETH" → ~1900
    AsIs,
    /// Input is base, output is quote → invert the price
    /// e.g. input=WETH, output=USDC → "how many WETH per USDC" → ~0.000526
    Inverted,
}

/// Application state shared across handlers.
pub struct AppState {
    signer: Signer,
    pyth_price_feed_id: String,
    expiry_seconds: u64,
    token_pair: TokenPairConfig,
}

impl AppState {
    pub fn new(
        private_key: &str,
        pyth_price_feed_id: &str,
        expiry_seconds: u64,
        token_pair: TokenPairConfig,
    ) -> anyhow::Result<Self> {
        let signer = Signer::new(private_key)?;
        Ok(Self {
            signer,
            pyth_price_feed_id: pyth_price_feed_id.to_string(),
            expiry_seconds,
            token_pair,
        })
    }

    pub fn signer_address(&self) -> Address {
        self.signer.address()
    }

    /// Determine price direction from the order's input/output tokens.
    fn price_direction(&self, input_token: Address, output_token: Address) -> Result<PriceDirection, OracleRequestError> {
        let is_input_base = input_token == self.token_pair.base_token;
        let is_input_quote = input_token == self.token_pair.quote_token;
        let is_output_base = output_token == self.token_pair.base_token;
        let is_output_quote = output_token == self.token_pair.quote_token;

        match (is_input_base, is_input_quote, is_output_base, is_output_quote) {
            // input=quote (USDC), output=base (WETH) → price as-is (USDC per WETH)
            (_, true, true, _) => Ok(PriceDirection::AsIs),
            // input=base (WETH), output=quote (USDC) → inverted (WETH per USDC)
            (true, _, _, true) => Ok(PriceDirection::Inverted),
            _ => Err(OracleRequestError::UnsupportedTokenPair {
                input_token,
                output_token,
                base_token: self.token_pair.base_token,
                quote_token: self.token_pair.quote_token,
            }),
        }
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

/// Error response body for client-facing errors.
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    detail: String,
}

/// POST handler — receives ABI-encoded (OrderV4, uint256 inputIOIndex, uint256 outputIOIndex, address counterparty).
/// Decodes the order to determine input/output tokens and returns the correctly-directed price.
async fn post_signed_context(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // Decode the ABI-encoded request body
    let (order, input_io_index, output_io_index, _counterparty) =
        <OracleRequestBody>::abi_decode(&body, true)
            .map_err(|e| OracleRequestError::InvalidBody(e.to_string()))?;

    let input_idx = input_io_index.try_into().unwrap_or(usize::MAX);
    let output_idx = output_io_index.try_into().unwrap_or(usize::MAX);

    // Extract input/output token addresses from the order
    let input_token = order
        .validInputs
        .get(input_idx)
        .ok_or_else(|| OracleRequestError::InvalidIndex {
            kind: "input",
            index: input_idx,
            len: order.validInputs.len(),
        })?
        .token;

    let output_token = order
        .validOutputs
        .get(output_idx)
        .ok_or_else(|| OracleRequestError::InvalidIndex {
            kind: "output",
            index: output_idx,
            len: order.validOutputs.len(),
        })?
        .token;

    // Determine price direction
    let direction = state.price_direction(input_token, output_token)?;

    tracing::debug!(
        "Oracle request: input={} output={} direction={:?}",
        input_token,
        output_token,
        direction
    );

    build_signed_context_response(&state, direction).await
}

async fn build_signed_context_response(
    state: &AppState,
    direction: PriceDirection,
) -> Result<impl IntoResponse, AppError> {
    let price_data = pyth::fetch_price(&state.pyth_price_feed_id).await?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expiry = now + state.expiry_seconds;

    let context = oracle::build_context(price_data.price, price_data.expo, expiry, direction)?;

    let (signature, signer) = state.signer.sign_context(&context).await?;

    let response = oracle::OracleResponse {
        signer,
        context,
        signature,
    };

    Ok(Json(response))
}

/// Client-facing request errors (returned as 400).
#[derive(Debug, thiserror::Error)]
pub enum OracleRequestError {
    #[error("Invalid ABI-encoded body: {0}")]
    InvalidBody(String),

    #[error("Invalid {kind} IO index: {index} (order has {len} {kind}s)")]
    InvalidIndex {
        kind: &'static str,
        index: usize,
        len: usize,
    },

    #[error("Unsupported token pair: input {input_token} / output {output_token} does not match configured pair (base={base_token}, quote={quote_token})")]
    UnsupportedTokenPair {
        input_token: Address,
        output_token: Address,
        base_token: Address,
        quote_token: Address,
    },
}

/// Application error type for axum handlers.
pub enum AppError {
    Internal(anyhow::Error),
    BadRequest(OracleRequestError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::Internal(err) => {
                tracing::error!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        detail: format!("{}", err),
                    }),
                )
                    .into_response()
            }
            AppError::BadRequest(err) => {
                tracing::warn!("Bad request: {}", err);
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: err.error_code().to_string(),
                        detail: format!("{}", err),
                    }),
                )
                    .into_response()
            }
        }
    }
}

impl OracleRequestError {
    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidBody(_) => "invalid_body",
            Self::InvalidIndex { .. } => "invalid_index",
            Self::UnsupportedTokenPair { .. } => "unsupported_token_pair",
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl From<OracleRequestError> for AppError {
    fn from(err: OracleRequestError) -> Self {
        Self::BadRequest(err)
    }
}
