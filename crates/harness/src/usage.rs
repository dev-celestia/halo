//! Context-window size for the composer meter.
//!
//! Pi reports `model.contextWindow`. The other harnesses report token counts
//! without a window, and the meter hides itself when that window is zero.
//! This table is the same fallback Pi already uses, shared so every harness
//! can fill the chip.

/// One turn's context fill, ready for `AgentEvent::Usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMeter {
    pub used_tokens: u64,
    pub window_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Best-effort window when a harness does not report `contextWindow`.
pub fn context_window_for(model_key: &str) -> u64 {
    let id = model_key.to_ascii_lowercase();
    if id.contains("claude") || id.contains("anthropic") {
        200_000
    } else if id.contains("gemini") {
        1_000_000
    } else if id.contains("gpt-4.1") || id.contains("gpt-5") {
        1_000_000
    } else if id.contains("composer") {
        200_000
    } else if id.contains("deepseek") {
        128_000
    } else {
        128_000
    }
}

/// Pair input, output, and extra (cache, reasoning) with a window.
///
/// `window` wins when the harness measured it. Otherwise the model id selects
/// the fallback. A turn with no tokens stays hidden.
pub fn meter(
    model_key: &str,
    input: u64,
    output: u64,
    extra: u64,
    window: Option<u64>,
) -> Option<ContextMeter> {
    let used = input.saturating_add(output).saturating_add(extra);
    occupied(model_key, used, input, output, window)
}

/// `used` is already the context occupancy (ACP `usage_update.used`).
pub fn occupied(
    model_key: &str,
    used: u64,
    input: u64,
    output: u64,
    window: Option<u64>,
) -> Option<ContextMeter> {
    let used = if used == 0 {
        input.saturating_add(output)
    } else {
        used
    };
    if used == 0 {
        return None;
    }
    let window = window
        .filter(|size| *size > 0)
        .unwrap_or_else(|| context_window_for(model_key));
    if window == 0 {
        return None;
    }
    let (input, output) = if input == 0 && output == 0 {
        (used, 0)
    } else {
        (input, output)
    };
    Some(ContextMeter {
        used_tokens: used.min(window.saturating_mul(2)),
        window_tokens: window,
        input_tokens: input,
        output_tokens: output,
    })
}

/// JSON number that may arrive as an integer or a whole float.
pub fn json_count(value: Option<&serde_json::Value>) -> u64 {
    let Some(value) = value else {
        return 0;
    };
    if let Some(count) = value.as_u64() {
        return count;
    }
    value
        .as_f64()
        .filter(|count| count.is_finite() && *count >= 0.0)
        .map(|count| count as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_family_picks_a_window() {
        assert_eq!(context_window_for("anthropic/claude-sonnet-5-5"), 200_000);
        assert_eq!(context_window_for("gemini-4-argon"), 1_000_000);
        assert_eq!(context_window_for("composer-2.5"), 200_000);
        assert_eq!(context_window_for("auto"), 128_000);
    }

    #[test]
    fn a_turn_without_tokens_stays_hidden() {
        assert!(meter("claude", 0, 0, 0, None).is_none());
    }

    #[test]
    fn counted_tokens_fill_the_window() {
        let meter = meter("claude-sonnet", 1_000, 200, 50, None).unwrap();
        assert_eq!(meter.used_tokens, 1_250);
        assert_eq!(meter.window_tokens, 200_000);
        assert_eq!(meter.input_tokens, 1_000);
        assert_eq!(meter.output_tokens, 200);
    }

    #[test]
    fn a_reported_window_wins_over_the_fallback() {
        let meter = occupied("", 53_000, 0, 0, Some(200_000)).unwrap();
        assert_eq!(meter.window_tokens, 200_000);
        assert_eq!(meter.used_tokens, 53_000);
        assert_eq!(meter.input_tokens, 53_000);
    }
}
