//! Typed HTTP helpers over the `host-http` import.
//!
//! [`crate::host::http`] is the raw interface: it takes a request record of
//! strings and bytes and returns a response record. This module adds the parts
//! every provider repeats, so a plugin does not hand-roll them:
//!
//! - request construction for the two shapes a JSON API needs, [`get`] and
//!   [`post_json`];
//! - [`send`], which maps the host's `http-error` to [`ProviderError`];
//! - [`status_error`], which turns a non-2xx response into the matching
//!   [`ProviderError`] variant, including `retry-after` for `429`;
//! - [`send_json`], which sends a request and deserializes a JSON body.
//!
//! The host still enforces the instance's permissions: a URL outside the
//! manifest's `permissions.http.hosts` allowlist fails with
//! `http-error.denied`, which [`send`] maps to [`ProviderError::Unauthorized`].
//!
//! # Example
//!
//! ```ignore
//! use hearthsound_plugin::http;
//!
//! #[derive(serde::Deserialize)]
//! struct Lookup {
//!     id: u64,
//! }
//!
//! let mut request = http::get("https://api.example.com/lookup?q=hello");
//! request.headers.push(("accept".to_string(), "application/json".to_string()));
//! let lookup: Lookup = http::send_json(&request)?;
//! ```

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::host::http::{self, HttpError, Request, Response};
use crate::types::ProviderError;

/// Number of characters of a response body kept in an error message.
///
/// Response bodies can be megabytes; an error string only needs enough of the
/// body for a human to recognize the failure.
const ERROR_BODY_CHARS: usize = 500;

/// Builds a `GET` request with no body.
///
/// # Parameters
///
/// - `url`: absolute URL to fetch, including any query string.
///
/// # Returns
///
/// A request with no headers and no body. Add headers by pushing onto
/// [`Request::headers`] before calling [`send`].
pub fn get(url: &str) -> Request {
    Request {
        method: "GET".to_string(),
        url: url.to_string(),
        headers: Vec::new(),
        body: None,
        timeout_ms: None,
    }
}

/// Builds a `POST` request with a JSON body.
///
/// The request carries `content-type: application/json`.
///
/// # Parameters
///
/// - `url`: absolute URL to post to.
/// - `body`: value serialized to JSON as the request body.
///
/// # Returns
///
/// The request, ready for [`send`].
///
/// # Errors
///
/// Returns [`ProviderError::Internal`] when `body` cannot be serialized.
pub fn post_json<T: Serialize>(url: &str, body: &T) -> Result<Request, ProviderError> {
    let body =
        serde_json::to_vec(body).map_err(|e| ProviderError::Internal(format!("body: {e}")))?;
    Ok(Request {
        method: "POST".to_string(),
        url: url.to_string(),
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Some(body),
        timeout_ms: None,
    })
}

/// Sends a request through the host.
///
/// # Parameters
///
/// - `request`: the request to perform.
///
/// # Returns
///
/// The response, whatever its status; check it with [`is_success`] or hand it
/// to [`status_error`].
///
/// # Errors
///
/// Maps the host's `http-error` to a provider error: a request the host denied
/// (allowlist, address policy or redirect limit) becomes
/// [`ProviderError::Unauthorized`] with the host's reason, and a timeout, an
/// oversized body or a network failure become [`ProviderError::Unavailable`].
pub fn send(request: &Request) -> Result<Response, ProviderError> {
    http::send(request).map_err(http_error)
}

/// Returns whether a response carries a successful status.
///
/// # Parameters
///
/// - `response`: the response to test.
///
/// # Returns
///
/// `true` for every `2xx` status, `false` otherwise.
pub fn is_success(response: &Response) -> bool {
    (200..300).contains(&response.status)
}

/// Maps a non-success response to a provider error.
///
/// # Parameters
///
/// - `response`: a response whose status is not `2xx`.
///
/// # Returns
///
/// [`ProviderError::Unauthorized`] for `401` and `403`,
/// [`ProviderError::NotFound`] for `404`, [`ProviderError::RateLimited`] for
/// `429` (the `retry-after` header in milliseconds, or `0` when the header is
/// absent or not a number of seconds), [`ProviderError::Unavailable`] for other
/// `5xx` statuses and [`ProviderError::Internal`] for anything else. Message
/// carriers hold the status line and at most the first 500 characters of the
/// body.
pub fn status_error(response: &Response) -> ProviderError {
    let message = body_excerpt(response);
    match response.status {
        401 | 403 => ProviderError::Unauthorized(message),
        404 => ProviderError::NotFound,
        429 => ProviderError::RateLimited(retry_after_ms(response)),
        status if (500..600).contains(&status) => {
            ProviderError::Unavailable(format!("HTTP {status}: {message}"))
        }
        status => ProviderError::Internal(format!("HTTP {status}: {message}")),
    }
}

/// Sends a request and deserializes a JSON response body.
///
/// # Parameters
///
/// - `request`: the request to perform.
///
/// # Returns
///
/// The response body deserialized into `T`.
///
/// # Errors
///
/// Returns the [`status_error`] of a non-`2xx` response, and
/// [`ProviderError::Internal`] when the body is not JSON for `T`.
pub fn send_json<T: DeserializeOwned>(request: &Request) -> Result<T, ProviderError> {
    let response = send(request)?;
    if !is_success(&response) {
        return Err(status_error(&response));
    }
    serde_json::from_slice(&response.body)
        .map_err(|e| ProviderError::Internal(format!("response body: {e}")))
}

/// Maps the host's HTTP error to a provider error.
///
/// # Parameters
///
/// - `error`: the error returned by the `host-http` import.
///
/// # Returns
///
/// [`ProviderError::Unauthorized`] for a denied request, so an administrator
/// can see the allowlist reason, and [`ProviderError::Unavailable`] for a
/// timeout, an oversized body or a network failure.
fn http_error(error: HttpError) -> ProviderError {
    match error {
        HttpError::Denied(reason) => ProviderError::Unauthorized(reason),
        HttpError::Timeout => ProviderError::Unavailable("request timed out".to_string()),
        HttpError::BodyTooLarge => {
            ProviderError::Unavailable("response body is too large".to_string())
        }
        HttpError::Network(message) => ProviderError::Unavailable(message),
    }
}

/// Returns the status line and an excerpt of the body, for an error message.
///
/// # Parameters
///
/// - `response`: the response to describe.
///
/// # Returns
///
/// The body decoded as UTF-8 with invalid sequences replaced, truncated to
/// [`ERROR_BODY_CHARS`] characters at a character boundary.
fn body_excerpt(response: &Response) -> String {
    let text = String::from_utf8_lossy(&response.body);
    let text = text.trim();
    match text.char_indices().nth(ERROR_BODY_CHARS) {
        Some((index, _)) => format!("{}...", &text[..index]),
        None => text.to_string(),
    }
}

/// Reads the `retry-after` header as a delay in milliseconds.
///
/// # Parameters
///
/// - `response`: the response to read the header from.
///
/// # Returns
///
/// The header value in seconds times 1000, saturated to `u32`, or `0` when the
/// header is absent, not a number, or the HTTP-date form of `retry-after`.
fn retry_after_ms(response: &Response) -> u32 {
    response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, value)| value.trim().parse::<u64>().ok())
        .map(|seconds| u32::try_from(seconds.saturating_mul(1000)).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a response with the given status, headers and body.
    fn response(status: u16, headers: &[(&str, &str)], body: &str) -> Response {
        Response {
            status,
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn get_builds_an_empty_get_request() {
        let request = get("https://example.com/a?b=c");
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://example.com/a?b=c");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
        assert!(request.timeout_ms.is_none());
    }

    #[test]
    fn post_json_sets_the_content_type_and_serializes_the_body() {
        let request = post_json("https://example.com", &vec!["a", "b"]).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.headers,
            vec![("content-type".to_string(), "application/json".to_string())]
        );
        assert_eq!(request.body.unwrap(), b"[\"a\",\"b\"]".to_vec());
    }

    #[test]
    fn success_covers_2xx_only() {
        assert!(is_success(&response(200, &[], "")));
        assert!(is_success(&response(299, &[], "")));
        assert!(!is_success(&response(300, &[], "")));
        assert!(!is_success(&response(404, &[], "")));
    }

    #[test]
    fn status_error_maps_common_failures() {
        assert!(matches!(
            status_error(&response(401, &[], "no token")),
            ProviderError::Unauthorized(message) if message == "no token"
        ));
        assert!(matches!(
            status_error(&response(404, &[], "gone")),
            ProviderError::NotFound
        ));
        assert!(matches!(
            status_error(&response(503, &[], "maintenance")),
            ProviderError::Unavailable(message) if message == "HTTP 503: maintenance"
        ));
        assert!(matches!(
            status_error(&response(400, &[], "bad query")),
            ProviderError::Internal(message) if message == "HTTP 400: bad query"
        ));
    }

    #[test]
    fn rate_limited_reads_retry_after_in_seconds_and_milliseconds() {
        assert!(matches!(
            status_error(&response(429, &[("Retry-After", " 3 ")], "")),
            ProviderError::RateLimited(3_000)
        ));
        assert!(matches!(
            status_error(&response(429, &[], "")),
            ProviderError::RateLimited(0)
        ));
        assert!(matches!(
            status_error(&response(429, &[("retry-after", "99999999999")], "")),
            ProviderError::RateLimited(u32::MAX)
        ));
    }

    #[test]
    fn body_excerpt_truncates_on_a_character_boundary() {
        let body = "e".repeat(600);
        let excerpt = body_excerpt(&response(500, &[], &body));
        assert_eq!(excerpt.chars().count(), ERROR_BODY_CHARS + 3);
        assert!(excerpt.ends_with("..."));

        let unicode = "\u{e9}".repeat(600);
        let excerpt = body_excerpt(&response(500, &[], &unicode));
        assert_eq!(excerpt.chars().count(), ERROR_BODY_CHARS + 3);
    }
}
