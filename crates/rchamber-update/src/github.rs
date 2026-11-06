use serde::Deserialize;
use url::Url;

pub const RELEASES_URL: &str = "https://github.com/rustchamber/rustchamber/releases";

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    #[serde(default)]
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub digest: Option<String>,
}

impl Release {
    pub fn version(&self) -> Option<semver::Version> {
        parse_tag(&self.tag_name)
    }

    pub fn notes(&self) -> String {
        match self.body.as_deref() {
            Some(body) if !body.trim().is_empty() => body.trim().to_string(),
            _ => self
                .name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "See the release notes on GitHub.".into()),
        }
    }

    pub fn macos_zip(&self) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|asset| {
                let name = asset.name.to_ascii_lowercase();
                name.ends_with(".zip") && name.contains("macos")
            })
            .or_else(|| {
                self.assets
                    .iter()
                    .find(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
            })
    }

    pub fn sha256(&self) -> Option<String> {
        self.macos_zip()
            .and_then(|asset| asset.digest.as_deref())
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .map(str::to_string)
    }

    pub fn download_size(&self) -> Option<u64> {
        self.macos_zip()
            .map(|asset| asset.size)
            .filter(|size| *size > 0)
    }
}

pub fn parse_tag(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches('v')).ok()
}

pub fn parse_releases(body: &[u8]) -> Result<Vec<Release>, String> {
    serde_json::from_slice(body).map_err(|error| format!("Could not read the update feed: {error}"))
}

pub fn pick_release(releases: &[Release], current: &semver::Version) -> Option<Release> {
    let mut best: Option<(semver::Version, Release)> = None;
    for release in releases {
        if release.draft {
            continue;
        }
        if current.pre.is_empty() && release.prerelease {
            continue;
        }
        let Some(version) = release.version() else {
            continue;
        };
        if version <= *current {
            continue;
        }
        if release.macos_zip().is_none() {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(best_version, _)| version > *best_version)
        {
            best = Some((version, release.clone()));
        }
    }
    best.map(|(_, release)| release)
}

pub fn release_url(release: &Release) -> String {
    if !release.html_url.is_empty() {
        return release.html_url.clone();
    }
    let tag = release.tag_name.trim();
    if tag.is_empty() {
        RELEASES_URL.into()
    } else {
        format!("{RELEASES_URL}/tag/{tag}")
    }
}

pub fn asset_url(release: &Release) -> Option<Url> {
    release
        .macos_zip()
        .and_then(|asset| Url::parse(&asset.browser_download_url).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, assets: &[(&str, u64)]) -> Release {
        Release {
            tag_name: tag.into(),
            name: None,
            body: Some("notes".into()),
            html_url: format!("https://example.test/{tag}"),
            draft: false,
            prerelease,
            published_at: None,
            assets: assets
                .iter()
                .map(|(name, size)| Asset {
                    name: (*name).into(),
                    browser_download_url: format!("https://example.test/{name}"),
                    size: *size,
                    digest: None,
                })
                .collect(),
        }
    }

    const ZIP: &str = "RustChamber-0.2.0-macos-universal.zip";

    #[test]
    fn parses_a_v_prefixed_tag() {
        assert_eq!(
            parse_tag("v0.2.0-beta.1"),
            Some(semver::Version::parse("0.2.0-beta.1").unwrap())
        );
        assert_eq!(parse_tag("nightly"), None);
    }

    #[test]
    fn ignores_older_drafts_and_missing_platform_assets() {
        let current = semver::Version::parse("0.1.0-beta").unwrap();
        let mut draft = release("v0.3.0", false, &[(ZIP, 10)]);
        draft.draft = true;
        let mut no_asset = release("v0.4.0", false, &[("source.tar.gz", 10)]);
        no_asset.assets.clear();
        let releases = vec![
            draft,
            release("v0.1.0-beta", false, &[(ZIP, 10)]),
            no_asset,
            release("v0.2.0", false, &[(ZIP, 10)]),
        ];
        let picked = pick_release(&releases, &current).unwrap();
        assert_eq!(picked.tag_name, "v0.2.0");
    }

    #[test]
    fn prerelease_builds_see_prereleases() {
        let current = semver::Version::parse("0.1.0-beta").unwrap();
        let releases = vec![
            release("v0.2.0-beta.1", true, &[(ZIP, 10)]),
            release("v0.1.0", false, &[(ZIP, 10)]),
        ];
        assert_eq!(
            pick_release(&releases, &current).map(|r| r.tag_name),
            Some("v0.2.0-beta.1".into())
        );
    }

    #[test]
    fn stable_builds_ignore_prereleases() {
        let current = semver::Version::parse("0.2.0").unwrap();
        let releases = vec![
            release("v0.3.0-beta.1", true, &[(ZIP, 10)]),
            release("v0.2.1", false, &[(ZIP, 10)]),
        ];
        assert_eq!(
            pick_release(&releases, &current).map(|r| r.tag_name),
            Some("v0.2.1".into())
        );
    }

    #[test]
    fn picks_the_universal_macos_zip() {
        let release = release(
            "v0.2.0",
            false,
            &[
                ("source.tar.gz", 1),
                ("RustChamber-0.2.0-macos-universal.zip", 42),
            ],
        );
        assert_eq!(release.download_size(), Some(42));
        assert_eq!(release.macos_zip().unwrap().size, 42);
    }

    #[test]
    fn reads_the_sha256_digest() {
        let mut release = release("v0.2.0", false, &[(ZIP, 42)]);
        release.assets[0].digest = Some(format!("sha256:{}", "a".repeat(64)));
        assert_eq!(release.sha256(), Some("a".repeat(64)));
    }

    #[test]
    fn notes_fall_back_to_the_release_name() {
        let mut release = release("v0.2.0", false, &[(ZIP, 1)]);
        release.body = Some("  ".into());
        release.name = Some("RustChamber 0.2.0".into());
        assert_eq!(release.notes(), "RustChamber 0.2.0");
    }
}
