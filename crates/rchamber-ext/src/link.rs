use std::path::PathBuf;

use url::Url;

pub const INSTALL_SCHEME: &str = "rchamber://extensions/install";
pub const SESSION_SCHEME: &str = "rchamber://session";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallLink {
    Path(PathBuf),
    Url(String),
}

impl InstallLink {
    pub fn source(&self) -> String {
        match self {
            InstallLink::Path(path) => path.to_string_lossy().to_string(),
            InstallLink::Url(url) => url.clone(),
        }
    }
}

pub fn parse_install_link(href: &str) -> Option<InstallLink> {
    let url = Url::parse(href).ok()?;
    if url.scheme() != "rchamber" || url.host_str() != Some("extensions") {
        return None;
    }
    if url.path() != "/install" {
        return None;
    }
    let mut path = None;
    let mut remote = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "path" => path = Some(value.to_string()),
            "url" => remote = Some(value.to_string()),
            _ => {}
        }
    }
    if let Some(path) = path {
        if !path.starts_with('/') && !is_windows_absolute(&path) {
            return None;
        }
        return Some(InstallLink::Path(PathBuf::from(path)));
    }
    let remote = remote?;
    let parsed = Url::parse(&remote).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    Some(InstallLink::Url(remote))
}

pub fn parse_session_link(href: &str) -> Option<String> {
    let url = Url::parse(href).ok()?;
    if url.scheme() != "rchamber" || url.host_str() != Some("session") {
        return None;
    }
    let id = url.path().trim_start_matches('/').to_string();
    if id.is_empty() || id.contains('/') || !id.starts_with("ses") {
        return None;
    }
    Some(id)
}

fn is_windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_folder_installs() {
        let link = parse_install_link(
            "rchamber://extensions/install?path=/Users/me/rustchamber-extensions/deploys",
        )
        .unwrap();
        assert_eq!(
            link,
            InstallLink::Path(PathBuf::from("/Users/me/rustchamber-extensions/deploys"))
        );
    }

    #[test]
    fn decodes_spaces() {
        let link =
            parse_install_link("rchamber://extensions/install?path=/Users/me/my%20ext").unwrap();
        assert_eq!(link.source(), "/Users/me/my ext");
    }

    #[test]
    fn refuses_relative_paths_and_http_urls() {
        assert!(parse_install_link("rchamber://extensions/install?path=relative").is_none());
        assert!(
            parse_install_link("rchamber://extensions/install?url=http://example.com/a.zip")
                .is_none()
        );
    }

    #[test]
    fn accepts_https_urls() {
        assert!(matches!(
            parse_install_link("rchamber://extensions/install?url=https%3A%2F%2Fexample.com%2Fa"),
            Some(InstallLink::Url(_))
        ));
    }

    #[test]
    fn parses_session_links() {
        assert_eq!(
            parse_session_link("rchamber://session/ses_abc123").as_deref(),
            Some("ses_abc123")
        );
        assert!(parse_session_link("rchamber://session/../../etc").is_none());
    }
}
