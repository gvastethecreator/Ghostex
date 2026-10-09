//! Invite links: `https://<name>.convex.site/join?code=gxi_…`, which carry the team's deployment and
//! a one-time code. A deployment outside `*.convex.cloud` (self-hosted) adds `&deployment=<url>`.

use url::Url;

/// The deployment URL as stored: `https://host[/path]` with no trailing slash.
pub(crate) fn normalize_deployment_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = Url::parse(trimmed)
        .map_err(|_| format!("\"{trimmed}\" is not a Convex deployment URL."))?;
    if url.scheme() != "https" && url.host_str() != Some("127.0.0.1") {
        return Err("The Convex deployment URL must start with https://.".to_string());
    }
    if url
        .host_str()
        .is_some_and(|host| host.ends_with(".convex.site"))
    {
        return Err(
            "That is the HTTP actions URL (.convex.site); pass the deployment URL (.convex.cloud)."
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

/// Where the deployment serves HTTP actions (Slack and Linear webhooks, `/join`).
pub(crate) fn site_url(deployment_url: &str) -> Option<String> {
    let url = Url::parse(deployment_url).ok()?;
    let host = url.host_str()?;
    let name = host.strip_suffix(".convex.cloud")?;
    Some(format!("https://{name}.convex.site"))
}

pub(crate) fn build_invite_link(deployment_url: &str, code: &str) -> String {
    match site_url(deployment_url) {
        Some(site) => format!("{site}/join?code={code}"),
        None => {
            let mut url = Url::parse(&format!("{deployment_url}/join")).unwrap_or_else(|_| {
                Url::parse("https://invalid.invalid/join").expect("static URL parses")
            });
            url.query_pairs_mut()
                .append_pair("code", code)
                .append_pair("deployment", deployment_url);
            url.to_string()
        }
    }
}

/// The deployment URL and code an invite link carries.
pub(crate) fn parse_invite_link(link: &str) -> Result<(String, String), String> {
    let invalid = || {
        "That is not a Ghostex invite link. Ask a teammate to copy it again from Settings → Workspaces."
            .to_string()
    };
    let url = Url::parse(link.trim()).map_err(|_| invalid())?;
    let mut code = None;
    let mut deployment = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.trim().to_string()),
            "deployment" => deployment = Some(value.trim().to_string()),
            _ => {}
        }
    }
    let code = code
        .filter(|code| code.starts_with("gxi_"))
        .ok_or_else(invalid)?;
    let deployment = match deployment {
        Some(deployment) => deployment,
        None => {
            let host = url.host_str().ok_or_else(invalid)?;
            let name = host.strip_suffix(".convex.site").ok_or_else(invalid)?;
            format!("https://{name}.convex.cloud")
        }
    };
    Ok((normalize_deployment_url(&deployment)?, code))
}
