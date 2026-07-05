use std::env;

use anyhow::{Context, Result, bail};
use boxlite::runtime::options::{BoxliteOptions, ImageRegistry, RegistryTransport};

pub fn load_boxlite_options() -> Result<BoxliteOptions> {
    if let Ok(json) = env::var("BLINK_BOXLITE_OPTIONS") {
        if !json.trim().is_empty() {
            return serde_json::from_str(&json)
                .context("failed to parse BLINK_BOXLITE_OPTIONS JSON");
        }
    }

    let mut options = BoxliteOptions::default();
    if let Ok(raw) = env::var("BLINK_IMAGE_REGISTRIES") {
        if !raw.trim().is_empty() {
            options.image_registries = parse_image_registries(&raw)?;
        }
    }
    Ok(options)
}

fn parse_image_registries(raw: &str) -> Result<Vec<ImageRegistry>> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_registry_entry)
        .collect()
}

fn parse_registry_entry(entry: &str) -> Result<ImageRegistry> {
    let (host_part, modifier) = match entry.split_once('@') {
        Some((host, modifier)) => (host.trim(), modifier.trim()),
        None => (entry.trim(), "https"),
    };
    if host_part.is_empty() {
        bail!("empty registry host in BLINK_IMAGE_REGISTRIES entry: {entry}");
    }

    let mut transport = RegistryTransport::Https;
    let mut search = false;
    let mut skip_verify = false;

    for token in modifier.split('+') {
        match token.trim() {
            "http" => transport = RegistryTransport::Http,
            "https" => transport = RegistryTransport::Https,
            "search" => search = true,
            "skip_verify" => skip_verify = true,
            "" => {}
            other => bail!("unknown registry modifier '{other}' in entry: {entry}"),
        }
    }

    Ok(ImageRegistry {
        host: host_part.to_string(),
        transport,
        skip_verify,
        search,
        auth: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_registry() {
        let registries = parse_image_registries("localhost:5000@http").unwrap();
        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0].host, "localhost:5000");
        assert_eq!(registries[0].transport, RegistryTransport::Http);
    }

    #[test]
    fn parse_multiple_registries() {
        let registries =
            parse_image_registries("localhost:5000@http,ghcr.io/myorg@https+search").unwrap();
        assert_eq!(registries.len(), 2);
        assert!(registries[1].search);
    }
}
