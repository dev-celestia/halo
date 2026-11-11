use std::collections::HashMap;

use base64::{Engine, engine::general_purpose};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::encoding::{hex_encode, md5_hex, percent_decode, percent_encode};
use crate::markers::position_name;
use crate::types::{
    InvokerAttackConfig, InvokerAttackMode, InvokerPayloadConfig, InvokerPayloadPosition,
    InvokerPayloadProcessingStep, InvokerPayloadType,
};

pub fn format_number_payload(value: i64, format: Option<&str>) -> String {
    let format = format.unwrap_or("{}");
    if let Some(width_text) = format
        .strip_prefix("{:0")
        .and_then(|text| text.strip_suffix('}'))
    {
        if let Ok(width) = width_text.parse::<usize>() {
            return format!("{:0width$}", value, width = width);
        }
    }

    format.replace("{}", &value.to_string())
}

pub fn apply_payload_processing(
    mut value: String,
    steps: &[InvokerPayloadProcessingStep],
) -> String {
    for step in steps {
        value = match step {
            InvokerPayloadProcessingStep::UrlEncode => percent_encode(&value),
            InvokerPayloadProcessingStep::UrlDecode => percent_decode(&value).unwrap_or(value),
            InvokerPayloadProcessingStep::Base64Encode => {
                general_purpose::STANDARD.encode(value.as_bytes())
            }
            InvokerPayloadProcessingStep::Base64Decode => general_purpose::STANDARD
                .decode(value.as_bytes())
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or(value),
            InvokerPayloadProcessingStep::Md5Hash => md5_hex(value.as_bytes()),
            InvokerPayloadProcessingStep::Sha1Hash => {
                let mut hasher = Sha1::new();
                hasher.update(value.as_bytes());
                hex_encode(&hasher.finalize())
            }
            InvokerPayloadProcessingStep::Sha256Hash => {
                let mut hasher = Sha256::new();
                hasher.update(value.as_bytes());
                hex_encode(&hasher.finalize())
            }
        };
    }

    value
}

pub fn build_payload_source(config: &InvokerPayloadConfig) -> Result<Vec<String>, String> {
    let values = match config.payload_type {
        InvokerPayloadType::SimpleList => config.values.clone(),
        InvokerPayloadType::RuntimeFile => {
            if let Some(file_path) = &config.file_path {
                std::fs::read_to_string(file_path)
                    .map_err(|error| format!("Failed to read payload file: {}", error))?
                    .lines()
                    .map(str::to_string)
                    .collect()
            } else {
                config.values.clone()
            }
        }
        InvokerPayloadType::NumberRange => {
            let start = config.number_start.unwrap_or(0);
            let end = config.number_end.unwrap_or(100);
            let mut step = config.number_step.unwrap_or(1);
            if step == 0 {
                step = 1;
            }

            let mut values = Vec::new();
            let mut current = start;
            while if step > 0 {
                current <= end
            } else {
                current >= end
            } {
                values.push(format_number_payload(
                    current,
                    config.number_format.as_deref(),
                ));
                current += step;
                if values.len() > 250_000 {
                    return Err("Number range is too large".to_string());
                }
            }
            values
        }
    };

    Ok(values
        .into_iter()
        .map(|value| apply_payload_processing(value, &config.processing))
        .filter(|value| !value.is_empty())
        .collect())
}

pub fn build_invoker_payload_rows(
    config: &InvokerAttackConfig,
    position_count: usize,
) -> Result<Vec<HashMap<String, String>>, String> {
    if config
        .position_payloads
        .as_ref()
        .is_some_and(|position_payloads| !position_payloads.is_empty())
    {
        let sources = build_position_payload_sources(config, position_count)?;
        return Ok(generate_aligned_position_payloads(&sources));
    }

    let payload_source = build_payload_source(&config.payload_config)?;
    Ok(generate_invoker_payloads(
        config,
        &payload_source,
        position_count,
    ))
}

pub fn build_position_payload_sources(
    config: &InvokerAttackConfig,
    position_count: usize,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let Some(position_payloads) = config.position_payloads.as_ref() else {
        return Ok(Vec::new());
    };

    let mut sources = Vec::with_capacity(position_count);

    for position_index in 0..position_count {
        let name = position_name(&config.positions, position_index);
        let payload_config = position_payloads
            .get(&name)
            .ok_or_else(|| format!("Add payloads for {}", name))?;
        let source = build_payload_source(payload_config)?;

        if source.is_empty() {
            return Err(format!("Add payloads for {}", name));
        }

        sources.push((name, source));
    }

    Ok(sources)
}

pub fn generate_aligned_position_payloads(
    sources: &[(String, Vec<String>)],
) -> Vec<HashMap<String, String>> {
    let Some(row_count) = sources.iter().map(|(_, source)| source.len()).min() else {
        return Vec::new();
    };

    (0..row_count)
        .map(|row_index| {
            sources
                .iter()
                .map(|(name, source)| (name.clone(), source[row_index].clone()))
                .collect()
        })
        .collect()
}

pub fn generate_invoker_payloads(
    config: &InvokerAttackConfig,
    source: &[String],
    position_count: usize,
) -> Vec<HashMap<String, String>> {
    match config.mode {
        InvokerAttackMode::Sniper => {
            let mut rows = Vec::new();
            for position_index in 0..position_count {
                for payload in source {
                    let mut row = HashMap::new();
                    row.insert(
                        position_name(&config.positions, position_index),
                        payload.clone(),
                    );
                    rows.push(row);
                }
            }
            rows
        }
        InvokerAttackMode::BatteringRam => source
            .iter()
            .map(|payload| {
                (0..position_count)
                    .map(|index| (position_name(&config.positions, index), payload.clone()))
                    .collect()
            })
            .collect(),
        InvokerAttackMode::Pitchfork => source
            .iter()
            .map(|payload| {
                (0..position_count)
                    .map(|index| (position_name(&config.positions, index), payload.clone()))
                    .collect()
            })
            .collect(),
        InvokerAttackMode::ClusterBomb => {
            let mut rows = Vec::new();
            append_cluster_payloads(
                &mut rows,
                HashMap::new(),
                source,
                &config.positions,
                position_count,
                0,
            );
            rows
        }
    }
}

pub fn append_cluster_payloads(
    rows: &mut Vec<HashMap<String, String>>,
    current: HashMap<String, String>,
    source: &[String],
    positions: &[InvokerPayloadPosition],
    position_count: usize,
    position_index: usize,
) {
    if position_index == position_count {
        rows.push(current);
        return;
    }

    for payload in source {
        let mut next = current.clone();
        next.insert(position_name(positions, position_index), payload.clone());
        append_cluster_payloads(
            rows,
            next,
            source,
            positions,
            position_count,
            position_index + 1,
        );
        if rows.len() > 250_000 {
            return;
        }
    }
}
