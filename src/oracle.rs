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

/// Pack a signed coefficient and exponent into a Rain DecimalFloat (bytes32).
///
/// Rain float layout (256 bits):
/// - Top 32 bits: exponent (int32)
/// - Bottom 224 bits: signed coefficient (int224)
///
/// The coefficient must fit in int224 and the exponent must fit in int32.
pub fn pack_rain_float(coefficient: i64, exponent: i32) -> FixedBytes<32> {
    // coefficient as i224 (sign-extended in the bottom 224 bits)
    // exponent as i32 (in the top 32 bits)
    let mut bytes = [0u8; 32];

    // Write exponent into top 4 bytes (big-endian)
    let exp_bytes = exponent.to_be_bytes();
    bytes[0..4].copy_from_slice(&exp_bytes);

    // Write coefficient into bottom 28 bytes (big-endian, sign-extended)
    // i64 fits easily in i224
    let coeff_i128 = coefficient as i128;
    let coeff_bytes = coeff_i128.to_be_bytes(); // 16 bytes

    // Sign-extend: if negative, fill bytes 4..16 with 0xFF, else 0x00
    let fill = if coefficient < 0 { 0xFF } else { 0x00 };
    for byte in bytes.iter_mut().take(16).skip(4) {
        *byte = fill;
    }

    // Copy the 16 bytes of i128 into bytes[16..32]
    bytes[16..32].copy_from_slice(&coeff_bytes);

    FixedBytes::from(bytes)
}

/// Build the context array from a Pyth price and expiry timestamp.
///
/// Context layout:
/// - [0]: price as a Rain DecimalFloat (coefficient * 10^exponent)
/// - [1]: expiry timestamp as a Rain DecimalFloat
pub fn build_context(price: i64, expo: i32, expiry: u64) -> Vec<FixedBytes<32>> {
    // Pack price directly as Rain float — Pyth already gives us coefficient + exponent
    let price_float = pack_rain_float(price, expo);

    // Pack expiry as Rain float: coefficient=expiry, exponent=0
    let expiry_float = pack_rain_float(expiry as i64, 0);

    vec![price_float, expiry_float]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_rain_float_simple() {
        // 3.14 = coefficient 314, exponent -2
        let float = pack_rain_float(314, -2);

        // Verify exponent in top 4 bytes
        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        let exp = i32::from_be_bytes(exp_bytes);
        assert_eq!(exp, -2);

        // Verify coefficient in bottom 28 bytes (sign-extended i224)
        // For positive 314, bottom bytes should end with ...00 00 01 3A
        assert_eq!(float.0[30], 0x01);
        assert_eq!(float.0[31], 0x3A);
        // Fill bytes should be 0x00
        assert_eq!(float.0[4], 0x00);
    }

    #[test]
    fn test_pack_rain_float_negative() {
        // -100 = coefficient -100, exponent 0
        let float = pack_rain_float(-100, 0);

        // Verify exponent
        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        let exp = i32::from_be_bytes(exp_bytes);
        assert_eq!(exp, 0);

        // For negative coefficient, fill bytes should be 0xFF
        assert_eq!(float.0[4], 0xFF);
    }

    #[test]
    fn test_pack_rain_float_pyth_price() {
        // Typical Pyth ETH/USD: price=310012345678, expo=-8
        // This means $3100.12345678
        let float = pack_rain_float(310012345678, -8);

        // Verify exponent
        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        let exp = i32::from_be_bytes(exp_bytes);
        assert_eq!(exp, -8);

        // Verify coefficient is positive and correct
        assert_eq!(float.0[4], 0x00); // positive fill
    }

    #[test]
    fn test_build_context_layout() {
        let ctx = build_context(310012345678, -8, 1700000000);
        assert_eq!(ctx.len(), 2);
    }

    #[test]
    fn test_pack_rain_float_zero() {
        let float = pack_rain_float(0, 0);
        // Should be all zeros
        assert_eq!(float, FixedBytes::ZERO);
    }

    #[test]
    fn test_pack_rain_float_expiry() {
        // Expiry timestamp: 1700000000, exponent 0
        let float = pack_rain_float(1700000000, 0);

        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        assert_eq!(i32::from_be_bytes(exp_bytes), 0);

        // Reconstruct coefficient from bottom bytes
        let mut coeff_bytes = [0u8; 16];
        coeff_bytes.copy_from_slice(&float.0[16..32]);
        let coeff = i128::from_be_bytes(coeff_bytes);
        assert_eq!(coeff, 1700000000);
    }
}
