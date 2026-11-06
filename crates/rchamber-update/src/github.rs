use serde::Deserialize;

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

    pub fn macos_dmg(&self) -> Option<&Asset> {
        self.assets.iter().find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.ends_with(".dmg") && name.contains("macos")
        })
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

pub fn parse_atom(body: &str) -> Result<Vec<Release>, String> {
    let mut releases = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<entry>") {
        rest = &rest[start + "<entry>".len()..];
        let Some(end) = rest.find("</entry>") else {
            break;
        };
        let entry = &rest[..end];
        rest = &rest[end + "</entry>".len()..];
        let Some(link) = release_link(entry) else {
            continue;
        };
        let Some(tag) = link.rsplit('/').next().filter(|tag| tag.starts_with('v')) else {
            continue;
        };
        let Some(version) = parse_tag(tag) else {
            continue;
        };
        let title = find_text(entry, "title")
            .map(|title| decode_entities(&title))
            .filter(|title| !title.trim().is_empty());
        let updated = find_text(entry, "updated").map(|value| decode_entities(&value));
        let notes = find_text(entry, "content")
            .map(|content| html_to_text(&content))
            .filter(|notes| !notes.trim().is_empty());
        let dmg_name = format!("RustChamber-{version}-macos-universal.dmg");
        let zip_name = format!("RustChamber-{version}-macos-universal.zip");
        let dmg_url = format!("{RELEASES_URL}/download/{tag}/{dmg_name}");
        let zip_url = format!("{RELEASES_URL}/download/{tag}/{zip_name}");
        releases.push(Release {
            tag_name: tag.to_string(),
            name: title,
            body: notes,
            html_url: link,
            draft: false,
            prerelease: !version.pre.is_empty(),
            published_at: updated,
            assets: vec![
                Asset {
                    name: dmg_name,
                    browser_download_url: dmg_url,
                    size: 0,
                    digest: None,
                },
                Asset {
                    name: zip_name,
                    browser_download_url: zip_url,
                    size: 0,
                    digest: None,
                },
            ],
        });
    }
    if releases.is_empty() {
        Err("The release feed has no entries.".into())
    } else {
        Ok(releases)
    }
}

fn release_link(entry: &str) -> Option<String> {
    let mut rest = entry;
    while let Some(start) = rest.find("<link") {
        let after = &rest[start..];
        let end = after.find('>')?;
        let tag = &after[..end];
        if let Some(href) = attr(tag, "href")
            && href.contains("/releases/tag/")
        {
            return Some(decode_entities(&href));
        }
        rest = &after[end + 1..];
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

fn find_text(entry: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let start = entry.find(&open)?;
    let after = &entry[start..];
    let gt = after.find('>')?;
    let inner = start + gt + 1;
    let close = format!("</{name}>");
    let end = entry[inner..].find(&close)?;
    Some(entry[inner..inner + end].to_string())
}

fn decode_entities(value: &str) -> String {
    value
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

fn html_to_text(raw: &str) -> String {
    let decoded = decode_entities(raw);
    let mut text = decoded
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<li>", "- ");
    for tag in [
        "</p>",
        "</li>",
        "</ul>",
        "</ol>",
        "</h1>",
        "</h2>",
        "</h3>",
        "</h4>",
        "</blockquote>",
        "</div>",
    ] {
        text = text.replace(tag, "\n");
    }
    let mut stripped = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => stripped.push(character),
            _ => {}
        }
    }
    let mut result = String::new();
    let mut blank = 0;
    for line in stripped.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        result.push_str(line);
        result.push('\n');
    }
    result.trim().to_string()
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
        assert_eq!(release.macos_zip().unwrap().size, 42);
        assert!(release.macos_dmg().is_none());
    }

    #[test]
    fn selects_the_dmg_asset() {
        let release = release(
            "v0.2.0",
            false,
            &[
                ("RustChamber-0.2.0-macos-universal.zip", 42),
                ("RustChamber-0.2.0-macos-universal.dmg", 64),
            ],
        );
        assert_eq!(release.macos_dmg().unwrap().size, 64);
        assert_eq!(release.macos_zip().unwrap().size, 42);
    }

    #[test]
    fn reads_the_sha256_digest() {
        let mut release = release("v0.2.0", false, &[(ZIP, 42)]);
        release.assets[0].digest = Some(format!("sha256:{}", "a".repeat(64)));
        assert_eq!(
            release.macos_zip().unwrap().digest.as_deref(),
            Some(format!("sha256:{}", "a".repeat(64)).as_str())
        );
    }

    #[test]
    fn notes_fall_back_to_the_release_name() {
        let mut release = release("v0.2.0", false, &[(ZIP, 1)]);
        release.body = Some("  ".into());
        release.name = Some("RustChamber 0.2.0".into());
        assert_eq!(release.notes(), "RustChamber 0.2.0");
    }

    #[test]
    fn parses_the_atom_feed_as_a_fallback() {
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom">
<entry>
  <updated>2026-11-06T12:17:12Z</updated>
  <link rel="alternate" type="text/html" href="https://github.com/rustchamber/rustchamber/releases/tag/v0.2.1-beta"/>
  <title>RustChamber 0.2.1-beta</title>
  <content type="html">&lt;p&gt;Dock icon fix&lt;/p&gt;&lt;ul&gt;&lt;li&gt;Set the icon at runtime&lt;/li&gt;&lt;/ul&gt;</content>
</entry>
<entry>
  <updated>2026-11-05T15:00:00Z</updated>
  <link rel="alternate" type="text/html" href="https://github.com/rustchamber/rustchamber/releases/tag/v0.2.0-beta"/>
  <title>RustChamber 0.2.0-beta</title>
  <content type="html">&lt;p&gt;In-app updates&lt;/p&gt;</content>
</entry>
</feed>"#;
        let releases = parse_atom(atom).unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].tag_name, "v0.2.1-beta");
        assert!(releases[0].notes().contains("Dock icon fix"));
        assert!(releases[0].notes().contains("- Set the icon at runtime"));
        assert_eq!(releases[0].macos_zip().unwrap().size, 0);
        assert_eq!(
            releases[0].macos_dmg().unwrap().browser_download_url,
            "https://github.com/rustchamber/rustchamber/releases/download/v0.2.1-beta/RustChamber-0.2.1-beta-macos-universal.dmg"
        );
        assert_eq!(
            releases[0].macos_zip().unwrap().browser_download_url,
            "https://github.com/rustchamber/rustchamber/releases/download/v0.2.1-beta/RustChamber-0.2.1-beta-macos-universal.zip"
        );
        let current = semver::Version::parse("0.2.0-beta").unwrap();
        assert_eq!(
            pick_release(&releases, &current)
                .and_then(|release| release.version())
                .map(|version| version.to_string()),
            Some("0.2.1-beta".into())
        );
    }
}
