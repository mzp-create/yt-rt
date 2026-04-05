use anyhow::Context;
use regex::Regex;
use std::collections::HashMap;
use yt_dlp_jsinterp::JsInterpreter;

/// Extracts and caches signature decipher and n-parameter transform functions.
pub struct SignatureDecryptor {
    /// The decipher function JavaScript code
    decipher_func: Option<String>,
    /// The n-parameter transform function JavaScript code
    nsig_func: Option<String>,
    /// Cache of already-deciphered signatures
    sig_cache: HashMap<String, String>,
    /// Cache of already-transformed n-parameters
    nsig_cache: HashMap<String, String>,
}

impl SignatureDecryptor {
    pub fn new() -> Self {
        Self {
            decipher_func: None,
            nsig_func: None,
            sig_cache: HashMap::new(),
            nsig_cache: HashMap::new(),
        }
    }

    /// Extract decipher and nsig functions from player.js source code.
    pub fn extract_functions(&mut self, player_js: &str) -> anyhow::Result<()> {
        self.decipher_func = Some(extract_decipher_function(player_js)?);
        self.nsig_func = extract_nsig_function(player_js).ok();
        Ok(())
    }

    /// Decipher a scrambled signature.
    pub fn decipher_signature(&mut self, scrambled_sig: &str) -> anyhow::Result<String> {
        if let Some(cached) = self.sig_cache.get(scrambled_sig) {
            return Ok(cached.clone());
        }
        let func = self
            .decipher_func
            .as_ref()
            .context("decipher function not extracted")?;

        let mut js = JsInterpreter::new();
        js.load(func)?;
        let result = js.execute(&format!(
            "decipher(\"{}\")",
            escape_js_string(scrambled_sig)
        ))?;

        self.sig_cache
            .insert(scrambled_sig.to_string(), result.clone());
        Ok(result)
    }

    /// Transform an n-parameter to bypass throttling.
    pub fn transform_nsig(&mut self, n: &str) -> anyhow::Result<String> {
        if let Some(cached) = self.nsig_cache.get(n) {
            return Ok(cached.clone());
        }
        let func = self
            .nsig_func
            .as_ref()
            .context("nsig function not extracted")?;

        let mut js = JsInterpreter::new();
        js.load(func)?;
        let result = js.execute(&format!("nsig(\"{}\")", escape_js_string(n)))?;

        // If the result is the same length or longer, something went wrong --
        // YouTube returns the same value when the transform fails, but we still
        // cache it to avoid re-executing.
        self.nsig_cache.insert(n.to_string(), result.clone());
        Ok(result)
    }

    /// Process a format URL -- decipher signature and transform n-parameter.
    ///
    /// If `signature_cipher` is provided, it is parsed to extract the URL, the scrambled
    /// signature, and the signature parameter name. Otherwise, only the n-parameter in
    /// the existing URL is transformed.
    pub fn process_url(
        &mut self,
        url: &str,
        signature_cipher: Option<&str>,
    ) -> anyhow::Result<String> {
        let mut final_url = if let Some(cipher) = signature_cipher {
            let (base_url, scrambled_sig, sig_param) = parse_signature_cipher(cipher)?;
            let deciphered = self.decipher_signature(&scrambled_sig)?;
            apply_signature(&base_url, &deciphered, &sig_param)?
        } else {
            url.to_string()
        };

        // Transform the n-parameter if present and if we have the function.
        if self.nsig_func.is_some() {
            if let Some(n_value) = extract_n_param(&final_url) {
                match self.transform_nsig(&n_value) {
                    Ok(new_n) => {
                        final_url = apply_nsig(&final_url, &new_n)?;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to transform n-parameter: {e}");
                    }
                }
            }
        }

        Ok(final_url)
    }
}

impl Default for SignatureDecryptor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Decipher function extraction
// ---------------------------------------------------------------------------

/// Extract the signature decipher function and its helper object from player.js,
/// wrapping them into a callable `function decipher(a) { ... }`.
fn extract_decipher_function(player_js: &str) -> anyhow::Result<String> {
    // Patterns to find the main decipher function name.
    let func_name_patterns: &[&str] = &[
        r#"\b([a-zA-Z0-9$]{2,})\s*=\s*function\(\s*a\s*\)\s*\{\s*a\s*=\s*a\.split\(\s*""\s*\)"#,
        r#"([a-zA-Z0-9$]+)\s*=\s*function\(\s*a\s*\)\s*\{\s*a\s*=\s*a\.split\(\s*""\s*\);"#,
        r#"\bm=([a-zA-Z0-9$]{2,})\(decodeURIComponent"#,
        r#"\bc\s*&&\s*d\.set\([^,]+,\s*(?:encodeURIComponent\s*\()?\s*([a-zA-Z0-9$]+)\("#,
        r#"\bc\s*&&\s*[a-z]\.set\([^,]+,\s*([a-zA-Z0-9$]+)\("#,
    ];

    let func_name = find_first_match(player_js, func_name_patterns)
        .context("could not find decipher function name in player.js")?;

    // Extract the function body: `var FUNC_NAME=function(a){a=a.split(""); ... ;return a.join("")}`
    let escaped_name = regex::escape(&func_name);
    let func_body_re = Regex::new(&format!(
        r#"(?:var\s+)?{escaped_name}\s*=\s*function\(\s*a\s*\)\s*\{{([^}}]*(?:\{{[^}}]*\}}[^}}]*)*)\}}"#,
    ))?;

    let func_body = func_body_re
        .captures(player_js)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("could not extract decipher function body")?;

    // Find the helper object referenced in the function body.
    // The body calls something like `Xy.Ab(a, N)` -- we need the `Xy` object.
    let helper_re = Regex::new(r"([a-zA-Z0-9$]{2,})\.[a-zA-Z0-9$]{2,}\(a,")?;
    let helper_name = helper_re
        .captures(&func_body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("could not find helper object name in decipher function")?;

    // Extract the helper object definition.
    let escaped_helper = regex::escape(&helper_name);
    let helper_obj = extract_object_definition(player_js, &escaped_helper)
        .context("could not extract helper object definition")?;

    // Build the complete decipher code.
    Ok(format!(
        "{helper_obj}\nfunction decipher(a) {{ a = a.split(\"\"); {func_body}; return a.join(\"\"); }}"
    ))
}

/// Extract a `var NAME = { ... };` object definition from JavaScript source,
/// handling nested braces.
fn extract_object_definition(js: &str, escaped_name: &str) -> Option<String> {
    let pattern = format!(r"var\s+{escaped_name}\s*=\s*\{{");
    let re = Regex::new(&pattern).ok()?;
    let m = re.find(js)?;
    let start = m.start();
    // Find the matching closing brace.
    let after_open = m.end();
    let mut depth: u32 = 1;
    let bytes = js.as_bytes();
    let mut pos = after_open;
    while pos < bytes.len() && depth > 0 {
        match bytes[pos] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        pos += 1;
    }
    if depth == 0 {
        // Include a trailing semicolon if present.
        let end = if pos < bytes.len() && bytes[pos] == b';' {
            pos + 1
        } else {
            pos
        };
        Some(js[start..end].to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// N-parameter function extraction
// ---------------------------------------------------------------------------

/// Extract the n-parameter transform function from player.js.
fn extract_nsig_function(player_js: &str) -> anyhow::Result<String> {
    // Multiple patterns that match across different player.js versions.
    let func_name_patterns: &[&str] = &[
        // Pattern: assignment of enhanced_except_ function
        r#"\b([a-zA-Z0-9$]+)\s*=\s*function\(\s*a\s*\)\s*\{[^}]*?enhanced_except_"#,
        // Pattern: var b=a.split("")
        r#"([a-zA-Z0-9$]{2,})\s*=\s*function\(a\)\s*\{\s*var\s+b\s*=\s*a\.split\(\s*""\s*\)"#,
        // Pattern: referenced in URL construction with &n=
        r#"&&\(b=([a-zA-Z0-9$]+)\(b\)\s*,\s*[a-z]+\.set\("n""#,
        // Pattern: set("n", ...) style
        r#";\s*([a-zA-Z0-9$]+)\s*=\s*function\(\s*a\s*\)\s*\{\s*var\s+b\s*=\s*a\.split\(\s*""\s*\).*?return\s+b\.join\(\s*""\s*\)"#,
        // Pattern: newer player.js with different naming
        r#"\b([a-zA-Z0-9$]+)\s*=\s*function\(\s*a\s*\)\s*\{.*?(?:enhanced_except_|return\s+b\.join)"#,
    ];

    let func_name = find_first_match(player_js, func_name_patterns)
        .context("could not find nsig function name in player.js")?;

    // Extract the full function body.
    let escaped_name = regex::escape(&func_name);

    // Try extracting as `var NAME = function(a) { ... }` or `function NAME(a) { ... }`
    let func_code = extract_full_function(player_js, &escaped_name)
        .context("could not extract nsig function body")?;

    // Wrap it so we can call `nsig(a)`.
    Ok(format!(
        "{func_code}\nfunction nsig(a) {{ return {func_name}(a); }}"
    ))
}

/// Extract a full function definition (including nested braces) by name.
fn extract_full_function(js: &str, escaped_name: &str) -> Option<String> {
    // Try `var NAME = function(a) { ... }` first.
    let patterns = [
        format!(r"var\s+{escaped_name}\s*=\s*function\([^)]*\)\s*\{{"),
        format!(r"(?:^|[;\n])\s*{escaped_name}\s*=\s*function\([^)]*\)\s*\{{"),
        format!(r"function\s+{escaped_name}\s*\([^)]*\)\s*\{{"),
    ];

    for pat in &patterns {
        let re = Regex::new(pat).ok()?;
        if let Some(m) = re.find(js) {
            let start = m.start();
            // Find the opening brace.
            let brace_pos = js[m.start()..m.end()]
                .rfind('{')
                .map(|p| m.start() + p)?;
            let after_brace = brace_pos + 1;
            let mut depth: u32 = 1;
            let bytes = js.as_bytes();
            let mut pos = after_brace;
            while pos < bytes.len() && depth > 0 {
                match bytes[pos] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                pos += 1;
            }
            if depth == 0 {
                let end = if pos < bytes.len() && bytes[pos] == b';' {
                    pos + 1
                } else {
                    pos
                };
                return Some(js[start..end].trim_start_matches(|c: char| c == ';' || c == '\n' || c == ' ').to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// URL manipulation helpers
// ---------------------------------------------------------------------------

/// Parse a `signature_cipher` or `cipher` parameter string.
///
/// Format: `s=SCRAMBLED_SIG&sp=sig&url=ENCODED_URL`
///
/// Returns `(url, scrambled_sig, sig_param_name)`.
pub fn parse_signature_cipher(cipher: &str) -> anyhow::Result<(String, String, String)> {
    let params: HashMap<String, String> = url::form_urlencoded::parse(cipher.as_bytes())
        .into_owned()
        .collect();

    let scrambled_sig = params
        .get("s")
        .context("missing 's' parameter in signature cipher")?
        .clone();

    let sig_param = params
        .get("sp")
        .cloned()
        .unwrap_or_else(|| "signature".to_string());

    let base_url = params
        .get("url")
        .context("missing 'url' parameter in signature cipher")?
        .clone();

    Ok((base_url, scrambled_sig, sig_param))
}

/// Apply the deciphered signature to the URL.
pub fn apply_signature(url: &str, signature: &str, sig_param: &str) -> anyhow::Result<String> {
    let mut parsed = url::Url::parse(url).context("invalid URL when applying signature")?;
    parsed
        .query_pairs_mut()
        .append_pair(sig_param, signature);
    Ok(parsed.to_string())
}

/// Replace the `n` parameter in a URL query string with a new value.
pub fn apply_nsig(url: &str, new_n: &str) -> anyhow::Result<String> {
    let mut parsed = url::Url::parse(url).context("invalid URL when applying nsig")?;
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| {
            if k == "n" {
                (k.into_owned(), new_n.to_string())
            } else {
                (k.into_owned(), v.into_owned())
            }
        })
        .collect();

    parsed.query_pairs_mut().clear().extend_pairs(&pairs);
    Ok(parsed.to_string())
}

/// Extract the `n` parameter value from a URL.
fn extract_n_param(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == "n")
        .map(|(_, v)| v.into_owned())
}

/// Escape a string for safe embedding in a JavaScript string literal.
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Try multiple regex patterns against the source and return the first capture group match.
fn find_first_match(source: &str, patterns: &[&str]) -> Option<String> {
    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(source) {
                if let Some(m) = caps.get(1) {
                    return Some(m.as_str().to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_signature_cipher() {
        let cipher = "s=SCRAMBLED&sp=sig&url=https%3A%2F%2Fexample.com%2Fvideo";
        let (url, sig, param) = parse_signature_cipher(cipher).unwrap();
        assert_eq!(url, "https://example.com/video");
        assert_eq!(sig, "SCRAMBLED");
        assert_eq!(param, "sig");
    }

    #[test]
    fn test_parse_signature_cipher_default_sp() {
        let cipher = "s=SCRAMBLED&url=https%3A%2F%2Fexample.com%2Fvideo";
        let (_, _, param) = parse_signature_cipher(cipher).unwrap();
        assert_eq!(param, "signature");
    }

    #[test]
    fn test_apply_signature() {
        let url = "https://example.com/video?itag=22";
        let result = apply_signature(url, "DECIPHERED", "sig").unwrap();
        assert!(result.contains("sig=DECIPHERED"));
    }

    #[test]
    fn test_apply_nsig() {
        let url = "https://example.com/video?n=oldvalue&itag=22";
        let result = apply_nsig(url, "newvalue").unwrap();
        assert!(result.contains("n=newvalue"));
        assert!(!result.contains("n=oldvalue"));
    }

    #[test]
    fn test_escape_js_string() {
        assert_eq!(escape_js_string(r#"he"llo"#), r#"he\"llo"#);
        assert_eq!(escape_js_string("line\nnew"), "line\\nnew");
    }

    #[test]
    fn test_extract_n_param() {
        let url = "https://example.com/video?n=abc123&itag=22";
        assert_eq!(extract_n_param(url), Some("abc123".to_string()));

        let url_no_n = "https://example.com/video?itag=22";
        assert_eq!(extract_n_param(url_no_n), None);
    }

    #[test]
    fn test_extract_object_definition() {
        let js = r#"var abc = {
            reverse: function(a) { a.reverse(); },
            splice: function(a, b) { a.splice(0, b); }
        };
        var other = 5;"#;
        let result = extract_object_definition(js, "abc").unwrap();
        assert!(result.starts_with("var abc"));
        assert!(result.contains("reverse"));
        assert!(result.contains("splice"));
    }
}
