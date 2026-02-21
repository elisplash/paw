// Paw Agent Engine — DEX Whale / Wallet Monitoring

use super::abi::{decode_abi_string, encode_balance_of, encode_decimals, encode_symbol};
use super::constants::{chain_name, KNOWN_TOKENS, TRANSFER_EVENT_TOPIC};
use super::primitives::{hex_decode, parse_address, raw_to_amount};
use super::rpc::{chunked_get_logs, eth_call, eth_chain_id, eth_get_balance, rpc_call};
use std::collections::HashMap;
use crate::atoms::error::EngineResult;

/// Internal representation of a parsed ERC-20 Transfer event.
struct Transfer {
    block: u64,
    from: String,
    to: String,
    amount: f64,
    #[allow(dead_code)]
    amount_str: String,
    tx_hash: String,
}

/// Monitor a wallet address: show ETH balance, recent ERC-20 transfers (in/out),
/// and current holdings of known tokens.
pub async fn execute_dex_watch_wallet(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let wallet = args["wallet_address"].as_str()
        .ok_or("dex_watch_wallet: missing 'wallet_address'")?;
    let blocks_back = args["blocks_back"].as_u64().unwrap_or(1000);
    let addr_clean = wallet.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid wallet address: '{}'", addr_clean).into());
    }

    let mut output = format!("Wallet Monitor: {}\n\n", addr_clean);

    // 1. ETH balance
    match eth_get_balance(rpc_url, addr_clean).await {
        Ok(bal_hex) => {
            if let Ok(eth_bal) = raw_to_amount(&bal_hex, 18) {
                output.push_str(&format!("ETH Balance: {} ETH\n", eth_bal));
            }
        }
        Err(e) => { output.push_str(&format!("ETH Balance: error ({})\n", e)); }
    }

    // 2. Check known token balances
    let wallet_bytes = parse_address(addr_clean)?;
    output.push_str("\nToken Holdings:\n");
    let mut has_tokens = false;
    for (symbol, addr, decimals) in KNOWN_TOKENS {
        if *symbol == "ETH" { continue; }
        let call_data = encode_balance_of(&wallet_bytes);
        if let Ok(result) = eth_call(rpc_url, addr, &call_data).await {
            if let Ok(amount) = raw_to_amount(&result, *decimals) {
                let amt_f: f64 = amount.parse().unwrap_or(0.0);
                if amt_f > 0.0 {
                    output.push_str(&format!("  {}: {}\n", symbol, amount));
                    has_tokens = true;
                }
            }
        }
    }

    // Also check user-specified tokens
    if let Some(extra_tokens) = args["tokens"].as_array() {
        for token_val in extra_tokens {
            if let Some(token_addr) = token_val.as_str() {
                let token_addr = token_addr.trim();
                if token_addr.starts_with("0x") && token_addr.len() == 42 {
                    let call_data = encode_balance_of(&wallet_bytes);
                    if let Ok(result) = eth_call(rpc_url, token_addr, &call_data).await {
                        let symbol = match eth_call(rpc_url, token_addr, &encode_symbol()).await {
                            Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| token_addr[..10].to_string()),
                            Err(_) => token_addr[..10].to_string(),
                        };
                        let decimals = match eth_call(rpc_url, token_addr, &encode_decimals()).await {
                            Ok(d) => {
                                let b = hex_decode(&d).unwrap_or_default();
                                if b.len() >= 32 { b[31] } else { 18 }
                            }
                            Err(_) => 18,
                        };
                        if let Ok(amount) = raw_to_amount(&result, decimals) {
                            let amt_f: f64 = amount.parse().unwrap_or(0.0);
                            if amt_f > 0.0 {
                                output.push_str(&format!("  {}: {}\n", symbol, amount));
                                has_tokens = true;
                            }
                        }
                    }
                }
            }
        }
    }

    if !has_tokens {
        output.push_str("  (no known token holdings)\n");
    }

    // 3. Get current block number
    let block_num = match rpc_call(rpc_url, "eth_blockNumber", serde_json::json!([])).await {
        Ok(val) => {
            let hex = val.as_str().unwrap_or("0x0");
            u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
        }
        Err(e) => return Err(e),
    };
    let from_block = block_num.saturating_sub(blocks_back);

    output.push_str(&format!("\nRecent Transfers (last {} blocks, ~{} to #{}):\n",
        blocks_back, from_block, block_num));

    // Pad wallet to 32 bytes for topic filter
    let wallet_topic = format!("0x000000000000000000000000{}", &addr_clean[2..].to_lowercase());

    // Outgoing transfers (wallet is sender = topic[1])
    let outgoing_logs = chunked_get_logs(
        rpc_url,
        None,
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC)), Some(serde_json::json!(wallet_topic))],
        500,
    ).await;

    // Incoming transfers (wallet is receiver = topic[2])
    let incoming_logs = chunked_get_logs(
        rpc_url,
        None,
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC)), None, Some(serde_json::json!(wallet_topic))],
        500,
    ).await;

    let mut transfers: Vec<(u64, String, String, String, String, String)> = Vec::new();

    if let Ok(logs) = outgoing_logs {
        for log in logs.iter().take(50) {
            if let Some(parsed) = parse_transfer_log(log, "SELL/SEND", rpc_url).await {
                transfers.push(parsed);
            }
        }
    }

    if let Ok(logs) = incoming_logs {
        for log in logs.iter().take(50) {
            if let Some(parsed) = parse_transfer_log(log, "BUY/RECV", rpc_url).await {
                transfers.push(parsed);
            }
        }
    }

    // Sort by block number
    transfers.sort_by_key(|t| t.0);

    if transfers.is_empty() {
        output.push_str("  No ERC-20 transfers found in this range.\n");
    } else {
        for (block, direction, token_addr, counterparty, amount, symbol) in &transfers {
            output.push_str(&format!("  Block {} | {} | {} {} | {} | counterparty: {}\n",
                block, direction, amount, symbol,
                &token_addr[..10.min(token_addr.len())],
                &counterparty[..10.min(counterparty.len())]));
        }
        output.push_str(&format!("\n  Total: {} transfers found\n", transfers.len()));
    }

    // Chain info
    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = chain_name(chain_id);
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, chain_id));
    }

    Ok(output)
}

/// Scan recent large transfers of a specific token to detect whale activity.
pub async fn execute_dex_whale_transfers(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_whale_transfers: missing 'token_address'")?;
    let blocks_back = args["blocks_back"].as_u64().unwrap_or(2000);
    let min_amount_str = args["min_amount"].as_str().unwrap_or("0");

    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid token address: '{}'", addr_clean).into());
    }

    // Get token info
    let symbol = match eth_call(rpc_url, addr_clean, &encode_symbol()).await {
        Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| "???".into()),
        Err(_) => "???".into(),
    };
    let decimals = match eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        Ok(d) => {
            let b = hex_decode(&d).unwrap_or_default();
            if b.len() >= 32 { b[31] } else { 18 }
        }
        Err(_) => 18,
    };

    let mut output = format!("Whale Transfer Scanner: {} ({})\nContract: {}\n\n", symbol, decimals, addr_clean);

    // Get current block
    let block_num = match rpc_call(rpc_url, "eth_blockNumber", serde_json::json!([])).await {
        Ok(val) => {
            let hex = val.as_str().unwrap_or("0x0");
            u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
        }
        Err(e) => return Err(e),
    };
    let from_block = block_num.saturating_sub(blocks_back);

    // Get all Transfer events for this token
    let log_arr = chunked_get_logs(
        rpc_url,
        Some(addr_clean),
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC))],
        500,
    ).await?;

    if log_arr.is_empty() {
        output.push_str(&format!("No transfers found in last {} blocks.\n", blocks_back));
        return Ok(output);
    }

    let min_amount_f: f64 = min_amount_str.parse().unwrap_or(0.0);

    let mut transfers: Vec<Transfer> = Vec::new();
    let mut accumulation_map: HashMap<String, f64> = HashMap::new();
    let mut total_volume = 0.0f64;

    for log in &log_arr {
        let topics = match log["topics"].as_array() {
            Some(t) if t.len() >= 3 => t,
            _ => continue,
        };

        let from_topic = topics[1].as_str().unwrap_or("");
        let to_topic = topics[2].as_str().unwrap_or("");
        let data = log["data"].as_str().unwrap_or("0x");
        let block_hex = log["blockNumber"].as_str().unwrap_or("0x0");
        let tx_hash = log["transactionHash"].as_str().unwrap_or("").to_string();
        let block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).unwrap_or(0);

        let amount_str = match raw_to_amount(data, decimals) {
            Ok(a) => a,
            Err(_) => continue,
        };
        let amount_f: f64 = amount_str.parse().unwrap_or(0.0);
        if amount_f < min_amount_f { continue; }

        let from_addr = if from_topic.len() >= 42 {
            format!("0x{}", &from_topic[from_topic.len()-40..])
        } else { "0x?".into() };
        let to_addr = if to_topic.len() >= 42 {
            format!("0x{}", &to_topic[to_topic.len()-40..])
        } else { "0x?".into() };

        total_volume += amount_f;
        let from_lower = from_addr.to_lowercase();
        let to_lower = to_addr.to_lowercase();
        *accumulation_map.entry(to_lower.clone()).or_insert(0.0) += amount_f;
        *accumulation_map.entry(from_lower.clone()).or_insert(0.0) -= amount_f;

        transfers.push(Transfer {
            block, from: from_addr, to: to_addr,
            amount: amount_f, amount_str, tx_hash,
        });
    }

    // Sort by amount descending
    transfers.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));

    output.push_str(&format!("Scanned blocks {} → {} ({} blocks)\n", from_block, block_num, blocks_back));
    output.push_str(&format!("Total transfers found: {}\n", transfers.len()));
    output.push_str(&format!("Total volume: {} {}\n\n", format_large_number(total_volume), symbol));

    output.push_str("Largest Transfers:\n");
    for (i, t) in transfers.iter().take(20).enumerate() {
        output.push_str(&format!("  {}. {} {} | {} → {} | block {} | tx: {}...\n",
            i + 1,
            format_large_number(t.amount), symbol,
            &t.from[..8.min(t.from.len())], &t.to[..8.min(t.to.len())],
            t.block,
            if t.tx_hash.len() > 14 { &t.tx_hash[..14] } else { &t.tx_hash },
        ));
    }

    // Top accumulators and distributors
    let mut accumulators: Vec<(String, f64)> = Vec::new();
    let mut distributors: Vec<(String, f64)> = Vec::new();
    for (addr, net) in &accumulation_map {
        if addr.contains("000000000000000000000000000000000") { continue; }
        if *net > 0.0 {
            accumulators.push((addr.clone(), *net));
        } else if *net < 0.0 {
            distributors.push((addr.clone(), net.abs()));
        }
    }
    accumulators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    distributors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if !accumulators.is_empty() {
        output.push_str("\nTop Accumulators (net buyers — potential smart money):\n");
        for (i, (addr, net)) in accumulators.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} | net +{} {}\n",
                i + 1, addr, format_large_number(*net), symbol));
        }
    }

    if !distributors.is_empty() {
        output.push_str("\nTop Distributors (net sellers — potential exit signals):\n");
        for (i, (addr, net)) in distributors.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} | net -{} {}\n",
                i + 1, addr, format_large_number(*net), symbol));
        }
    }

    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = chain_name(chain_id);
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, chain_id));
    }

    output.push_str("\nTip: Use dex_watch_wallet on top accumulator addresses to see their full portfolio and trading history.\n");

    Ok(output)
}

/// Analyze on-chain Transfer events for a token to identify the most profitable wallets.
pub async fn execute_dex_top_traders(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_top_traders: missing 'token_address'")?;
    let blocks_back = args["blocks_back"].as_u64().unwrap_or(5000);
    let min_trades = args["min_trades"].as_u64().unwrap_or(2) as usize;

    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid token address: '{}'", addr_clean).into());
    }

    let symbol = match eth_call(rpc_url, addr_clean, &encode_symbol()).await {
        Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| "???".into()),
        Err(_) => "???".into(),
    };
    let decimals = match eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        Ok(d) => {
            let b = hex_decode(&d).unwrap_or_default();
            if b.len() >= 32 { b[31] } else { 18 }
        }
        Err(_) => 18,
    };

    let mut output = format!("Top Traders Analysis: {} ({})\nContract: {}\n\n", symbol, decimals, addr_clean);

    let block_num = match rpc_call(rpc_url, "eth_blockNumber", serde_json::json!([])).await {
        Ok(val) => {
            let hex = val.as_str().unwrap_or("0x0");
            u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
        }
        Err(e) => return Err(e),
    };
    let from_block = block_num.saturating_sub(blocks_back);

    let log_arr = chunked_get_logs(
        rpc_url,
        Some(addr_clean),
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC))],
        500,
    ).await?;

    if log_arr.is_empty() {
        output.push_str(&format!("No transfers found in last {} blocks.\n", blocks_back));
        return Ok(output);
    }

    struct WalletProfile {
        address: String,
        total_bought: f64,
        total_sold: f64,
        buy_count: usize,
        sell_count: usize,
        first_seen_block: u64,
        last_seen_block: u64,
    }

    let mut profiles: HashMap<String, WalletProfile> = HashMap::new();
    let zero_addr = "0x0000000000000000000000000000000000000000";

    for log in &log_arr {
        let topics = match log["topics"].as_array() {
            Some(t) if t.len() >= 3 => t,
            _ => continue,
        };

        let from_topic = topics[1].as_str().unwrap_or("");
        let to_topic = topics[2].as_str().unwrap_or("");
        let data = log["data"].as_str().unwrap_or("0x");
        let block_hex = log["blockNumber"].as_str().unwrap_or("0x0");
        let block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).unwrap_or(0);

        let amount_str = match raw_to_amount(data, decimals) {
            Ok(a) => a,
            Err(_) => continue,
        };
        let amount_f: f64 = amount_str.parse().unwrap_or(0.0);
        if amount_f == 0.0 { continue; }

        let from_addr = if from_topic.len() >= 42 {
            format!("0x{}", &from_topic[from_topic.len()-40..]).to_lowercase()
        } else { continue };
        let to_addr = if to_topic.len() >= 42 {
            format!("0x{}", &to_topic[to_topic.len()-40..]).to_lowercase()
        } else { continue };

        if from_addr == zero_addr || to_addr == zero_addr { continue; }

        let buyer = profiles.entry(to_addr.clone()).or_insert(WalletProfile {
            address: to_addr.clone(),
            total_bought: 0.0, total_sold: 0.0,
            buy_count: 0, sell_count: 0,
            first_seen_block: block, last_seen_block: block,
        });
        buyer.total_bought += amount_f;
        buyer.buy_count += 1;
        if block < buyer.first_seen_block { buyer.first_seen_block = block; }
        if block > buyer.last_seen_block { buyer.last_seen_block = block; }

        let seller = profiles.entry(from_addr.clone()).or_insert(WalletProfile {
            address: from_addr.clone(),
            total_bought: 0.0, total_sold: 0.0,
            buy_count: 0, sell_count: 0,
            first_seen_block: block, last_seen_block: block,
        });
        seller.total_sold += amount_f;
        seller.sell_count += 1;
        if block < seller.first_seen_block { seller.first_seen_block = block; }
        if block > seller.last_seen_block { seller.last_seen_block = block; }
    }

    struct TraderScore {
        address: String,
        total_bought: f64,
        total_sold: f64,
        net_pnl_tokens: f64,
        trade_count: usize,
        win_indicator: f64,
        first_block: u64,
        #[allow(dead_code)]
        last_block: u64,
        still_holding: f64,
        trader_type: String,
    }

    let mut scored: Vec<TraderScore> = Vec::new();

    for p in profiles.values() {
        let total_trades = p.buy_count + p.sell_count;
        if total_trades < min_trades { continue; }

        let net = p.total_sold - p.total_bought;
        let still_holding = p.total_bought - p.total_sold;
        let win_indicator = if p.total_bought > 0.0 { p.total_sold / p.total_bought } else { 0.0 };

        let trader_type = if p.sell_count == 0 && p.buy_count > 0 {
            "Accumulator".to_string()
        } else if win_indicator > 1.5 {
            "Profit Taker".to_string()
        } else if win_indicator > 0.8 && win_indicator <= 1.5 {
            "Rotator".to_string()
        } else if p.buy_count > 0 && p.sell_count > 0 && total_trades > 5 {
            "Active Trader".to_string()
        } else if p.first_seen_block <= from_block + (blocks_back / 10) {
            "Early Buyer".to_string()
        } else {
            "Trader".to_string()
        };

        scored.push(TraderScore {
            address: p.address.clone(),
            total_bought: p.total_bought,
            total_sold: p.total_sold,
            net_pnl_tokens: net,
            trade_count: total_trades,
            win_indicator,
            first_block: p.first_seen_block,
            last_block: p.last_seen_block,
            still_holding,
            trader_type,
        });
    }

    scored.sort_by(|a, b| b.net_pnl_tokens.partial_cmp(&a.net_pnl_tokens).unwrap_or(std::cmp::Ordering::Equal));

    output.push_str(&format!("Scanned blocks {} → {} ({} blocks, {} transfers)\n", from_block, block_num, blocks_back, log_arr.len()));
    output.push_str(&format!("Unique traders: {} (min {} trades filter)\n\n", scored.len(), min_trades));

    output.push_str("Top Profit Takers (sold more than bought — realized gains):\n");
    let profit_takers: Vec<&TraderScore> = scored.iter().filter(|s| s.net_pnl_tokens > 0.0).take(15).collect();
    if profit_takers.is_empty() {
        output.push_str("  None found — all traders are still accumulating\n");
    } else {
        for (i, t) in profit_takers.iter().enumerate() {
            output.push_str(&format!("  {}. {} [{}]\n", i + 1, t.address, t.trader_type));
            output.push_str(&format!("     Bought: {} {} | Sold: {} {} | Net: +{} {}\n",
                format_large_number(t.total_bought), symbol,
                format_large_number(t.total_sold), symbol,
                format_large_number(t.net_pnl_tokens), symbol));
            output.push_str(&format!("     Trades: {} | Sell/Buy ratio: {:.2}x | First block: {}\n",
                t.trade_count, t.win_indicator, t.first_block));
        }
    }

    output.push_str("\nTop Accumulators (bought more than sold — still holding):\n");
    let mut accumulators: Vec<&TraderScore> = scored.iter().filter(|s| s.still_holding > 0.0).collect();
    accumulators.sort_by(|a, b| b.still_holding.partial_cmp(&a.still_holding).unwrap_or(std::cmp::Ordering::Equal));

    if accumulators.is_empty() {
        output.push_str("  None found\n");
    } else {
        for (i, t) in accumulators.iter().take(15).enumerate() {
            output.push_str(&format!("  {}. {} [{}]\n", i + 1, t.address, t.trader_type));
            output.push_str(&format!("     Bought: {} {} | Sold: {} {} | Holding: {} {}\n",
                format_large_number(t.total_bought), symbol,
                format_large_number(t.total_sold), symbol,
                format_large_number(t.still_holding), symbol));
            output.push_str(&format!("     Trades: {} | First seen: block {}\n",
                t.trade_count, t.first_block));
        }
    }

    output.push_str("\nEarly Smart Money (first 10% of blocks AND took profit):\n");
    let early_cutoff = from_block + (blocks_back / 10);
    let mut early_winners: Vec<&TraderScore> = scored.iter()
        .filter(|s| s.first_block <= early_cutoff && s.net_pnl_tokens > 0.0)
        .collect();
    early_winners.sort_by(|a, b| b.net_pnl_tokens.partial_cmp(&a.net_pnl_tokens).unwrap_or(std::cmp::Ordering::Equal));

    if early_winners.is_empty() {
        output.push_str("  No early profit-takers found in this range\n");
    } else {
        for (i, t) in early_winners.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} — in at block {}, net +{} {}, {} trades\n",
                i + 1, t.address, t.first_block,
                format_large_number(t.net_pnl_tokens), symbol, t.trade_count));
        }
        output.push_str("\n  ^ These wallets got in early AND profited. Watch them with dex_watch_wallet.\n");
    }

    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = chain_name(chain_id);
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, chain_id));
    }

    output.push_str("\nNext: Use dex_watch_wallet on promising addresses to see their full portfolio across tokens.\n");

    Ok(output)
}

/// Helper: parse a single Transfer event log into a transfer tuple.
async fn parse_transfer_log(
    log: &serde_json::Value,
    direction: &str,
    rpc_url: &str,
) -> Option<(u64, String, String, String, String, String)> {
    let topics = log["topics"].as_array()?;
    if topics.len() < 3 { return None; }

    let token_addr = log["address"].as_str()?.to_string();
    let block_hex = log["blockNumber"].as_str()?;
    let block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).ok()?;
    let data = log["data"].as_str()?;

    let counterparty = if direction.contains("SELL") || direction.contains("SEND") {
        let to_topic = topics[2].as_str()?;
        if to_topic.len() >= 42 {
            format!("0x{}", &to_topic[to_topic.len()-40..])
        } else { "0x?".into() }
    } else {
        let from_topic = topics[1].as_str()?;
        if from_topic.len() >= 42 {
            format!("0x{}", &from_topic[from_topic.len()-40..])
        } else { "0x?".into() }
    };

    let symbol = match eth_call(rpc_url, &token_addr, &encode_symbol()).await {
        Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| token_addr[..8.min(token_addr.len())].to_string()),
        Err(_) => token_addr[..8.min(token_addr.len())].to_string(),
    };

    let decimals = match eth_call(rpc_url, &token_addr, &encode_decimals()).await {
        Ok(d) => {
            let b = hex_decode(&d).unwrap_or_default();
            if b.len() >= 32 { b[31] } else { 18 }
        }
        Err(_) => 18,
    };

    let amount = raw_to_amount(data, decimals).unwrap_or_else(|_| "?".into());

    Some((block, direction.to_string(), token_addr, counterparty, amount, symbol))
}

/// Format a large number with K/M/B suffix for readability.
fn format_large_number(n: f64) -> String {
    if n >= 1_000_000_000.0 {
        format!("{:.2}B", n / 1_000_000_000.0)
    } else if n >= 1_000_000.0 {
        format!("{:.2}M", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.2}K", n / 1_000.0)
    } else if n >= 1.0 {
        format!("{:.4}", n)
    } else {
        format!("{:.8}", n)
    }
}
