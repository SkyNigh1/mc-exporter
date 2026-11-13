//! Checks GitHub for a newer release (once, at startup, in the background).

pub const RELEASES_PAGE: &str = "https://github.com/SkyNigh1/mc-exporter/releases/latest";
const API: &str = "https://api.github.com/repos/SkyNigh1/mc-exporter/releases/latest";

pub struct Update {
    pub version: String,
    pub url: String,
}

fn parse(v: &str) -> Vec<u32> {
    v.trim_start_matches('v').split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect()
}

/// true when `a` is a later version than `b` ("0.10.0" > "0.9.2")
pub fn newer(a: &str, b: &str) -> bool {
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// The latest release if it is newer than this build
pub fn check() -> Option<Update> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(6)).build();
    let resp = agent
        .get(API)
        .set("User-Agent", concat!("mc-exporter/", env!("CARGO_PKG_VERSION")))
        .set("Accept", "application/vnd.github+json")
        .call()
        .ok()?;
    let j: serde_json::Value = serde_json::from_reader(resp.into_reader()).ok()?;
    let tag = j.get("tag_name")?.as_str()?.to_string();
    let url = j.get("html_url").and_then(|u| u.as_str()).unwrap_or(RELEASES_PAGE).to_string();
    newer(&tag, env!("CARGO_PKG_VERSION")).then(|| Update { version: tag.trim_start_matches('v').to_string(), url })
}
