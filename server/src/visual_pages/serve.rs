use std::path::PathBuf;

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};

use crate::server::RoutedResponse;

use super::{prepare::PAGE_CONTENT_SECURITY_POLICY, store::read_page};

pub(crate) const VISUAL_PAGE_ROUTE_PREFIX: &str = "/visual/";

/// `GET /visual/<id>`: a page `ghostex show` published, answered before the auth gate.
///
/// CDXC:ServerApi 2026-10-06 WHY:
/// The browser page that opens a chat card has no gxserver auth token, so the unguessable 128-bit id in the path is the only grant, as the `/open-file/` tokens are. The `sandbox` policy in the response header gives the page an opaque origin: its scripts run, but they cannot read gxserver's cookies or storage or call it as a same-origin page, and the content policy beside it keeps every fetch on https:.
///
/// CDXC:SessionChat 2026-10-06 DECISION:
/// User: no JavaScript engine is added to the desktop. An HTML page runs only in a browser page (Ghostex's browser, the system browser, or the phone's WebView) that opens this URL, never inline in the GPUI chat.
pub(crate) async fn serve(gxserver_state_dir: PathBuf, path: String) -> RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .map(str::to_string)
    else {
        return status(StatusCode::NOT_FOUND);
    };
    let page = tokio::task::spawn_blocking(move || read_page(&gxserver_state_dir, &id))
        .await
        .ok()
        .flatten();
    let Some(page) = page else {
        return status(StatusCode::NOT_FOUND);
    };
    let mut response = RoutedResponse {
        endpoint_path: None,
        response: Response::new(Body::from(page)),
    };
    let content_security_policy =
        format!("sandbox allow-scripts allow-forms allow-popups; {PAGE_CONTENT_SECURITY_POLICY}");
    for (name, value) in [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (
            header::CONTENT_SECURITY_POLICY,
            content_security_policy.as_str(),
        ),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CACHE_CONTROL, "no-store"),
    ] {
        if let Ok(value) = HeaderValue::from_str(value) {
            response.response.headers_mut().insert(name, value);
        }
    }
    response
}

fn status(code: StatusCode) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: code.into_response(),
    }
}
