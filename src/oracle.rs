use alloy::primitives::{Address, Bytes, FixedBytes};
use serde::{Deserialize, Serialize};

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

/// Pack a signed coefficient and exponent into a Rain DecimalFloat (bytes32).
///
/// Rain float layout (256 bits, big-endian):
/// - Top 32 bits (bytes 0..4): exponent as int32
/// - Bottom 224 bits (bytes 4..32): coefficient as int224 (sign-extended)
///
/// Pyth gives coefficient * 10^exponent directly, which maps 1:1 to Rain's format.
pub fn pack_rain_float(coefficient: i64, exponent: i32) -> FixedBytes<32> {
    let mut bytes = [0u8; 32];

    // Write exponent into top 4 bytes (big-endian)
    bytes[0..4].copy_from_slice(&exponent.to_be_bytes());

    // Write coefficient into bottom 28 bytes (big-endian, sign-extended i224)
    // i64 fits in i224. Sign-extend by filling bytes 4..16 with 0xFF (negative) or 0x00 (positive).
    let fill = if coefficient < 0 { 0xFF } else { 0x00 };
    for byte in bytes.iter_mut().take(16).skip(4) {
        *byte = fill;
    }

    // i64 as i128 -> 16 big-endian bytes into bytes[16..32]
    let coeff_bytes = (coefficient as i128).to_be_bytes();
    bytes[16..32].copy_from_slice(&coeff_bytes);

    FixedBytes::from(bytes)
}

/// Build the context array from a Pyth price and expiry timestamp.
///
/// Context layout:
/// - [0]: price as a Rain DecimalFloat (coefficient * 10^exponent)
/// - [1]: expiry timestamp as a Rain DecimalFloat (coefficient=expiry, exponent=0)
pub fn build_context(price: i64, expo: i32, expiry: u64) -> Vec<FixedBytes<32>> {
    let price_float = pack_rain_float(price, expo);
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

        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        assert_eq!(i32::from_be_bytes(exp_bytes), -2);

        // Positive coefficient, bottom bytes should contain 314 = 0x13A
        assert_eq!(float.0[30], 0x01);
        assert_eq!(float.0[31], 0x3A);
        assert_eq!(float.0[4], 0x00); // positive fill
    }

    #[test]
    fn test_pack_rain_float_negative() {
        let float = pack_rain_float(-100, 0);

        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        assert_eq!(i32::from_be_bytes(exp_bytes), 0);
        assert_eq!(float.0[4], 0xFF); // negative fill
    }

    #[test]
    fn test_pack_rain_float_pyth_price() {
        // Typical Pyth ETH/USD: price=310012345678, expo=-8 => $3100.12345678
        let float = pack_rain_float(310012345678, -8);

        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        assert_eq!(i32::from_be_bytes(exp_bytes), -8);
        assert_eq!(float.0[4], 0x00); // positive
    }

    #[test]
    fn test_build_context_layout() {
        let ctx = build_context(310012345678, -8, 1700000000);
        assert_eq!(ctx.len(), 2);
    }

    #[test]
    fn test_pack_rain_float_zero() {
        let float = pack_rain_float(0, 0);
        assert_eq!(float, FixedBytes::ZERO);
    }

    #[test]
    fn test_pack_rain_float_expiry() {
        let float = pack_rain_float(1700000000, 0);

        let exp_bytes: [u8; 4] = float.0[0..4].try_into().unwrap();
        assert_eq!(i32::from_be_bytes(exp_bytes), 0);

        let mut coeff_bytes = [0u8; 16];
        coeff_bytes.copy_from_slice(&float.0[16..32]);
        assert_eq!(i128::from_be_bytes(coeff_bytes), 1700000000);
    }
}
