//! JWT toolkit — decode, vulnerability audit, and HMAC signing.
//!
//! Pure client-side like the reference implementation (`pages/jwt`): no
//! backend commands, all computation happens here on the UI thread (token
//! decoding is cheap; signing is bounded by the secret length).

use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha384, Sha512};

use super::*;
use serde::{Deserialize, Serialize};

// ── Public DTOs ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JwtMode {
    #[default]
    Decode,
    Generate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JwtSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JwtAlgorithm {
    HS256,
    HS384,
    HS512,
    RS256,
    RS384,
    RS512,
    ES256,
    ES384,
    ES512,
    PS256,
    PS384,
    PS512,
    None,
}

impl JwtAlgorithm {
    pub const ALL: [JwtAlgorithm; 13] = [
        Self::HS256,
        Self::HS384,
        Self::HS512,
        Self::RS256,
        Self::RS384,
        Self::RS512,
        Self::ES256,
        Self::ES384,
        Self::ES512,
        Self::PS256,
        Self::PS384,
        Self::PS512,
        Self::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            other => match other {
                Self::HS256 => "HS256",
                Self::HS384 => "HS384",
                Self::HS512 => "HS512",
                Self::RS256 => "RS256",
                Self::RS384 => "RS384",
                Self::RS512 => "RS512",
                Self::ES256 => "ES256",
                Self::ES384 => "ES384",
                Self::ES512 => "ES512",
                Self::PS256 => "PS256",
                Self::PS384 => "PS384",
                Self::PS512 => "PS512",
                Self::None => "none",
            },
        }
    }

    /// Category shown as the group label in the picker.
    pub fn category(self) -> &'static str {
        match self {
            Self::HS256 | Self::HS384 | Self::HS512 => "HMAC",
            Self::RS256 | Self::RS384 | Self::RS512 => "RSA",
            Self::ES256 | Self::ES384 | Self::ES512 => "ECDSA",
            Self::PS256 | Self::PS384 | Self::PS512 => "RSA-PSS",
            Self::None => "None",
        }
    }

    pub fn is_hmac(self) -> bool {
        matches!(self, Self::HS256 | Self::HS384 | Self::HS512)
    }

    pub fn is_asymmetric(self) -> bool {
        matches!(
            self,
            Self::RS256
                | Self::RS384
                | Self::RS512
                | Self::ES256
                | Self::ES384
                | Self::ES512
                | Self::PS256
                | Self::PS384
                | Self::PS512
        )
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.label() == value)
    }
}

/// One decoded claim with a pre-rendered display value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwtClaim {
    pub key: String,
    /// JSON-encoded value (as written in the token).
    pub value_json: String,
    /// `string` | `number` | `boolean` | `null` | `object` — drives value color.
    pub value_kind: String,
    /// Humanized `iat` / `exp` / `nbf` timestamps ("2026-01-01 12:00:00 UTC · expired 3d ago").
    pub timestamp: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwtVulnerability {
    pub severity: JwtSeverity,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwtDecoded {
    pub algorithm: String,
    pub header: Vec<JwtClaim>,
    pub payload: Vec<JwtClaim>,
    pub signature_b64: String,
}

/// Everything the JWT page renders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwtView {
    pub mode: JwtMode,
    pub decoded: Option<JwtDecoded>,
    pub decode_error: Option<String>,
    pub vulnerabilities: Vec<JwtVulnerability>,
    pub gen_header: String,
    pub gen_payload: String,
    pub gen_secret: String,
    pub gen_algorithm: JwtAlgorithm,
    pub generated_token: String,
    pub gen_error: Option<String>,
    pub show_secret: bool,
}

pub const JWT_DEFAULT_HEADER: &str = "{\n  \"alg\": \"HS256\",\n  \"typ\": \"JWT\"\n}";
pub const JWT_DEFAULT_PAYLOAD: &str =
    "{\n  \"sub\": \"1234567890\",\n  \"name\": \"John Doe\",\n  \"iat\": 1516239022\n}";

impl Default for JwtView {
    fn default() -> Self {
        Self {
            mode: JwtMode::Decode,
            decoded: None,
            decode_error: None,
            vulnerabilities: Vec::new(),
            gen_header: JWT_DEFAULT_HEADER.to_string(),
            gen_payload: JWT_DEFAULT_PAYLOAD.to_string(),
            gen_secret: String::new(),
            gen_algorithm: JwtAlgorithm::HS256,
            generated_token: String::new(),
            gen_error: None,
            show_secret: false,
        }
    }
}

// ── base64url ──────────────────────────────────────────────────────────────

fn b64url_decode(part: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(part.trim())
        .map_err(|_| "Invalid base64url segment".to_string())
}

fn b64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ── decode ─────────────────────────────────────────────────────────────────

fn parse_claims(json: &str) -> Result<Vec<JwtClaim>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "Header / payload is not valid JSON".to_string())?;
    let Some(map) = value.as_object() else {
        return Err("Header / payload must be a JSON object".to_string());
    };
    Ok(map
        .iter()
        .map(|(k, v)| {
            let value_json = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
            let value_kind = match v {
                serde_json::Value::String(_) => "string",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::Null => "null",
                _ => "object",
            }
            .to_string();
            let timestamp = v.as_i64().and_then(|ts| {
                let key = k.as_str();
                (key == "iat" || key == "exp" || key == "nbf").then(|| humanize_timestamp(ts))
            });
            JwtClaim {
                key: k.clone(),
                value_json,
                value_kind,
                timestamp,
            }
        })
        .collect())
}

fn humanize_timestamp(ts: i64) -> String {
    use chrono::TimeZone as _;
    let Some(dt) = chrono::Utc.timestamp_opt(ts, 0).single() else {
        return format!("{ts}");
    };
    let absolute = dt.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let now = chrono::Utc::now();
    let diff = (now - dt).num_seconds();
    let span = if diff >= 0 {
        format_ago(diff)
    } else {
        format!("in {}", format_ago(-diff))
    };
    format!("{absolute} · {span}")
}

fn format_ago(seconds: i64) -> String {
    let mins = seconds / 60;
    if mins < 1 {
        format!("{seconds}s")
    } else if mins < 60 {
        format!("{mins}m")
    } else {
        let hours = mins / 60;
        if hours < 24 {
            format!("{hours}h")
        } else {
            format!("{}d", hours / 24)
        }
    }
}

/// Decode a token into claims + audit findings. Any malformed token yields
/// `Err` with the reference's fixed error copy.
pub fn decode_and_audit(token: &str) -> Result<(JwtDecoded, Vec<JwtVulnerability>), String> {
    let token = token.trim();
    let parts: Vec<&str> = token.split('.').collect();
    // `alg: none` tokens legitimately carry an empty signature segment.
    if parts.len() != 3 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(
            "Invalid JWT format. Expected three base64url-encoded parts separated by dots."
                .to_string(),
        );
    }
    let header_json = String::from_utf8(
        b64url_decode(parts[0]).map_err(|_| "Invalid base64url segment".to_string())?,
    )
    .map_err(|_| "Header / payload is not valid JSON".to_string())?;
    let payload_json = String::from_utf8(
        b64url_decode(parts[1]).map_err(|_| "Invalid base64url segment".to_string())?,
    )
    .map_err(|_| "Header / payload is not valid JSON".to_string())?;

    let header = parse_claims(&header_json)?;
    let payload = parse_claims(&payload_json)?;

    let algorithm = header
        .iter()
        .find(|c| c.key == "alg")
        .map(|c| c.value_json.trim_matches('"').to_string())
        .unwrap_or_default();

    let decoded = JwtDecoded {
        algorithm: algorithm.clone(),
        header,
        payload,
        signature_b64: parts[2].to_string(),
    };
    let vulnerabilities = check_vulnerabilities(&decoded, &header_json, &payload_json);
    Ok((decoded, vulnerabilities))
}

fn claim_i64(payload: &[JwtClaim], key: &str) -> Option<i64> {
    payload
        .iter()
        .find(|c| c.key == key)
        .and_then(|c| c.value_json.parse::<i64>().ok())
}

fn check_vulnerabilities(
    decoded: &JwtDecoded,
    _header_json: &str,
    _payload_json: &str,
) -> Vec<JwtVulnerability> {
    let mut findings = Vec::new();
    let now = chrono::Utc::now().timestamp();

    if decoded.algorithm == "none" {
        findings.push(JwtVulnerability {
            severity: JwtSeverity::Critical,
            title: "Unsigned token (alg: none)".into(),
            detail: "The token declares the `none` algorithm — anyone can forge it. Reject `alg: none` server-side.".into(),
        });
    }
    match claim_i64(&decoded.payload, "exp") {
        Some(exp) if exp < now => findings.push(JwtVulnerability {
            severity: JwtSeverity::High,
            title: "Token expired".into(),
            detail: format!("`exp` was {} ({}).", humanize_timestamp(exp), exp),
        }),
        None => findings.push(JwtVulnerability {
            severity: JwtSeverity::Medium,
            title: "No expiration claim".into(),
            detail: "The payload has no `exp` — the token never expires.".into(),
        }),
        _ => {}
    }
    if let Some(nbf) = claim_i64(&decoded.payload, "nbf") {
        if nbf > now {
            findings.push(JwtVulnerability {
                severity: JwtSeverity::Medium,
                title: "Token not yet valid".into(),
                detail: format!("`nbf` is {} ({}).", humanize_timestamp(nbf), nbf),
            });
        }
    }
    if claim_i64(&decoded.payload, "iat").is_none() {
        findings.push(JwtVulnerability {
            severity: JwtSeverity::Low,
            title: "No issued-at claim".into(),
            detail: "The payload has no `iat` — issuance time cannot be audited.".into(),
        });
    }
    if decoded.algorithm.starts_with("HS") {
        findings.push(JwtVulnerability {
            severity: JwtSeverity::Info,
            title: "Symmetric algorithm".into(),
            detail:
                "HS* uses one shared secret on both ends — anyone who can verify can also sign."
                    .into(),
        });
    }
    findings
}

// ── sign ───────────────────────────────────────────────────────────────────

/// Sign `header_json.payload_json` with the configured algorithm. HMAC
/// families and `none` are supported in the desktop port; asymmetric
/// algorithms report an explicit error.
pub fn sign_jwt(
    algorithm: JwtAlgorithm,
    header_json: &str,
    payload_json: &str,
    secret: &str,
) -> Result<String, String> {
    if algorithm == JwtAlgorithm::None {
        let header = force_alg(header_json, "none")?;
        return Ok(format!(
            "{}.{}.",
            b64url_encode(header.as_bytes()),
            b64url_encode(payload_json.trim().as_bytes())
        ));
    }
    let alg_label = algorithm.label();
    let header = force_alg(header_json, alg_label)?;
    let signing_input = format!(
        "{}.{}",
        b64url_encode(header.as_bytes()),
        b64url_encode(payload_json.trim().as_bytes())
    );

    let signature = match algorithm {
        JwtAlgorithm::HS256 => hmac_sha256(secret.as_bytes(), signing_input.as_bytes()),
        JwtAlgorithm::HS384 => hmac_sha384(secret.as_bytes(), signing_input.as_bytes()),
        JwtAlgorithm::HS512 => hmac_sha512(secret.as_bytes(), signing_input.as_bytes()),
        asymmetric => sign_asymmetric(asymmetric, secret, signing_input.as_bytes())?,
    };
    Ok(format!("{signing_input}.{signature}"))
}

fn force_alg(header_json: &str, alg: &str) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(header_json.trim())
        .map_err(|_| "Header is not valid JSON".to_string())?;
    let Some(map) = value.as_object_mut() else {
        return Err("Header must be a JSON object".to_string());
    };
    map.insert("alg".into(), serde_json::Value::String(alg.into()));
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

macro_rules! hmac_impl {
    ($name:ident, $sha:ty) => {
        fn $name(secret: &[u8], input: &[u8]) -> String {
            let mut mac =
                <Hmac<$sha> as Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
            mac.update(input);
            b64url_encode(&mac.finalize().into_bytes())
        }
    };
}

hmac_impl!(hmac_sha256, Sha256);
hmac_impl!(hmac_sha384, Sha384);
hmac_impl!(hmac_sha512, Sha512);

/// Random hex secret sized per algorithm (32 / 48 / 64 bytes like the
/// reference's `crypto.getRandomValues` path).
pub fn generate_secret(algorithm: JwtAlgorithm) -> String {
    use rand::Rng as _;
    let bytes = match algorithm {
        JwtAlgorithm::HS256 => 32,
        JwtAlgorithm::HS384 => 48,
        JwtAlgorithm::HS512 => 64,
        _ => 32,
    };
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill(&mut buf[..]);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

// ── asymmetric signing (RSA / RSA-PSS / ECDSA) ─────────────────────────────
//
// JOSE signatures: RS* = RSA PKCS#1 v1.5, PS* = RSA-PSS (salt = digest size),
// ES* = ECDSA with raw r||s output (no DER). Keys arrive as PEM — PKCS#8,
// PKCS#1 (RSA), or SEC1 (EC) are all accepted.

fn sign_asymmetric(algorithm: JwtAlgorithm, pem: &str, input: &[u8]) -> Result<String, String> {
    use signature::{RandomizedSigner as _, SignatureEncoding as _, Signer as _};

    let pem = pem.trim();
    match algorithm {
        JwtAlgorithm::RS256 => {
            let key = rsa_key(pem)?;
            let signing = rsa::pkcs1v15::SigningKey::<Sha256>::new(key);
            let sig = signing.sign(input);
            Ok(b64url_encode(&sig.to_bytes()))
        }
        JwtAlgorithm::RS384 => {
            let key = rsa_key(pem)?;
            let signing = rsa::pkcs1v15::SigningKey::<Sha384>::new(key);
            let sig = signing.sign(input);
            Ok(b64url_encode(&sig.to_bytes()))
        }
        JwtAlgorithm::RS512 => {
            let key = rsa_key(pem)?;
            let signing = rsa::pkcs1v15::SigningKey::<Sha512>::new(key);
            let sig = signing.sign(input);
            Ok(b64url_encode(&sig.to_bytes()))
        }
        JwtAlgorithm::PS256 => {
            let key = rsa_key(pem)?;
            // `new` uses salt length = digest size, which JOSE PS* requires.
            let signing = rsa::pss::SigningKey::<Sha256>::new(key);
            let sig = signing.sign_with_rng(&mut rand::rngs::OsRng, input);
            Ok(b64url_encode(&sig.to_bytes()))
        }
        JwtAlgorithm::PS384 => {
            let key = rsa_key(pem)?;
            let signing = rsa::pss::SigningKey::<Sha384>::new(key);
            let sig = signing.sign_with_rng(&mut rand::rngs::OsRng, input);
            Ok(b64url_encode(&sig.to_bytes()))
        }
        JwtAlgorithm::PS512 => {
            let key = rsa_key(pem)?;
            let signing = rsa::pss::SigningKey::<Sha512>::new(key);
            let sig = signing.sign_with_rng(&mut rand::rngs::OsRng, input);
            Ok(b64url_encode(&sig.to_bytes()))
        }
        JwtAlgorithm::ES256 => {
            let key = ec_key_p256(pem)?;
            let signature: p256::ecdsa::Signature = key.sign(input);
            Ok(b64url_encode(&signature.to_bytes()))
        }
        JwtAlgorithm::ES384 => {
            let key = ec_key_p384(pem)?;
            let signature: p384::ecdsa::Signature = key.sign(input);
            Ok(b64url_encode(&signature.to_bytes()))
        }
        JwtAlgorithm::ES512 => Err(
            "ES512 (P-521) PEM keys are not supported in this build — use ES256/ES384 or RS*/PS*"
                .to_string(),
        ),
        _ => Err("not an asymmetric algorithm".to_string()),
    }
}

fn rsa_key(pem: &str) -> Result<rsa::RsaPrivateKey, String> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| rsa::RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|_| "Invalid RSA private key PEM (expected PKCS#8 or PKCS#1)".to_string())
}

fn ec_key_p256(pem: &str) -> Result<p256::ecdsa::SigningKey, String> {
    use p256::pkcs8::DecodePrivateKey;
    p256::ecdsa::SigningKey::from_pkcs8_pem(pem)
        .or_else(|_| p256::SecretKey::from_sec1_pem(pem).map(p256::ecdsa::SigningKey::from))
        .map_err(|_| "Invalid EC P-256 private key PEM (expected PKCS#8 or SEC1)".to_string())
}

fn ec_key_p384(pem: &str) -> Result<p384::ecdsa::SigningKey, String> {
    use p384::pkcs8::DecodePrivateKey;
    p384::ecdsa::SigningKey::from_pkcs8_pem(pem)
        .or_else(|_| p384::SecretKey::from_sec1_pem(pem).map(p384::ecdsa::SigningKey::from))
        .map_err(|_| "Invalid EC P-384 private key PEM (expected PKCS#8 or SEC1)".to_string())
}

/// Generate a key pair for `algorithm` and return `(private_pem, public_pem)`.
/// HMAC returns `Err` (callers use [`generate_secret`] instead); RSA keys are
/// 2048-bit like the reference's WebCrypto path.
pub fn generate_keypair(algorithm: JwtAlgorithm) -> Result<(String, String), String> {
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};

    match algorithm {
        alg if alg.is_hmac() => Err("HMAC uses a random secret, not a key pair".to_string()),
        JwtAlgorithm::None => Err("`none` needs no key".to_string()),
        alg if alg.category() == "RSA" || alg.category() == "RSA-PSS" => {
            let mut rng = rand::rngs::OsRng;
            let private = rsa::RsaPrivateKey::new(&mut rng, 2048)
                .map_err(|e| format!("RSA keygen failed: {e}"))?;
            let private_pem = private
                .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
                .map_err(|e| e.to_string())?
                .to_string();
            let public_pem = rsa::RsaPublicKey::from(&private)
                .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
                .map_err(|e| e.to_string())?;
            Ok((private_pem, public_pem))
        }
        alg => {
            let (private_pem, public_pem) = match alg {
                JwtAlgorithm::ES256 => {
                    use p256::ecdsa::SigningKey;
                    use p256::SecretKey;
                    let secret = SecretKey::random(&mut rand::rngs::OsRng);
                    let signing = SigningKey::from(&secret);
                    ec_pair_pems(&signing)?
                }
                JwtAlgorithm::ES384 => {
                    use p384::ecdsa::SigningKey;
                    use p384::SecretKey;
                    let secret = SecretKey::random(&mut rand::rngs::OsRng);
                    let signing = SigningKey::from(&secret);
                    ec_pair_pems_p384(&signing)?
                }
                _ => return Err("unsupported algorithm".to_string()),
            };
            Ok((private_pem, public_pem))
        }
    }
}

fn ec_pair_pems(signing: &p256::ecdsa::SigningKey) -> Result<(String, String), String> {
    use p256::pkcs8::{EncodePrivateKey, EncodePublicKey};
    ec_pair_pems_inner(
        signing
            .to_pkcs8_pem(pkcs8::LineEnding::LF)
            .map_err(|e| e.to_string())?
            .as_str(),
        |line| {
            Ok(signing
                .verifying_key()
                .to_public_key_pem(line)
                .map_err(|e| e.to_string())?)
        },
    )
}

fn ec_pair_pems_p384(signing: &p384::ecdsa::SigningKey) -> Result<(String, String), String> {
    use p384::pkcs8::{EncodePrivateKey, EncodePublicKey};
    ec_pair_pems_inner(
        signing
            .to_pkcs8_pem(pkcs8::LineEnding::LF)
            .map_err(|e| e.to_string())?
            .as_str(),
        |line| {
            Ok(signing
                .verifying_key()
                .to_public_key_pem(line)
                .map_err(|e| e.to_string())?)
        },
    )
}

fn ec_pair_pems_inner(
    private_pem: &str,
    public: impl FnOnce(pkcs8::LineEnding) -> Result<String, String>,
) -> Result<(String, String), String> {
    Ok((private_pem.to_string(), public(pkcs8::LineEnding::LF)?))
}

// ── Bridge command surface ─────────────────────────────────────────────────

impl Bridge {
    pub fn jwt_set_mode(mode: JwtMode, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.jwt.mode = mode;
        });
    }

    /// Live decode: re-runs on every keystroke via the app's editor sync.
    pub fn jwt_set_token(token: &str, cx: &mut App) {
        match decode_and_audit(token) {
            Ok((decoded, vulnerabilities)) => {
                Self::state(cx).update(cx, |s| {
                    s.jwt.decoded = Some(decoded);
                    s.jwt.vulnerabilities = vulnerabilities;
                    s.jwt.decode_error = None;
                });
            }
            Err(message) => {
                Self::state(cx).update(cx, |s| {
                    s.jwt.decoded = None;
                    s.jwt.vulnerabilities.clear();
                    s.jwt.decode_error = Some(message);
                });
            }
        }
    }

    pub fn jwt_set_algorithm(algorithm: JwtAlgorithm, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.jwt.gen_algorithm = algorithm;
        });
    }

    pub fn jwt_set_secret(secret: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.jwt.gen_secret = secret.to_string();
        });
    }

    pub fn jwt_toggle_show_secret(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.jwt.show_secret = !s.jwt.show_secret;
        });
    }

    /// Build and sign a token from the editor contents. JSON errors and
    /// unsupported algorithms surface in `gen_error` like the reference.
    pub fn jwt_generate(header: &str, payload: &str, secret: &str, cx: &mut App) {
        let algorithm = Self::state(cx).read(cx).jwt.gen_algorithm;
        let result = sign_jwt(algorithm, header, payload, secret);
        Self::state(cx).update(cx, |s| match result {
            Ok(token) => {
                s.jwt.generated_token = token;
                s.jwt.gen_error = None;
            }
            Err(message) => {
                s.jwt.gen_error = Some(message);
            }
        });
    }

    pub fn jwt_generate_secret(cx: &mut App) {
        let algorithm = Self::state(cx).read(cx).jwt.gen_algorithm;
        let secret = generate_secret(algorithm);
        Self::state(cx).update(cx, |s| {
            s.jwt.gen_secret = secret;
        });
    }

    /// Generate a key pair for the current algorithm. The private PEM lands in
    /// the secret field (the reference stores it the same way); the public PEM
    /// is reported through `last_message` for copying from the debug log.
    pub fn jwt_generate_keypair(cx: &mut App) {
        let algorithm = Self::state(cx).read(cx).jwt.gen_algorithm;
        match generate_keypair(algorithm) {
            Ok((private_pem, public_pem)) => Self::state(cx).update(cx, |s| {
                s.jwt.gen_secret = private_pem;
                s.jwt.show_secret = true;
                s.jwt.gen_error = None;
                let public = public_pem
                    .lines()
                    .filter(|line| !line.starts_with("-----"))
                    .collect::<Vec<_>>()
                    .concat();
                s.apply_ok_msg(format!("Key pair generated — public key: {public}"));
            }),
            Err(message) => Self::state(cx).update(cx, |s| {
                s.jwt.gen_error = Some(message);
            }),
        }
    }

    pub fn jwt_clear_decode(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.jwt.decoded = None;
            s.jwt.decode_error = None;
            s.jwt.vulnerabilities.clear();
        });
    }

    pub fn jwt_clear_generate(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.jwt.gen_header = JWT_DEFAULT_HEADER.to_string();
            s.jwt.gen_payload = JWT_DEFAULT_PAYLOAD.to_string();
            s.jwt.gen_secret.clear();
            s.jwt.generated_token.clear();
            s.jwt.gen_error = None;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpWVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.sig";

    #[test]
    fn decodes_three_part_token() {
        let (decoded, findings) = decode_and_audit(TOKEN).unwrap();
        assert_eq!(decoded.algorithm, "HS256");
        assert_eq!(decoded.payload.len(), 3);
        assert_eq!(decoded.signature_b64, "sig");
        // HS advisory + no exp.
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn rejects_malformed_tokens() {
        assert!(decode_and_audit("not-a-jwt").is_err());
        assert!(decode_and_audit("a.b").is_err());
        assert!(decode_and_audit("..").is_err());
    }

    #[test]
    fn none_algorithm_is_flagged() {
        let token = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIxIn0.";
        let (_, findings) = decode_and_audit(token).unwrap();
        assert!(findings
            .iter()
            .any(|f| f.severity == JwtSeverity::Critical && f.title.contains("none")));
    }

    #[test]
    fn expired_token_is_flagged() {
        let payload = serde_json::json!({ "exp": 1_000_000_000u64, "iat": 1_000_000_000u64 });
        let header = serde_json::json!({ "alg": "HS256", "typ": "JWT" });
        let token = format!(
            "{}.{}.sig",
            b64url_encode(header.to_string().as_bytes()),
            b64url_encode(payload.to_string().as_bytes())
        );
        let (_, findings) = decode_and_audit(&token).unwrap();
        assert!(findings.iter().any(|f| f.title == "Token expired"));
    }

    #[test]
    fn signs_and_round_trips_hmac() {
        let token = sign_jwt(
            JwtAlgorithm::HS256,
            r#"{"alg":"HS256","typ":"JWT"}"#,
            r#"{"sub":"1234567890"}"#,
            "secret",
        )
        .unwrap();
        let (decoded, _) = decode_and_audit(&token).unwrap();
        assert_eq!(decoded.algorithm, "HS256");
        assert_eq!(decoded.payload[0].key, "sub");

        // Known RFC 7515-style vector: HMAC-SHA256 over empty-ish payload.
        let t = sign_jwt(
            JwtAlgorithm::HS256,
            r#"{"typ":"JWT","alg":"HS256"}"#,
            r#"{"iss":"joe"}"#,
            "key",
        )
        .unwrap();
        // Signature must be stable across runs.
        let t2 = sign_jwt(
            JwtAlgorithm::HS256,
            r#"{"typ":"JWT","alg":"HS256"}"#,
            r#"{"iss":"joe"}"#,
            "key",
        )
        .unwrap();
        assert_eq!(t, t2);
    }

    #[test]
    fn alg_is_forced_on_sign() {
        // Header says none; signing with HS256 must rewrite it.
        let token = sign_jwt(
            JwtAlgorithm::HS256,
            r#"{"alg":"none","typ":"JWT"}"#,
            r#"{"sub":"1"}"#,
            "k",
        )
        .unwrap();
        let (decoded, _) = decode_and_audit(&token).unwrap();
        assert_eq!(decoded.algorithm, "HS256");
    }

    #[test]
    fn none_signing_produces_empty_signature() {
        let token = sign_jwt(
            JwtAlgorithm::None,
            r#"{"alg":"none"}"#,
            r#"{"sub":"1"}"#,
            "",
        )
        .unwrap();
        assert!(token.ends_with('.'));
    }

    #[test]
    fn rs256_keypair_signs_round_trip() {
        let (private, _public) = generate_keypair(JwtAlgorithm::RS256).unwrap();
        assert!(private.contains("BEGIN PRIVATE KEY"));
        let token = sign_jwt(
            JwtAlgorithm::RS256,
            r#"{"typ":"JWT","alg":"RS256"}"#,
            r#"{"iss":"joe"}"#,
            &private,
        )
        .unwrap();
        let (decoded, _) = decode_and_audit(&token).unwrap();
        assert_eq!(decoded.algorithm, "RS256");
        // 2048-bit signature → 256 bytes → 342 base64url chars.
        let sig = token.rsplit('.').next().unwrap();
        assert_eq!(sig.len(), 342);
    }

    #[test]
    fn ps256_signs_with_generated_key() {
        let (private, _) = generate_keypair(JwtAlgorithm::PS256).unwrap();
        let token = sign_jwt(
            JwtAlgorithm::PS256,
            r#"{"alg":"PS256"}"#,
            r#"{"sub":"1"}"#,
            &private,
        )
        .unwrap();
        let sig = token.rsplit('.').next().unwrap();
        assert_eq!(sig.len(), 342);
        let (decoded, _) = decode_and_audit(&token).unwrap();
        assert_eq!(decoded.algorithm, "PS256");
    }

    #[test]
    fn es256_keypair_signs_raw_signature() {
        let (private, public) = generate_keypair(JwtAlgorithm::ES256).unwrap();
        assert!(private.contains("BEGIN PRIVATE KEY"));
        assert!(public.contains("BEGIN PUBLIC KEY"));
        let token = sign_jwt(
            JwtAlgorithm::ES256,
            r#"{"alg":"ES256"}"#,
            r#"{"sub":"1"}"#,
            &private,
        )
        .unwrap();
        // P-256 JOSE signature = 64 raw bytes → 86 base64url chars.
        let sig = token.rsplit('.').next().unwrap();
        assert_eq!(sig.len(), 86);
    }

    #[test]
    fn invalid_pem_is_reported() {
        let err =
            sign_jwt(JwtAlgorithm::RS256, r#"{"alg":"RS256"}"#, "{}", "not a pem").unwrap_err();
        assert!(err.contains("Invalid RSA private key PEM"));
        let err = sign_jwt(JwtAlgorithm::ES256, r#"{"alg":"ES256"}"#, "{}", "nope").unwrap_err();
        assert!(err.contains("P-256"));
    }

    #[test]
    fn es512_is_explicitly_unsupported() {
        let err = sign_jwt(JwtAlgorithm::ES512, r#"{"alg":"ES512"}"#, "{}", "pem").unwrap_err();
        assert!(err.contains("ES512"));
    }
}
