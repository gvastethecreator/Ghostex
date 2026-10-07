use std::{
    fs,
    path::{Path, PathBuf},
};

use rand::RngCore;
use serde_json::json;

/// The largest prepared page gxserver stores, local images included.
pub(crate) const VISUAL_PAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Titles are card labels; anything longer is cut.
pub(super) const VISUAL_PAGE_TITLE_MAX_CHARS: usize = 200;
const VISUAL_PAGE_SESSION_REF_MAX_CHARS: usize = 200;
const VISUAL_PAGES_DIR_NAME: &str = "visual-pages";

pub(super) fn visual_pages_dir(gxserver_state_dir: &Path) -> PathBuf {
    gxserver_state_dir.join(VISUAL_PAGES_DIR_NAME)
}

/// 128 random bits from the operating system's generator, as 32 lowercase hex characters.
fn new_page_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn is_page_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn clip_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Writes `<id>.html` and its `<id>.json` record and returns the new id.
pub(super) fn write_page(
    gxserver_state_dir: &Path,
    title: &str,
    html: &str,
    session_ref: Option<&str>,
) -> std::io::Result<String> {
    let dir = visual_pages_dir(gxserver_state_dir);
    fs::create_dir_all(&dir)?;
    let id = new_page_id();
    let record = json!({
        "title": title,
        "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "sessionRef": session_ref
            .map(|value| clip_chars(value, VISUAL_PAGE_SESSION_REF_MAX_CHARS)),
    });
    fs::write(dir.join(format!("{id}.html")), html)?;
    fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_vec_pretty(&record).unwrap_or_default(),
    )?;
    Ok(id)
}

pub(super) fn read_page(gxserver_state_dir: &Path, id: &str) -> Option<Vec<u8>> {
    if !is_page_id(id) {
        return None;
    }
    fs::read(visual_pages_dir(gxserver_state_dir).join(format!("{id}.html"))).ok()
}
