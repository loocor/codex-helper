const LOOPBACK_NO_PROXY_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

pub fn configure_process_loopback_no_proxy() {
    let no_proxy = loopback_no_proxy_value();
    std::env::set_var("NO_PROXY", &no_proxy);
    std::env::set_var("no_proxy", no_proxy);
}

pub fn loopback_no_proxy_value() -> String {
    let existing = std::env::var("NO_PROXY")
        .ok()
        .or_else(|| std::env::var("no_proxy").ok());
    merge_loopback_no_proxy(existing.as_deref())
}

fn merge_loopback_no_proxy(existing: Option<&str>) -> String {
    let mut entries = existing
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    for host in LOOPBACK_NO_PROXY_HOSTS {
        if !entries.iter().any(|entry| entry == host) {
            entries.push(host.to_string());
        }
    }
    entries.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_proxy_value_adds_loopback_hosts() {
        assert_eq!(
            merge_loopback_no_proxy(Some("example.com")),
            "example.com,127.0.0.1,localhost,::1"
        );
    }

    #[test]
    fn no_proxy_value_keeps_existing_loopback_hosts() {
        assert_eq!(
            merge_loopback_no_proxy(Some("localhost, example.com,::1,127.0.0.1")),
            "localhost,example.com,::1,127.0.0.1"
        );
    }
}
