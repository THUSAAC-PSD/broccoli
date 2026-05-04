use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum_client_ip::ClientIpSource;
use ipnet::IpNet;
use tracing::warn;

const X_FORWARDED_FOR: &str = "x-forwarded-for";

pub fn parse_trusted_proxy_networks(entries: &[String]) -> Arc<Vec<IpNet>> {
    Arc::new(
        entries
            .iter()
            .filter_map(|entry| match parse_trusted_proxy_entry(entry) {
                Ok(net) => Some(net),
                Err(error) => {
                    warn!(
                        trusted_proxy = %entry,
                        %error,
                        "Ignoring invalid trusted proxy CIDR"
                    );
                    None
                }
            })
            .collect(),
    )
}

fn parse_trusted_proxy_entry(entry: &str) -> Result<IpNet, String> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err("empty entry".to_string());
    }

    trimmed
        .parse::<IpNet>()
        .or_else(|_| trimmed.parse::<IpAddr>().map(IpNet::from))
        .map_err(|error| format!("{error}"))
}

pub async fn client_ip_source_middleware(
    State(trusted_proxies): State<Arc<Vec<IpNet>>>,
    mut request: Request,
    next: Next,
) -> Response {
    let source = select_client_ip_source(&request, trusted_proxies.as_slice());
    request.extensions_mut().insert(source);
    next.run(request).await
}

fn select_client_ip_source<B>(
    request: &axum::http::Request<B>,
    trusted_proxies: &[IpNet],
) -> ClientIpSource {
    if trusted_proxies.is_empty() || !request.headers().contains_key(X_FORWARDED_FOR) {
        return ClientIpSource::ConnectInfo;
    }

    let remote_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());

    match remote_ip {
        Some(ip) if trusted_proxies.iter().any(|net| net.contains(&ip)) => {
            ClientIpSource::RightmostXForwardedFor
        }
        _ => ClientIpSource::ConnectInfo,
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    fn request(remote: SocketAddr, x_forwarded_for: Option<&str>) -> axum::http::Request<Body> {
        let mut builder = axum::http::Request::builder().uri("/");
        if let Some(value) = x_forwarded_for {
            builder = builder.header(X_FORWARDED_FOR, value);
        }
        let mut request = builder.body(Body::empty()).unwrap();
        request.extensions_mut().insert(ConnectInfo(remote));
        request
    }

    #[test]
    fn parses_cidr_and_exact_ip_entries() {
        let nets = parse_trusted_proxy_networks(&[
            "10.0.0.0/8".to_string(),
            "192.0.2.10".to_string(),
            "2001:db8::/32".to_string(),
        ]);

        assert!(
            nets.iter()
                .any(|net| net.contains(&"10.1.2.3".parse::<IpAddr>().unwrap()))
        );
        assert!(
            nets.iter()
                .any(|net| net.contains(&"192.0.2.10".parse::<IpAddr>().unwrap()))
        );
        assert!(
            nets.iter()
                .any(|net| net.contains(&"2001:db8::1".parse::<IpAddr>().unwrap()))
        );
    }

    #[test]
    fn invalid_trusted_proxy_entries_are_ignored() {
        let nets = parse_trusted_proxy_networks(&["not-a-cidr".to_string()]);
        assert!(nets.is_empty());
    }

    #[test]
    fn empty_trusted_proxy_list_uses_socket_address_even_with_xff() {
        let request = request("127.0.0.1:12345".parse().unwrap(), Some("203.0.113.10"));
        assert_eq!(
            select_client_ip_source(&request, &[]),
            ClientIpSource::ConnectInfo
        );
    }

    #[test]
    fn trusted_proxy_connection_uses_x_forwarded_for() {
        let request = request("10.0.0.4:12345".parse().unwrap(), Some("203.0.113.10"));
        let trusted = vec!["10.0.0.0/8".parse::<IpNet>().unwrap()];
        assert_eq!(
            select_client_ip_source(&request, &trusted),
            ClientIpSource::RightmostXForwardedFor
        );
    }

    #[test]
    fn untrusted_proxy_connection_uses_socket_address() {
        let request = request("192.0.2.4:12345".parse().unwrap(), Some("203.0.113.10"));
        let trusted = vec!["10.0.0.0/8".parse::<IpNet>().unwrap()];
        assert_eq!(
            select_client_ip_source(&request, &trusted),
            ClientIpSource::ConnectInfo
        );
    }
}
