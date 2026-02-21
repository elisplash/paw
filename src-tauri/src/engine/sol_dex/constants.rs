// Solana DEX — Constants
// KNOWN_TOKENS, API endpoints, TOKEN_PROGRAM_IDs, slippage defaults.

/// Well-known SPL tokens on Solana mainnet (symbol, mint_address, decimals)
pub(crate) const KNOWN_TOKENS: &[(&str, &str, u8)] = &[
    ("SOL",   "So11111111111111111111111111111111111111112",  9),
    ("USDC",  "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", 6),
    ("USDT",  "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",  6),
    ("BONK",  "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",  5),
    ("JUP",   "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",   6),
    ("RAY",   "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",  6),
    ("PYTH",  "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3",  6),
    ("WIF",   "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",  6),
    ("ORCA",  "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE",   6),
    ("MSOL",  "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So",   9),
    ("JITOSOL", "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn", 9),
];

/// Jupiter API base URL (Metis Swap API v1 — requires API key from jup.ag)
pub(crate) const JUPITER_API: &str = "https://api.jup.ag/swap/v1";

/// PumpPortal local-trade API — routes through pump.fun bonding curve + PumpSwap + Raydium
/// Returns a serialized versioned transaction for local signing (no private key sent)
pub(crate) const PUMPPORTAL_API: &str = "https://pumpportal.fun/api/trade-local";

/// Solana Token Program IDs
pub(crate) const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub(crate) const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// Default slippage tolerance (0.5% = 50 bps)
pub(crate) const DEFAULT_SLIPPAGE_BPS: u64 = 50;
/// Maximum allowed slippage (50% — needed for volatile meme/pump.fun tokens)
pub(crate) const MAX_SLIPPAGE_BPS: u64 = 5000;
