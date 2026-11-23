//! WebKit on macOS connects to a loopback address directly even when a proxy is configured. A
//! view therefore addresses such a remote host by a non-loopback alias, which WebKit sends to
//! the document endpoint, and the endpoint reads the alias back as the remote loopback host.
use std::net::{IpAddr, Ipv4Addr};

/// `*.localhost` names stay a secure context in WebKit and never resolve off the client.
const SUFFIX: &str = ".tasty.localhost";
const IPV6_LOOPBACK: &str = "ipv6-loopback";

/// The alias for a host WebKit would reach on the client itself, if it needs one.
pub(crate) fn alias_host(host: &str) -> Option<String> {
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let label = match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) if ip.is_loopback() || ip.is_unspecified() => {
            ip.to_string().replace('.', "-")
        }
        Ok(IpAddr::V6(ip)) if ip.is_loopback() => IPV6_LOOPBACK.to_owned(),
        Ok(_) => return None,
        Err(_) if bare.eq_ignore_ascii_case("localhost") => "localhost".to_owned(),
        Err(_) => return None,
    };
    Some(format!("{label}{SUFFIX}"))
}

/// The remote host an alias stands for. Anything else is not an alias.
pub(crate) fn logical_host(host: &str) -> Option<String> {
    let label = host.to_ascii_lowercase();
    let label = label.strip_suffix(SUFFIX)?;
    match label {
        "localhost" => Some("localhost".to_owned()),
        IPV6_LOOPBACK => Some("::1".to_owned()),
        _ => {
            let ip = label.replace('-', ".").parse::<Ipv4Addr>().ok()?;
            ((ip.is_loopback() || ip.is_unspecified()) && ip.to_string().replace('.', "-") == label)
                .then(|| ip.to_string())
        }
    }
}

fn map_url(url: &str, map: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut url = url::Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss") {
        return None;
    }
    let host = map(url.host_str()?)?;
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host
    };
    url.set_host(Some(&host)).ok()?;
    Some(url.into())
}

#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS WebKit view addresses hosts by alias"
    )
)]
/// `url` addressed through its alias, or None when the host needs none.
pub(crate) fn alias_url(url: &str) -> Option<String> {
    map_url(url, alias_host)
}

#[cfg_attr(
    not(any(target_os = "macos", test)),
    expect(
        dead_code,
        reason = "only the macOS WebKit view addresses hosts by alias"
    )
)]
/// `url` with an alias host read back as the remote host, or None when it has no alias.
pub(crate) fn logical_url(url: &str) -> Option<String> {
    map_url(url, logical_host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts_round_trip_through_their_alias() {
        for (url, aliased) in [
            (
                "http://localhost:8080/a?b#c",
                "http://localhost.tasty.localhost:8080/a?b#c",
            ),
            ("https://127.0.0.1/x", "https://127-0-0-1.tasty.localhost/x"),
            (
                "http://127.3.2.1:3000/",
                "http://127-3-2-1.tasty.localhost:3000/",
            ),
            (
                "http://0.0.0.0:5173/",
                "http://0-0-0-0.tasty.localhost:5173/",
            ),
            (
                "http://[::1]:8000/",
                "http://ipv6-loopback.tasty.localhost:8000/",
            ),
        ] {
            assert_eq!(alias_url(url).as_deref(), Some(aliased), "{url}");
            let logical = logical_url(aliased).unwrap();
            assert_eq!(url::Url::parse(&logical), url::Url::parse(url), "{aliased}");
        }
    }

    #[test]
    fn other_hosts_and_schemes_keep_their_address() {
        for url in [
            "http://example.test/",
            "http://10.0.0.5:8080/",
            "http://[2001:db8::1]/",
            "http://app.localhost/",
            "http://[::ffff:127.0.0.1]/",
            "file:///tmp/a.html",
            "about:blank",
        ] {
            assert_eq!(alias_url(url), None, "{url}");
            assert_eq!(logical_url(url), None, "{url}");
        }
    }

    #[test]
    fn only_exact_alias_labels_read_back() {
        for host in [
            "127-0-0-01.tasty.localhost",
            "10-0-0-1.tasty.localhost",
            "x.localhost.tasty.localhost",
            "localhost.tasty.localhost.evil.test",
            "tasty.localhost",
        ] {
            assert_eq!(logical_host(host), None, "{host}");
        }
        assert_eq!(
            logical_host("LOCALHOST.Tasty.Localhost").as_deref(),
            Some("localhost")
        );
    }
}
