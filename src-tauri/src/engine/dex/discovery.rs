// Paw Agent Engine — DEX Token Discovery (search + trending) via DexScreener API

use std::collections::HashMap;
use std::time::Duration;

/// Search for tokens by name or symbol using the DexScreener public API.
/// Returns contract addresses, chain, price, volume, liquidity, and pair info.
pub async fn execute_dex_search_token(
    args: &serde_json::Value,
    _creds: &HashMap<String, String>,
) -> Result<String, String> {
    let query = args["query"].as_str()
        .ok_or("dex_search_token: missing 'query'. Provide a token name or symbol (e.g. 'KIMCHI', 'pepe', 'uniswap').")?;

    let chain_filter = args["chain"].as_str().unwrap_or("");
    let max_results = args["max_results"].as_u64().unwrap_or(10).min(25) as usize;

    // Query DexScreener search API
    let url = format!("https://api.dexscreener.com/latest/dex/search?q={}", urlencoding(query));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; PawAgent/1.0)")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("DexScreener API request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("DexScreener API returned status {}", response.status()));
    }

    let body: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse DexScreener response: {}", e))?;

    let pairs = body["pairs"].as_array()
        .ok_or("No pairs found in DexScreener response")?;

    if pairs.is_empty() {
        return Ok(format!("No results found for '{}'. Try a different search term.", query));
    }

    let mut output = format!("Search results for '{}'\n\n", query);
    let mut seen_tokens: HashMap<String, bool> = HashMap::new();
    let mut count = 0;

    for pair in pairs {
        if count >= max_results { break; }

        let chain_id = pair["chainId"].as_str().unwrap_or("unknown");

        // Apply chain filter if specified
        if !chain_filter.is_empty() {
            let filter_lower = chain_filter.to_lowercase();
            let chain_lower = chain_id.to_lowercase();
            if !chain_lower.contains(&filter_lower) {
                continue;
            }
        }

        let base_token = &pair["baseToken"];
        let quote_token = &pair["quoteToken"];
        let token_address = base_token["address"].as_str().unwrap_or("?");
        let token_name = base_token["name"].as_str().unwrap_or("?");
        let token_symbol = base_token["symbol"].as_str().unwrap_or("?");
        let quote_symbol = quote_token["symbol"].as_str().unwrap_or("?");

        // Deduplicate by token address + chain
        let dedup_key = format!("{}:{}", chain_id, token_address.to_lowercase());
        if seen_tokens.contains_key(&dedup_key) { continue; }
        seen_tokens.insert(dedup_key, true);

        let price_usd = pair["priceUsd"].as_str().unwrap_or("N/A");
        let pair_address = pair["pairAddress"].as_str().unwrap_or("?");
        let dex_id = pair["dexId"].as_str().unwrap_or("?");
        let url = pair["url"].as_str().unwrap_or("");

        output.push_str(&format!("{}. {} ({}) on {}\n", count + 1, token_name, token_symbol, chain_id));
        output.push_str(&format!("   Contract: {}\n", token_address));
        output.push_str(&format!("   Pair: {}/{} | DEX: {}\n", token_symbol, quote_symbol, dex_id));
        output.push_str(&format!("   Pair Address: {}\n", pair_address));
        output.push_str(&format!("   Price: ${}\n", price_usd));

        // Volume
        if let Some(volume) = pair["volume"].as_object() {
            let h24 = volume.get("h24").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h6 = volume.get("h6").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h1 = volume.get("h1").and_then(|v| v.as_f64()).unwrap_or(0.0);
            output.push_str(&format!("   Volume: 24h=${:.0} | 6h=${:.0} | 1h=${:.0}\n", h24, h6, h1));
        }

        // Price changes
        if let Some(price_change) = pair["priceChange"].as_object() {
            let h24 = price_change.get("h24").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h6 = price_change.get("h6").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h1 = price_change.get("h1").and_then(|v| v.as_f64()).unwrap_or(0.0);
            output.push_str(&format!("   Price Change: 24h={:+.1}% | 6h={:+.1}% | 1h={:+.1}%\n", h24, h6, h1));
        }

        // Liquidity
        if let Some(liq) = pair["liquidity"].as_object() {
            let usd = liq.get("usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
            output.push_str(&format!("   Liquidity: ${:.0}\n", usd));
        }

        // FDV
        if let Some(fdv) = pair["fdv"].as_f64() {
            output.push_str(&format!("   FDV: ${:.0}\n", fdv));
        }

        // Pair creation time
        if let Some(created) = pair["pairCreatedAt"].as_u64() {
            let secs = created / 1000;
            let age_hrs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(secs)) / 3600;
            if age_hrs < 24 {
                output.push_str(&format!("   Age: {} hours old [NEW]\n", age_hrs));
            } else {
                output.push_str(&format!("   Age: {} days old\n", age_hrs / 24));
            }
        }

        if !url.is_empty() {
            output.push_str(&format!("   URL: {}\n", url));
        }

        output.push('\n');
        count += 1;
    }

    output.push_str(&format!("Showing {} of {} total pairs found.\n", count, pairs.len()));
    output.push_str("\nNext step: Use dex_check_token with the contract address to run safety checks before trading.\n");

    Ok(output)
}

/// Get trending / recently boosted tokens from DexScreener.
/// No API key needed — uses public endpoints.
pub async fn execute_dex_trending(
    args: &serde_json::Value,
    _creds: &HashMap<String, String>,
) -> Result<String, String> {
    let chain_filter = args["chain"].as_str().unwrap_or("");
    let max_results = args["max_results"].as_u64().unwrap_or(20).min(50) as usize;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; PawAgent/1.0)")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut output = String::from("Trending Tokens\n\n");

    // 1. Token Boosts (recently promoted/trending on DexScreener)
    let boosts_url = "https://api.dexscreener.com/token-boosts/latest/v1";
    match client.get(boosts_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(boosts) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = boosts.as_array() {
                        output.push_str(&format!("Recently Boosted ({} tokens):\n", arr.len()));
                        let mut count = 0;
                        for boost in arr {
                            if count >= max_results { break; }

                            let chain = boost["chainId"].as_str().unwrap_or("?");
                            if !chain_filter.is_empty() && !chain.to_lowercase().contains(&chain_filter.to_lowercase()) {
                                continue;
                            }

                            let token_addr = boost["tokenAddress"].as_str().unwrap_or("?");
                            let description = boost["description"].as_str().unwrap_or("");
                            let url = boost["url"].as_str().unwrap_or("");
                            let amount = boost["amount"].as_f64().unwrap_or(0.0);

                            output.push_str(&format!("  {}. {} on {}\n", count + 1, token_addr, chain));
                            if !description.is_empty() {
                                output.push_str(&format!("     {}\n", crate::engine::types::truncate_utf8(description, 100)));
                            }
                            if amount > 0.0 {
                                output.push_str(&format!("     Boost amount: ${:.0}\n", amount));
                            }
                            if !url.is_empty() {
                                output.push_str(&format!("     {}\n", url));
                            }
                            count += 1;
                        }
                        if count == 0 {
                            output.push_str(&format!("  No boosted tokens found for chain '{}'\n", chain_filter));
                        }
                    }
                }
            }
        }
        Err(e) => { output.push_str(&format!("  Boosts API error: {}\n", e)); }
    }

    // 2. Token Profiles (latest token listings with metadata)
    let profiles_url = "https://api.dexscreener.com/token-profiles/latest/v1";
    match client.get(profiles_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(profiles) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = profiles.as_array() {
                        output.push_str(&format!("\nRecent Token Profiles ({} listings):\n", arr.len()));
                        let mut count = 0;
                        for profile in arr {
                            if count >= max_results { break; }

                            let chain = profile["chainId"].as_str().unwrap_or("?");
                            if !chain_filter.is_empty() && !chain.to_lowercase().contains(&chain_filter.to_lowercase()) {
                                continue;
                            }

                            let token_addr = profile["tokenAddress"].as_str().unwrap_or("?");
                            let description = profile["description"].as_str().unwrap_or("");
                            let url = profile["url"].as_str().unwrap_or("");

                            output.push_str(&format!("  {}. {} on {}\n", count + 1, token_addr, chain));
                            if !description.is_empty() {
                                let desc_trimmed = crate::engine::types::truncate_utf8(description, 120);
                                output.push_str(&format!("     {}\n", desc_trimmed));
                            }
                            if !url.is_empty() {
                                output.push_str(&format!("     {}\n", url));
                            }
                            count += 1;
                        }
                        if count == 0 {
                            output.push_str(&format!("  No profiles found for chain '{}'\n", chain_filter));
                        }
                    }
                }
            }
        }
        Err(e) => { output.push_str(&format!("  Profiles API error: {}\n", e)); }
    }

    output.push_str("\nNext steps:\n");
    output.push_str("1. Use dex_search_token to get price/volume/liquidity for interesting tokens\n");
    output.push_str("2. Use dex_check_token to run safety audit before trading\n");
    output.push_str("3. Use dex_top_traders to find who's trading them profitably\n");

    Ok(output)
}

/// Simple URL encoding for query parameters
pub(crate) fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push_str("%20"),
            _ => result.push_str(&format!("%{:02X}", b)),
        }
    }
    result
}
