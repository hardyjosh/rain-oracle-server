use serde::Deserialize;

const HERMES_BASE_URL: &str = "https://hermes.pyth.network";

#[derive(Debug)]
pub struct PriceData {
    pub price: i64,
    pub expo: i32,
}

#[derive(Deserialize)]
struct HermesResponse {
    parsed: Vec<ParsedPriceFeed>,
}

#[derive(Deserialize)]
struct ParsedPriceFeed {
    price: PriceInfo,
}

#[derive(Deserialize)]
struct PriceInfo {
    price: String,
    expo: i32,
}

/// Fetch the latest price from Pyth Hermes API.
pub async fn fetch_price(feed_id: &str) -> anyhow::Result<PriceData> {
    let url = format!(
        "{}/v2/updates/price/latest?ids[]=0x{}",
        HERMES_BASE_URL, feed_id
    );

    let resp: HermesResponse = reqwest::get(&url).await?.error_for_status()?.json().await?;

    let feed = resp
        .parsed
        .first()
        .ok_or_else(|| anyhow::anyhow!("No price feed returned from Hermes"))?;

    let price: i64 = feed.price.price.parse()?;

    Ok(PriceData {
        price,
        expo: feed.price.expo,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_eth_price() {
        // ETH/USD feed ID
        let feed_id = "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace";
        let result = fetch_price(feed_id).await;
        assert!(result.is_ok(), "Failed to fetch ETH price: {:?}", result);

        let data = result.unwrap();
        assert!(data.price > 0, "Price should be positive");
        assert!(data.expo < 0, "Expo should be negative for USD prices");
        tracing::info!("ETH/USD: {} * 10^{}", data.price, data.expo);
    }
}
