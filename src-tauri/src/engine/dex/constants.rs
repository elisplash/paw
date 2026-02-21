// Paw Agent Engine — DEX Constants
// Well-known token addresses, contract addresses, and configuration defaults.

/// Well-known ERC-20 tokens on Ethereum mainnet
pub(crate) const KNOWN_TOKENS: &[(&str, &str, u8)] = &[
    ("ETH",  "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE", 18),
    ("WETH", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", 18),
    ("USDC", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 6),
    ("USDT", "0xdAC17F958D2ee523a2206206994597C13D831ec7", 6),
    ("DAI",  "0x6B175474E89094C44Da98b954EedeAC495271d0F", 18),
    ("WBTC", "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", 8),
    ("UNI",  "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984", 18),
    ("LINK", "0x514910771AF9Ca656af840dff83E8264EcF986CA", 18),
    ("PEPE", "0x6982508145454Ce325dDbE47a25d4ec3d2311933", 18),
    ("SHIB", "0x95aD61b0a150d79219dCF64E1E6Cc01f0B64C4cE", 18),
    ("ARB",  "0xB50721BCf8d664c30412Cfbc6cf7a15145234ad1", 18),
    ("AAVE", "0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9", 18),
];

/// Uniswap V3 contract addresses (Ethereum mainnet)
pub(crate) const UNISWAP_QUOTER_V2: &str = "0x61fFE014bA17989E743c5F6cB21bF9697530B21e";
pub(crate) const UNISWAP_SWAP_ROUTER_02: &str = "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45";
pub(crate) const WETH_ADDRESS: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";

/// Default slippage tolerance (0.5%)
pub(crate) const DEFAULT_SLIPPAGE_BPS: u64 = 50;
/// Maximum allowed slippage (5%)
pub(crate) const MAX_SLIPPAGE_BPS: u64 = 500;
/// Default fee tier for Uniswap V3 (0.3%)
pub(crate) const DEFAULT_FEE_TIER: u64 = 3000;

/// ERC-20 Transfer event topic: keccak256("Transfer(address,address,uint256)")
pub(crate) const TRANSFER_EVENT_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa0952e8da11aeba5c4f27ead9083c756cc2";

/// Returns the block explorer base TX URL for a given EVM chain ID.
/// Used to build transaction links in tool output.
pub(crate) fn explorer_tx_url(chain_id: u64) -> &'static str {
    match chain_id {
        1 => "https://etherscan.io/tx/",
        5 => "https://goerli.etherscan.io/tx/",
        11155111 => "https://sepolia.etherscan.io/tx/",
        137 => "https://polygonscan.com/tx/",
        42161 => "https://arbiscan.io/tx/",
        10 => "https://optimistic.etherscan.io/tx/",
        8453 => "https://basescan.org/tx/",
        _ => "https://etherscan.io/tx/",
    }
}

/// Returns a human-readable network name for a given EVM chain ID.
pub(crate) fn chain_name(chain_id: u64) -> &'static str {
    match chain_id {
        1 => "Ethereum Mainnet",
        5 => "Goerli Testnet",
        11155111 => "Sepolia Testnet",
        137 => "Polygon",
        42161 => "Arbitrum One",
        10 => "Optimism",
        8453 => "Base",
        _ => "Unknown",
    }
}
