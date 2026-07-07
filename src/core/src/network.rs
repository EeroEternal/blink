//! Session network defaults and resolution for BoxLite sandboxes.
//!
//! Persistent sessions default to outbound network enabled so agents can reach
//! LLM APIs, OAuth endpoints, and package registries. Override per request via
//! `POST /api/sessions` `network`, or globally via `BLINK_NETWORK` /
//! `BLINK_ALLOW_NET`.

use std::env;

use anyhow::{Result, bail};
use boxlite::runtime::options::{NetworkConfig, NetworkMode, NetworkSpec};

/// Resolve the network spec for a new session box.
///
/// Request body wins over environment defaults. When neither is set, network is
/// **enabled** with full egress (`allow_net` empty).
pub fn resolve_network_spec(request: Option<NetworkConfig>) -> Result<NetworkSpec> {
    let config = request.unwrap_or_else(default_network_config);
    NetworkSpec::try_from(config).map_err(|err| anyhow::anyhow!("invalid network config: {err}"))
}

/// Default network configuration from environment (enabled unless opted out).
pub fn default_network_config() -> NetworkConfig {
    let mode = env::var("BLINK_NETWORK")
        .ok()
        .map(|raw| parse_network_mode(&raw))
        .transpose()
        .ok()
        .flatten()
        .unwrap_or(NetworkMode::Enabled);

    let allow_net = env::var("BLINK_ALLOW_NET")
        .ok()
        .map(|raw| parse_allow_net(&raw))
        .transpose()
        .ok()
        .flatten()
        .unwrap_or_default();

    NetworkConfig { mode, allow_net }
}

fn parse_network_mode(raw: &str) -> Result<NetworkMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "enabled" | "1" | "true" | "on" => Ok(NetworkMode::Enabled),
        "disabled" | "0" | "false" | "off" => Ok(NetworkMode::Disabled),
        other => bail!("invalid BLINK_NETWORK value {other:?}; expected enabled or disabled"),
    }
}

fn parse_allow_net(raw: &str) -> Result<Vec<String>> {
    let entries: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(String::from)
        .collect();
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled_full_egress() {
        let spec = resolve_network_spec(None).unwrap();
        match spec {
            NetworkSpec::Enabled { allow_net } => assert!(allow_net.is_empty()),
            NetworkSpec::Disabled => panic!("expected enabled network"),
        }
    }

    #[test]
    fn request_overrides_default() {
        let spec = resolve_network_spec(Some(NetworkConfig {
            mode: NetworkMode::Disabled,
            allow_net: Vec::new(),
        }))
        .unwrap();
        assert!(matches!(spec, NetworkSpec::Disabled));
    }

    #[test]
    fn allowlist_hosts() {
        let spec = resolve_network_spec(Some(NetworkConfig {
            mode: NetworkMode::Enabled,
            allow_net: vec!["auth.kimi.com".into(), "api.moonshot.cn".into()],
        }))
        .unwrap();
        match spec {
            NetworkSpec::Enabled { allow_net } => {
                assert_eq!(allow_net, vec!["auth.kimi.com", "api.moonshot.cn"]);
            }
            NetworkSpec::Disabled => panic!("expected enabled network"),
        }
    }

    #[test]
    fn disabled_with_allow_net_is_invalid() {
        let err = resolve_network_spec(Some(NetworkConfig {
            mode: NetworkMode::Disabled,
            allow_net: vec!["example.com".into()],
        }))
        .unwrap_err();
        assert!(err.to_string().contains("incompatible"));
    }

    #[test]
    fn parse_network_mode_values() {
        assert_eq!(parse_network_mode("enabled").unwrap(), NetworkMode::Enabled);
        assert_eq!(parse_network_mode("OFF").unwrap(), NetworkMode::Disabled);
        assert!(parse_network_mode("maybe").is_err());
    }

    #[test]
    fn parse_allow_net_csv() {
        let list = parse_allow_net(" auth.kimi.com , npmjs.org ").unwrap();
        assert_eq!(list, vec!["auth.kimi.com", "npmjs.org"]);
    }
}
