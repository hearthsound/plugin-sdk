//! Outbound HTTP for the mock host.
//!
//! A guest request is checked against the manifest's `permissions.http`
//! allowlist, resolved to public addresses when the rule is static, sent
//! through a blocking client with a per-request timeout, throttled by the
//! manifest's token bucket, and capped on body size. Redirects are followed
//! by hand so that every hop is authorized again.

use std::io::{self, Read};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::{Url, redirect};
use serde_json::Value;

/// Timeout applied when a guest request names none.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Upper bound on any guest-requested timeout.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(60);

/// Response body cap used when the manifest names none (10 MiB).
pub const DEFAULT_MAX_BODY: u64 = 10 * 1024 * 1024;

/// Redirects followed before the request fails.
const MAX_REDIRECTS: usize = 5;

/// One entry of the manifest's `permissions.http.hosts` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// Exact host name, lowercase.
    Literal(String),
    /// `*.domain`: matches any proper subdomain of `domain`.
    Wildcard(String),
    /// `$config:field`: matches the host named by a server configuration
    /// value. The check is resolved per request.
    Config(String),
}

impl Rule {
    /// Parses one manifest host entry.
    ///
    /// Entries of the form `$config:field` become [`Rule::Config`], entries
    /// of the form `*.domain` become [`Rule::Wildcard`], and everything else
    /// is a [`Rule::Literal`]. Host names are lowercased.
    pub fn parse(entry: &str) -> Self {
        if let Some(field) = entry.strip_prefix("$config:") {
            Self::Config(field.to_owned())
        } else if let Some(domain) = entry.strip_prefix("*.") {
            Self::Wildcard(domain.to_ascii_lowercase())
        } else {
            Self::Literal(entry.to_ascii_lowercase())
        }
    }
}

/// One `permissions.http.hosts` entry, with the rate limit it declares.
///
/// A plain string entry waits on the instance-wide
/// `permissions.http.rateLimit`; an object entry waits on its own limit
/// instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEntry {
    /// A host pattern with no limit of its own.
    Pattern(String),
    /// A host pattern with its own rate limit.
    Limited {
        /// Host pattern, in the same forms [`Rule::parse`] accepts.
        host: String,
        /// Requests per second allowed for this host.
        per_second: u32,
        /// Largest burst allowed for this host.
        burst: u32,
    },
}

impl HostEntry {
    /// Parses one raw `permissions.http.hosts` value.
    ///
    /// A string becomes [`HostEntry::Pattern`]; an object with `host` and
    /// `rateLimit.perSecond`/`rateLimit.burst` becomes [`HostEntry::Limited`].
    /// A malformed or non-string entry yields `None`, exactly like a non-string
    /// entry did before object entries existed.
    pub fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::String(pattern) => Some(Self::Pattern(pattern.clone())),
            Value::Object(_) => {
                let host = value.get("host")?.as_str()?.to_owned();
                let limit = value.get("rateLimit")?;
                let per_second = u32::try_from(limit.get("perSecond")?.as_u64()?).ok()?;
                let burst = u32::try_from(limit.get("burst")?.as_u64()?).ok()?;
                Some(Self::Limited {
                    host,
                    per_second,
                    burst,
                })
            }
            _ => None,
        }
    }

    /// The host pattern this entry matches.
    pub fn host(&self) -> &str {
        match self {
            Self::Pattern(host) | Self::Limited { host, .. } => host,
        }
    }
}

/// Why a guest HTTP call failed. Mapped onto the WIT `http-error` variant by
/// the host bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The request is outside the allowlist or resolves to a non-public
    /// address.
    Denied(String),
    /// The request exceeded its timeout, including time spent waiting for
    /// the rate limiter.
    Timeout,
    /// The response body exceeded the manifest cap.
    BodyTooLarge,
    /// Any other transport or protocol failure.
    Network(String),
}

/// A request as the guest built it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// HTTP method, for example `GET`.
    pub method: String,
    /// Absolute URL.
    pub url: String,
    /// Request headers in the order the guest supplied them.
    pub headers: Vec<(String, String)>,
    /// Request body, if any.
    pub body: Option<Vec<u8>>,
    /// Requested timeout in milliseconds.
    pub timeout_ms: Option<u32>,
}

/// A response returned to the guest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// Response headers. Values that are not valid UTF-8 are lossily
    /// converted.
    pub headers: Vec<(String, String)>,
    /// Response body, at most the manifest cap.
    pub body: Vec<u8>,
}

/// Token bucket that spaces guest requests according to the manifest's
/// `rateLimit`.
///
/// The bucket starts full. [`Limiter::take`] reserves one token and returns
/// how long the caller must wait before the reservation is due; reservations
/// may drive the balance negative so that concurrent waiters queue in order.
#[derive(Debug, Clone)]
pub struct Limiter {
    per_second: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
}

impl Limiter {
    /// Creates a full bucket that refills at `per_second` tokens per second
    /// up to `burst` tokens, starting its clock at `now`.
    ///
    /// Both parameters must be at least 1; the manifest schema enforces it.
    pub fn new(per_second: u32, burst: u32, now: Instant) -> Self {
        Self {
            per_second: f64::from(per_second.max(1)),
            burst: f64::from(burst.max(1)),
            tokens: f64::from(burst.max(1)),
            last: now,
        }
    }

    /// Reserves one token at time `now` and returns the wait before it is
    /// available. A zero duration means the request may proceed at once.
    pub fn take(&mut self, now: Instant) -> Duration {
        if now > self.last {
            let elapsed = now.duration_since(self.last).as_secs_f64();
            self.tokens = (self.tokens + elapsed * self.per_second).min(self.burst);
            self.last = now;
        }
        self.tokens -= 1.0;
        if self.tokens < 0.0 {
            Duration::from_secs_f64(-self.tokens / self.per_second)
        } else {
            Duration::ZERO
        }
    }

    /// Returns one reserved token, for a request that gave up waiting.
    pub fn refund(&mut self) {
        self.tokens = (self.tokens + 1.0).min(self.burst);
    }
}

/// Inputs that shape a [`Policy`]. Built from the manifest by
/// [`Policy::from_manifest`]; tests construct it directly.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Parsed `permissions.http.hosts` entries.
    pub hosts: Vec<HostEntry>,
    /// `(perSecond, burst)` from `permissions.http.rateLimit`, if declared.
    pub rate: Option<(u32, u32)>,
    /// Response body cap in bytes.
    pub max_body: u64,
    /// Default `User-Agent` used when the guest sends none.
    pub user_agent: String,
}

/// The outbound HTTP policy of one plugin run.
pub struct Policy {
    rules: Vec<Rule>,
    /// One slot per rule: the rule's own limiter when it declared one.
    limiters: Vec<Option<Limiter>>,
    limiter: Option<Limiter>,
    max_body: u64,
    user_agent: String,
    client: reqwest::blocking::Client,
}

/// A URL that passed the allowlist, and whether its host still needs the
/// public-address check (static rules do, `$config:` rules do not).
#[derive(Debug)]
struct Authorized {
    url: Url,
    resolve: bool,
    /// Index of the rule that allowed the host.
    rule: usize,
}

impl Policy {
    /// Builds the policy from the manifest's `permissions.http` block.
    ///
    /// Missing `hosts` means no host is allowed; missing `rateLimit` means no
    /// throttling; missing `maxBodyBytes` means [`DEFAULT_MAX_BODY`]. The
    /// default `User-Agent` names the plugin.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn from_manifest(manifest: &Value, user_agent: String) -> Result<Self> {
        let hosts = manifest
            .pointer("/permissions/http/hosts")
            .and_then(Value::as_array)
            .map(|entries| entries.iter().filter_map(HostEntry::from_json).collect())
            .unwrap_or_default();
        let rate = manifest
            .pointer("/permissions/http/rateLimit")
            .and_then(|limit| {
                let per_second = limit.get("perSecond")?.as_u64()?;
                let burst = limit.get("burst")?.as_u64()?;
                Some((u32::try_from(per_second).ok()?, u32::try_from(burst).ok()?))
            });
        let max_body = manifest
            .pointer("/permissions/http/maxBodyBytes")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_MAX_BODY);
        Self::new(
            Settings {
                hosts,
                rate,
                max_body,
                user_agent,
            },
            Instant::now(),
        )
    }

    /// Builds the policy from explicit settings, starting the rate limiter at
    /// `now`.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(settings: Settings, now: Instant) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .redirect(redirect::Policy::none())
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            rules: settings
                .hosts
                .iter()
                .map(|entry| Rule::parse(entry.host()))
                .collect(),
            limiters: settings
                .hosts
                .iter()
                .map(|entry| match entry {
                    HostEntry::Pattern(_) => None,
                    HostEntry::Limited {
                        per_second, burst, ..
                    } => Some(Limiter::new(*per_second, *burst, now)),
                })
                .collect(),
            limiter: settings
                .rate
                .map(|(per_second, burst)| Limiter::new(per_second, burst, now)),
            max_body: settings.max_body,
            user_agent: settings.user_agent,
            client,
        })
    }

    /// Performs a guest request, following up to five redirects.
    ///
    /// Each hop is authorized against the allowlist, checked for public
    /// addresses when its rule is static, throttled, and sent with the
    /// request timeout. Guest headers are forwarded on every hop.
    /// `config` is the server configuration that `$config:` rules read.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Denied`] for disallowed hosts, schemes, or
    /// non-public addresses; [`Failure::Timeout`] when the timeout elapses
    /// (including while queued by the rate limiter); [`Failure::BodyTooLarge`]
    /// when the body exceeds the cap; and [`Failure::Network`] for other
    /// transport failures, invalid requests, or too many redirects.
    pub fn send(&mut self, request: Request, config: &Value) -> Result<Response, Failure> {
        let timeout = timeout_for(request.timeout_ms);
        let mut method = request.method;
        let mut url = request.url;
        let mut body = request.body;
        let headers = request.headers;
        let forward_user_agent = !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("user-agent"));

        for hop in 0..=MAX_REDIRECTS {
            let authorized = authorize(&self.rules, &url, config)?;
            if authorized.resolve {
                check_public(&authorized.url)?;
            }
            self.throttle(Some(authorized.rule), timeout)?;

            let verb = reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|error| Failure::Network(format!("invalid method: {error}")))?;
            let mut builder = self
                .client
                .request(verb, authorized.url.clone())
                .timeout(timeout);
            for (name, value) in &headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            if forward_user_agent {
                builder = builder.header(reqwest::header::USER_AGENT, self.user_agent.as_str());
            }
            if let Some(bytes) = &body {
                builder = builder.body(bytes.clone());
            }

            let mut response = builder.send().map_err(transport)?;
            let status = response.status();
            if status.is_redirection()
                && let Some(location) = response.headers().get(reqwest::header::LOCATION)
            {
                if hop == MAX_REDIRECTS {
                    return Err(Failure::Network("too many redirects".to_owned()));
                }
                let target = location
                    .to_str()
                    .map_err(|error| Failure::Network(format!("invalid redirect: {error}")))?;
                let next = response
                    .url()
                    .join(target)
                    .map_err(|error| Failure::Network(format!("invalid redirect: {error}")))?;
                if matches!(status.as_u16(), 301..=303) {
                    method = "GET".to_owned();
                    body = None;
                }
                url = next.to_string();
                continue;
            }

            let headers = response
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let body = self.read_body(&mut response)?;
            return Ok(Response {
                status: status.as_u16(),
                headers,
                body,
            });
        }
        Err(Failure::Network("too many redirects".to_owned()))
    }

    /// Waits for the rate limiter that applies to the matched rule, or fails
    /// with [`Failure::Timeout`] when the wait would exceed `timeout`.
    ///
    /// A rule that declared its own `rateLimit` waits on that limiter only; any
    /// other rule waits on the instance-wide limiter.
    fn throttle(&mut self, rule: Option<usize>, timeout: Duration) -> Result<(), Failure> {
        let per_host = rule
            .and_then(|index| self.limiters.get_mut(index))
            .and_then(|slot| slot.as_mut());
        let limiter = per_host.or(self.limiter.as_mut());
        let Some(limiter) = limiter else {
            return Ok(());
        };
        let wait = limiter.take(Instant::now());
        if wait > timeout {
            limiter.refund();
            return Err(Failure::Timeout);
        }
        if !wait.is_zero() {
            thread::sleep(wait);
        }
        Ok(())
    }

    /// Reads the response body, rejecting it when it exceeds the cap either
    /// by its declared length or by the bytes actually received.
    fn read_body(&self, response: &mut reqwest::blocking::Response) -> Result<Vec<u8>, Failure> {
        if response
            .content_length()
            .is_some_and(|declared| declared > self.max_body)
        {
            return Err(Failure::BodyTooLarge);
        }
        let mut body = Vec::new();
        (&mut *response)
            .take(self.max_body + 1)
            .read_to_end(&mut body)
            .map_err(io_failure)?;
        if body.len() as u64 > self.max_body {
            return Err(Failure::BodyTooLarge);
        }
        Ok(body)
    }
}

/// Resolves a guest's requested timeout: the request value or
/// [`DEFAULT_TIMEOUT`], capped at [`MAX_TIMEOUT`].
pub fn timeout_for(requested: Option<u32>) -> Duration {
    requested
        .map(|ms| Duration::from_millis(u64::from(ms)))
        .unwrap_or(DEFAULT_TIMEOUT)
        .min(MAX_TIMEOUT)
}

/// Checks a URL against the allowlist without touching the network.
///
/// Only `http` and `https` are allowed. The host must match a rule: literal
/// equality, `*.domain` for a proper subdomain, or a `$config:` rule whose
/// configured value names the host. Static matches still need the
/// public-address check. The matched rule's index is returned so the caller
/// can pick that rule's own rate limit.
fn authorize(rules: &[Rule], url: &str, config: &Value) -> Result<Authorized, Failure> {
    let parsed =
        Url::parse(url).map_err(|error| Failure::Network(format!("invalid url: {error}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(Failure::Denied(format!("scheme {other} is not allowed"))),
    }
    let host = parsed
        .host_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| Failure::Denied("url has no host".to_owned()))?;

    let mut matched: Option<(usize, bool)> = None;
    for (index, rule) in rules.iter().enumerate() {
        match rule {
            Rule::Literal(name) if *name == host => matched = Some((index, true)),
            Rule::Wildcard(domain) if host.ends_with(&format!(".{domain}")) => {
                matched = Some((index, true));
            }
            Rule::Config(field) => {
                let configured = config.get(field).and_then(Value::as_str);
                if configured
                    .and_then(config_host)
                    .is_some_and(|name| name == host)
                {
                    matched = matched.or(Some((index, false)));
                }
            }
            _ => {}
        }
        if matched.is_some_and(|(_, resolve)| resolve) {
            break;
        }
    }
    match matched {
        Some((rule, resolve)) => Ok(Authorized {
            url: parsed,
            resolve,
            rule,
        }),
        None => Err(Failure::Denied(format!(
            "host {host} is not in the allowlist"
        ))),
    }
}

/// Extracts the host from a `$config:` value, which may be a bare host name
/// or a URL.
fn config_host(value: &str) -> Option<String> {
    let host = if value.contains("://") {
        Url::parse(value).ok()?.host_str()?.to_owned()
    } else {
        value.trim().to_owned()
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Fails with [`Failure::Denied`] when the URL's host resolves to any
/// address that is not public.
fn check_public(url: &Url) -> Result<(), Failure> {
    let host = url
        .host_str()
        .ok_or_else(|| Failure::Denied("url has no host".to_owned()))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| Failure::Network(format!("resolving {host}: {error}")))?;
    let mut any = false;
    for address in addresses {
        any = true;
        if !is_public(address.ip()) {
            return Err(Failure::Denied(format!(
                "host {host} resolves to a non-public address"
            )));
        }
    }
    if any {
        Ok(())
    } else {
        Err(Failure::Network(format!("resolving {host}: no addresses")))
    }
}

/// Reports whether `ip` is a publicly routable address.
///
/// Loopback, private, link-local, unspecified, broadcast, multicast, and
/// shared (`100.64.0.0/10`) IPv4 addresses are not public, nor are loopback,
/// unspecified, multicast, unique-local (`fc00::/7`), and link-local
/// (`fe80::/10`) IPv6 addresses. IPv4-mapped IPv6 addresses take the IPv4
/// answer.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            is_public_v6(v6)
        }
    }
}

/// IPv4 half of [`is_public`].
fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [first, second, ..] = ip.octets();
    let shared = first == 100 && (second & 0xc0) == 64;
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || shared)
}

/// IPv6 half of [`is_public`], after IPv4-mapped addresses are handled.
fn is_public_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    let unique_local = (first & 0xfe00) == 0xfc00;
    let link_local = (first & 0xffc0) == 0xfe80;
    !(ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || unique_local || link_local)
}

/// Maps a reqwest failure onto the guest-visible failure kinds.
fn transport(error: reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Timeout
    } else {
        Failure::Network(error.to_string())
    }
}

/// Maps an I/O failure while reading a body onto the guest-visible failure
/// kinds.
fn io_failure(error: io::Error) -> Failure {
    if error.kind() == io::ErrorKind::TimedOut {
        Failure::Timeout
    } else {
        Failure::Network(error.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use serde_json::json;

    use super::*;

    fn rules(entries: &[&str]) -> Vec<Rule> {
        entries.iter().map(|entry| Rule::parse(entry)).collect()
    }

    #[test]
    fn public_addresses_are_classified() {
        let public = [
            IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0, 0, 0, 0, 0, 0x1111)),
        ];
        for ip in public {
            assert!(is_public(ip), "{ip} should be public");
        }
        let private = [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V4(Ipv4Addr::BROADCAST),
            IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv4Addr::new(10, 1, 2, 3).to_ipv6_mapped()),
        ];
        for ip in private {
            assert!(!is_public(ip), "{ip} should not be public");
        }
    }

    #[test]
    fn literal_and_wildcard_rules_allow_matching_hosts() {
        let rules = rules(&["api.example.com", "*.cdn.example.com"]);
        let config = json!({});
        let literal = authorize(&rules, "https://API.example.com/x", &config).unwrap();
        assert!(literal.resolve);
        let sub = authorize(&rules, "https://a.b.cdn.example.com/", &config).unwrap();
        assert!(sub.resolve);
    }

    #[test]
    fn wildcard_does_not_match_the_bare_domain() {
        let rules = rules(&["*.cdn.example.com"]);
        let denied = authorize(&rules, "https://cdn.example.com/", &json!({}));
        assert!(matches!(denied, Err(Failure::Denied(_))));
    }

    #[test]
    fn hosts_outside_the_allowlist_are_denied() {
        let rules = rules(&["api.example.com"]);
        let denied = authorize(&rules, "https://evil.example.org/", &json!({})).unwrap_err();
        assert_eq!(
            denied,
            Failure::Denied("host evil.example.org is not in the allowlist".to_owned())
        );
    }

    #[test]
    fn non_http_schemes_are_denied() {
        let rules = rules(&["api.example.com"]);
        let denied = authorize(&rules, "ftp://api.example.com/", &json!({})).unwrap_err();
        assert_eq!(
            denied,
            Failure::Denied("scheme ftp is not allowed".to_owned())
        );
    }

    #[test]
    fn config_rules_match_the_configured_host_without_resolution() {
        let rules = rules(&["$config:baseUrl"]);
        let config = json!({"baseUrl": "https://lan.example.net:8443/api"});
        let allowed = authorize(&rules, "https://lan.example.net:8443/x", &config).unwrap();
        assert!(!allowed.resolve);
        let denied = authorize(&rules, "https://other.example.net/", &config);
        assert!(matches!(denied, Err(Failure::Denied(_))));
    }

    #[test]
    fn config_rules_accept_a_bare_host() {
        let rules = rules(&["$config:server"]);
        let config = json!({"server": "Music.Local"});
        let allowed = authorize(&rules, "http://music.local/", &config).unwrap();
        assert!(!allowed.resolve);
    }

    #[test]
    fn timeouts_default_and_cap() {
        assert_eq!(timeout_for(None), DEFAULT_TIMEOUT);
        assert_eq!(timeout_for(Some(250)), Duration::from_millis(250));
        assert_eq!(timeout_for(Some(600_000)), MAX_TIMEOUT);
    }

    #[test]
    fn limiter_queues_reservations_in_order() {
        let start = Instant::now();
        let mut limiter = Limiter::new(1, 1, start);
        assert_eq!(limiter.take(start), Duration::ZERO);
        assert_eq!(limiter.take(start), Duration::from_secs(1));
        assert_eq!(limiter.take(start), Duration::from_secs(2));
    }

    #[test]
    fn limiter_refills_with_time() {
        let start = Instant::now();
        let mut limiter = Limiter::new(2, 2, start);
        assert_eq!(limiter.take(start), Duration::ZERO);
        assert_eq!(limiter.take(start), Duration::ZERO);
        let later = start + Duration::from_secs(1);
        assert_eq!(limiter.take(later), Duration::ZERO);
    }

    #[test]
    fn refund_returns_a_token() {
        let start = Instant::now();
        let mut limiter = Limiter::new(1, 1, start);
        assert_eq!(limiter.take(start), Duration::ZERO);
        limiter.refund();
        assert_eq!(limiter.take(start), Duration::ZERO);
    }

    #[test]
    fn policy_reads_the_manifest_block() {
        let manifest = json!({
            "permissions": {
                "http": {
                    "hosts": ["api.example.com"],
                    "rateLimit": {"perSecond": 5, "burst": 10},
                    "maxBodyBytes": 1024
                }
            }
        });
        let policy = Policy::from_manifest(&manifest, "hsplugin".to_owned()).unwrap();
        assert_eq!(policy.max_body, 1024);
        assert_eq!(policy.rules, rules(&["api.example.com"]));
        assert!(policy.limiters.iter().all(Option::is_none));
        assert!(policy.limiter.is_some());
    }

    #[test]
    fn host_objects_carry_their_own_limit() {
        let manifest = json!({
            "permissions": {
                "http": {
                    "hosts": [
                        {"host": "musicbrainz.org", "rateLimit": {"perSecond": 1, "burst": 1}},
                        "*.example.com",
                        {"host": "broken.example.com"}
                    ],
                    "rateLimit": {"perSecond": 5, "burst": 5}
                }
            }
        });
        let policy = Policy::from_manifest(&manifest, "hsplugin".to_owned()).unwrap();
        assert_eq!(
            policy.rules,
            rules(&["musicbrainz.org", "*.example.com"]),
            "an object entry without rateLimit is ignored"
        );
        assert!(policy.limiters[0].is_some());
        assert!(policy.limiters[1].is_none());
        let matched = authorize(
            &policy.rules,
            "https://musicbrainz.org/ws/2/artist",
            &json!({}),
        )
        .unwrap();
        assert_eq!(matched.rule, 0);
        let other = authorize(&policy.rules, "https://a.example.com/", &json!({})).unwrap();
        assert_eq!(other.rule, 1);
    }

    #[test]
    fn host_entries_parse_strings_and_objects() {
        assert_eq!(
            HostEntry::from_json(&json!("api.example.com")),
            Some(HostEntry::Pattern("api.example.com".to_owned()))
        );
        assert_eq!(
            HostEntry::from_json(&json!({
                "host": "$config:baseUrl",
                "rateLimit": {"perSecond": 1, "burst": 1}
            })),
            Some(HostEntry::Limited {
                host: "$config:baseUrl".to_owned(),
                per_second: 1,
                burst: 1,
            })
        );
        assert_eq!(HostEntry::from_json(&json!(7)), None);
        assert_eq!(
            HostEntry::from_json(&json!({"host": "a.example.com"})),
            None
        );
    }

    #[test]
    fn empty_manifest_allows_no_hosts() {
        let policy = Policy::from_manifest(&json!({}), "hsplugin".to_owned()).unwrap();
        assert!(policy.rules.is_empty());
        assert!(policy.limiter.is_none());
        assert_eq!(policy.max_body, DEFAULT_MAX_BODY);
    }
}
