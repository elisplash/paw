// Paw Agent Engine — Model pricing & task complexity
// Extracted from engine/types.rs.
// ModelPrice struct lives in crate::atoms::types.

use crate::atoms::types::*;

pub fn model_price(model: &str) -> ModelPrice {
    // Normalize: strip provider prefixes like "anthropic/"
    let m = model.split('/').next_back().unwrap_or(model);
    match m {
        // Anthropic
        s if s.starts_with("claude-3-haiku") => ModelPrice { input: 0.25, output: 1.25 },
        s if s.starts_with("claude-haiku-4") => ModelPrice { input: 1.00, output: 5.00 },
        s if s.starts_with("claude-sonnet-4") || s.starts_with("claude-3-5-sonnet") || s.starts_with("claude-3-sonnet") =>
            ModelPrice { input: 3.00, output: 15.00 },
        s if s.starts_with("claude-opus-4") || s.starts_with("claude-3-opus") =>
            ModelPrice { input: 15.00, output: 75.00 },
        // Google
        s if s.starts_with("gemini-3.1-pro") =>
            ModelPrice { input: 2.50, output: 15.00 },
        s if s.starts_with("gemini-3-deep-think") =>
            ModelPrice { input: 5.00, output: 25.00 },
        s if s.starts_with("gemini-3-flash") =>
            ModelPrice { input: 0.20, output: 0.80 },
        s if s.starts_with("gemini-2.5-flash-lite") =>
            ModelPrice { input: 0.05, output: 0.20 },
        s if s.starts_with("gemini-2.0-flash") || s.starts_with("gemini-2.5-flash") =>
            ModelPrice { input: 0.15, output: 0.60 },
        s if s.starts_with("gemini-2.5-pro") || s.starts_with("gemini-1.5-pro") || s.starts_with("gemini-pro") =>
            ModelPrice { input: 1.25, output: 10.00 },
        // OpenAI
        s if s.starts_with("gpt-4o-mini") || s.starts_with("gpt-4.1-mini") || s.starts_with("gpt-4.1-nano") =>
            ModelPrice { input: 0.15, output: 0.60 },
        s if s.starts_with("gpt-4o") || s.starts_with("gpt-4.1") =>
            ModelPrice { input: 2.50, output: 10.00 },
        s if s.starts_with("o4-mini") || s.starts_with("o3-mini") =>
            ModelPrice { input: 1.10, output: 4.40 },
        s if s.starts_with("o3") || s.starts_with("o1") =>
            ModelPrice { input: 10.00, output: 40.00 },
        // DeepSeek
        s if s.starts_with("deepseek-chat") || s.starts_with("deepseek-v3") =>
            ModelPrice { input: 0.27, output: 1.10 },
        s if s.starts_with("deepseek-reasoner") || s.starts_with("deepseek-r1") =>
            ModelPrice { input: 0.55, output: 2.19 },
        // Fallback: assume cheap model
        _ => ModelPrice { input: 0.50, output: 2.00 },
    }
}

/// Estimate USD cost given token counts and model name.
/// Accounts for Anthropic cache tokens: reads charged at 10%, creation at 25%.
pub fn estimate_cost_usd(model: &str, input: u64, output: u64, cache_read: u64, cache_create: u64) -> f64 {
    let p = model_price(model);
    // Regular input tokens (subtract cached from total input for accurate costing)
    let regular_input = input.saturating_sub(cache_read + cache_create);
    let input_cost = (regular_input as f64 * p.input / 1_000_000.0)
        + (cache_read as f64 * p.input * 0.10 / 1_000_000.0)   // 90% discount
        + (cache_create as f64 * p.input * 0.25 / 1_000_000.0); // 75% discount on write
    let output_cost = output as f64 * p.output / 1_000_000.0;
    input_cost + output_cost
}

// ── Task Complexity Classification ─────────────────────────────────────

/// How complex a user message is — determines model tier.
/// Classify a user message's complexity to choose the right model tier.
/// Looks for signals of multi-step reasoning, code, analysis, etc.
pub fn classify_task_complexity(message: &str) -> TaskComplexity {
    let msg = message.to_lowercase();
    let len = msg.len();

    // Long messages are usually complex
    if len > 1500 { return TaskComplexity::Complex; }

    // Code-related signals
    let code_signals = [
        "write code", "implement", "refactor", "debug", "fix the bug",
        "create a function", "write a script", "build a", "architect",
        "```", "code review", "unit test", "write test",
        "optimize", "performance", "algorithm",
    ];

    // Analysis / reasoning signals
    let reasoning_signals = [
        "analyze", "compare", "explain why", "reason", "think through",
        "pros and cons", "trade-off", "evaluate", "assess",
        "plan", "strategy", "design", "architecture",
        "step by step", "break down", "complex",
        "research", "investigate", "deep dive",
        "write a report", "summarize", "synthesis",
    ];

    // Multi-step signals
    let multi_step = [
        "and then", "after that", "first,", "second,", "third,",
        "steps:", "1.", "2.", "3.",
        "multiple", "several", "all of",
    ];

    for signal in code_signals.iter().chain(reasoning_signals.iter()).chain(multi_step.iter()) {
        if msg.contains(signal) {
            return TaskComplexity::Complex;
        }
    }

    TaskComplexity::Simple
}

