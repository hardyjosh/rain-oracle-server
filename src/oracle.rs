use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use serde::{Deserialize, Serialize};

/// Oracle response matching the SDK's expected format.
/// Maps directly to `SignedContextV1` in the Rain orderbook contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleResponse {
    /// The signer address (EIP-191 signer of the context data)
    pub signer: Address,
    /// The signed context data as bytes32[] values
    pub context: Vec<FixedBytes<32>>,
    /// The EIP-191 signature over keccak256(abi.encodePacked(context))
    pub signature: Bytes,
}

/// Build the context array from a Pyth price and expiry timestamp.
///
/// Context layout:
/// - [0]: ETH/USD price scaled to 18 decimals
/// - [1]: expiry timestamp (unix seconds)
pub fn build_context(price: i64, expo: i32, expiry: u64) -> Vec<FixedBytes<32>> {
    let price_18 = scale_price_to_18_decimals(price, expo);
    let price_bytes = FixedBytes::<32>::from(U256::from(price_18));
    let expiry_bytes = FixedBytes::<32>::from(U256::from(expiry));
    vec![price_bytes, expiry_bytes]
}

/// Scale a Pyth price (with exponent) to 18 decimal fixed point.
///
/// Pyth prices come as `price * 10^expo` where expo is typically negative
/// (e.g. price=310000000000, expo=-8 means $3100.00000000).
/// We need to convert to 18 decimal fixed point: price * 10^18.
pub fn scale_price_to_18_decimals(price: i64, expo: i32) -> u128 {
    // price represents price * 10^expo
    // We want price * 10^18
    // So multiply by 10^(18 - (-expo)) = 10^(18 + expo)
    let price = price.unsigned_abs() as u128;
    let shift = 18 + expo; // expo is negative, so this is 18 - |expo|

    if shift >= 0 {
        price * 10u128.pow(shift as u32)
    } else {
        // If expo is very negative (more than -18 decimals), divide
        price / 10u128.pow((-shift) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_price_typical_pyth() {
        // ETH at $3100.12345678, expo=-8
        // price = 310012345678, expo = -8
        // Expected: 3100_12345678_0000000000 (18 decimals)
        let result = scale_price_to_18_decimals(310012345678, -8);
        assert_eq!(result, 3_100_123_456_780_000_000_000u128);
    }

    #[test]
    fn test_scale_price_expo_minus_5() {
        // price = 310000, expo = -5 => $3.10000
        // 18 decimals: 3_100_000_000_000_000_000
        let result = scale_price_to_18_decimals(310000, -5);
        assert_eq!(result, 3_100_000_000_000_000_000u128);
    }

    #[test]
    fn test_scale_price_expo_zero() {
        // price = 3100, expo = 0 => $3100
        // 18 decimals: 3100 * 10^18
        let result = scale_price_to_18_decimals(3100, 0);
        assert_eq!(result, 3_100_000_000_000_000_000_000u128);
    }

    #[test]
    fn test_build_context_layout() {
        let ctx = build_context(310000000000, -8, 1700000000);
        assert_eq!(ctx.len(), 2);

        // Price slot
        let price_u256 = U256::from_be_bytes(*ctx[0]);
        assert_eq!(price_u256, U256::from(3_100_000_000_000_000_000_000u128));

        // Expiry slot
        let expiry_u256 = U256::from_be_bytes(*ctx[1]);
        assert_eq!(expiry_u256, U256::from(1700000000u64));
    }

    #[test]
    fn test_scale_price_very_negative_expo() {
        // expo = -20, more decimals than 18
        // price = 3100000000000, expo = -20 => $0.000000031
        // 18 decimals: 31_000_000_000
        let result = scale_price_to_18_decimals(3_100_000_000_000, -20);
        assert_eq!(result, 31_000_000_000u128);
    }
}
