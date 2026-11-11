use regex::Regex;

use crate::types::{
    InvokerGrepExtractConfig, InvokerGrepMatchConfig, InvokerSessionHandlingConfig,
};

pub fn response_matches(body: &str, config: &InvokerGrepMatchConfig) -> bool {
    if !config.enabled || config.keyword.is_empty() {
        return false;
    }

    if config.case_sensitive {
        body.contains(&config.keyword)
    } else {
        body.to_lowercase().contains(&config.keyword.to_lowercase())
    }
}

pub fn extract_grep_value(body: &str, config: &InvokerGrepExtractConfig) -> Option<String> {
    if !config.enabled || config.regex.is_empty() {
        return None;
    }

    let regex = Regex::new(&config.regex).ok()?;
    let captures = regex.captures(body)?;
    if let Some(replacement) = &config.replacement {
        return Some(regex.replace(body, replacement.as_str()).to_string());
    }

    captures
        .get(1)
        .or_else(|| captures.get(0))
        .map(|match_| match_.as_str().to_string())
}

pub fn extract_session_value(body: &str, config: &InvokerSessionHandlingConfig) -> Option<String> {
    if let Some(pattern) = config
        .extract_from_response
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        let regex = Regex::new(pattern).ok()?;
        return regex
            .captures(body)
            .and_then(|captures| captures.get(1).or_else(|| captures.get(0)))
            .map(|match_| match_.as_str().to_string());
    }

    let token_name = config.extract_token_name.as_ref()?.trim();
    if token_name.is_empty() {
        return None;
    }

    let pattern = format!(
        r#"(?i){}\s*["'=:\s]+\s*["']?([^"'<>\s&;]+)"#,
        regex::escape(token_name)
    );
    let regex = Regex::new(&pattern).ok()?;
    regex
        .captures(body)
        .and_then(|captures| captures.get(1))
        .map(|match_| match_.as_str().to_string())
}
