use alloy::primitives::{Address, Bytes, FixedBytes};
use rain_math_float::Float;
use serde::{Deserialize, Serialize};

use crate::PriceDirection;

/// Oracle response matching the SDK's expected format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleResponse {
    /// The signer address (EIP-191 signer of the context data)
    pub signer: Address,
    /// The signed context data as bytes32[] values (Rain DecimalFloats)
    pub context: Vec<FixedBytes<32>>,
    /// The EIP-191 signature over keccak256(abi.encodePacked(context))
    pub signature: Bytes,
}

/// Format a Pyth price (coefficient * 10^expo) as a decimal string for Float::parse.
///
/// e.g. price=310012345678, expo=-8 => "3100.12345678"
fn format_pyth_price(price: i64, expo: i32) -> String {
    if expo >= 0 {
        let mut s = price.to_string();
        for _ in 0..expo {
            s.push('0');
        }
        s
    } else {
        let abs_expo = (-expo) as usize;
        let is_negative = price < 0;
        let digits = price.unsigned_abs().to_string();

        if digits.len() <= abs_expo {
            let zeros = abs_expo - digits.len();
            let prefix = if is_negative { "-0." } else { "0." };
            format!("{}{}{}", prefix, "0".repeat(zeros), digits)
        } else {
            let split_pos = digits.len() - abs_expo;
            let prefix = if is_negative { "-" } else { "" };
            format!("{}{}.{}", prefix, &digits[..split_pos], &digits[split_pos..])
        }
    }
}

/// Build the context array from a Pyth price and expiry timestamp.
///
/// All values are encoded as Rain DecimalFloats (bytes32) via Float::parse.
///
/// If direction is `Inverted`, the price is inverted (1/price) before encoding.
/// This is needed when input is the base asset and output is the quote asset,
/// because the order wants "how many base per quote" rather than "how many quote per base".
///
/// Context layout:
/// - [0]: price as Rain DecimalFloat
/// - [1]: expiry timestamp as Rain DecimalFloat
pub fn build_context(
    price: i64,
    expo: i32,
    expiry: u64,
    direction: PriceDirection,
) -> Result<Vec<FixedBytes<32>>, anyhow::Error> {
    let price_str = format_pyth_price(price, expo);
    let price_float = Float::parse(price_str.clone())
        .map_err(|e| anyhow::anyhow!("Failed to parse price '{}' as Rain float: {:?}", price_str, e))?;

    // Apply direction — invert if needed
    let final_price = match direction {
        PriceDirection::AsIs => price_float,
        PriceDirection::Inverted => {
            let one = Float::parse("1".to_string())
                .map_err(|e| anyhow::anyhow!("Failed to parse '1' as Rain float: {:?}", e))?;
            (one / price_float)
                .map_err(|e| anyhow::anyhow!("Failed to invert price: {:?}", e))?
        }
    };

    let expiry_str = expiry.to_string();
    let expiry_float = Float::parse(expiry_str.clone())
        .map_err(|e| anyhow::anyhow!("Failed to parse expiry '{}' as Rain float: {:?}", expiry_str, e))?;

    let price_bytes: alloy::primitives::B256 = final_price.into();
    let expiry_bytes: alloy::primitives::B256 = expiry_float.into();

    Ok(vec![
        FixedBytes::from(price_bytes),
        FixedBytes::from(expiry_bytes),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_pyth_price_typical() {
        assert_eq!(format_pyth_price(310012345678, -8), "3100.12345678");
    }

    #[test]
    fn test_format_pyth_price_small() {
        assert_eq!(format_pyth_price(31, -5), "0.00031");
    }

    #[test]
    fn test_format_pyth_price_positive_expo() {
        assert_eq!(format_pyth_price(3100, 0), "3100");
        assert_eq!(format_pyth_price(31, 2), "3100");
    }

    #[test]
    fn test_format_pyth_price_negative() {
        assert_eq!(format_pyth_price(-310012345678, -8), "-3100.12345678");
    }

    #[test]
    fn test_build_context_as_is() {
        let ctx = build_context(310012345678, -8, 1700000000, PriceDirection::AsIs).unwrap();
        assert_eq!(ctx.len(), 2);

        let price_float = Float::from(alloy::primitives::B256::from(ctx[0]));
        let formatted = price_float.format().unwrap();
        assert_eq!(formatted, "3100.12345678");
    }

    #[test]
    fn test_build_context_inverted() {
        // Price is 2000.0, inverted should be 0.0005
        let ctx = build_context(200000000000, -8, 1700000000, PriceDirection::Inverted).unwrap();
        assert_eq!(ctx.len(), 2);

        let price_float = Float::from(alloy::primitives::B256::from(ctx[0]));
        let formatted = price_float.format().unwrap();
        assert_eq!(formatted, "5e-4"); // 1/2000 = 0.0005
    }

    #[test]
    fn test_build_context_expiry_roundtrip() {
        let ctx = build_context(310012345678, -8, 1700000000, PriceDirection::AsIs).unwrap();

        let expiry_float = Float::from(alloy::primitives::B256::from(ctx[1]));
        let formatted = expiry_float.format().unwrap();
        assert_eq!(formatted, "1.7e9");
    }
}
