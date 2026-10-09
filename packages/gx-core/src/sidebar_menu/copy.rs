//! A session menu's Copy submenu: Copy Details, and every link or name of the session's work.

use crate::sidebar_view::view::SessionRow;

use super::clipboard::session_details_text;
use super::commands::{message, MenuCommand};
use super::group::MenuGroup;
use super::item::MenuItem;

/// The Copy submenu of one row. Copy Details is always there; the other rows only when the session
/// has something for them (a branch from the git probe or work mode, the work-mode links).
///
/// CDXC:ContextMenus 2026-10-09 DECISION:
/// User: Copy Details moves into a Copy submenu next to Copy Branch, Copy Linear ID, Copy PR Link and the like, for normal and work-mode sessions; it stays always available so anyone can hand a session to another agent. Supersedes the 2026-09-26 decision that kept Copy Details under Advanced.
pub(crate) fn copy_submenu(id: &str, row: &SessionRow, group: &MenuGroup<'_>) -> MenuItem {
    let mut children = vec![MenuItem::row(
        "Copy Details",
        "copy",
        MenuCommand::command(message::copy_session_details(
            id,
            &session_details_text(row, &group.details()),
        )),
    )];
    let mut copy_row = |label: &str, icon: &str, text: Option<String>| {
        if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
            children.push(MenuItem::row(
                label,
                icon,
                MenuCommand::command(message::copy_text(&text)),
            ));
        }
    };
    copy_row("Copy Branch", "git-branch", row.menu_facts.branch.clone());
    let work = row.work.as_ref();
    let linear_issues = work
        .map(|work| work.linear_issues.as_slice())
        .unwrap_or(&[]);
    // Several issues shipped in one PR copy as one comma-joined line, which is what a PR title or
    // a Linear search takes.
    copy_row(
        if linear_issues.len() > 1 {
            "Copy Linear IDs"
        } else {
            "Copy Linear ID"
        },
        "hash",
        (!linear_issues.is_empty()).then(|| {
            linear_issues
                .iter()
                .map(|issue| issue.identifier.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }),
    );
    copy_row(
        "Copy Linear Link",
        "link",
        joined_lines(
            linear_issues
                .iter()
                .filter_map(|issue| issue.url.as_deref()),
        ),
    );
    copy_row(
        "Copy PR Link",
        "git-pull-request",
        work.and_then(|work| work.pull_request.as_ref())
            .and_then(|pr| pr.url.clone()),
    );
    copy_row(
        "Copy Issue Link",
        "circle-dot",
        joined_lines(
            work.map(|work| work.github_issues.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter_map(|issue| issue.url.as_deref()),
        ),
    );
    copy_row(
        "Copy Linear Project Link",
        "box",
        work.and_then(|work| work.linear_project.as_ref())
            .and_then(|project| project.url.clone()),
    );
    MenuItem::submenu("Copy", "copy", children)
}

fn joined_lines<'a>(values: impl Iterator<Item = &'a str>) -> Option<String> {
    let values: Vec<&str> = values.filter(|value| !value.trim().is_empty()).collect();
    (!values.is_empty()).then(|| values.join("\n"))
}
