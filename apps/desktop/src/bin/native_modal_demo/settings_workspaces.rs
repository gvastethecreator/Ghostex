//! Preview fixtures for the Workspaces Settings page with a Work workspace's team rows, Team flow
//! and team-flow steps (mockups 09 and 10).
//!
//! States: `workspaces-team` (the Work workspace connected as the team's owner, Slack and the
//! team's Linear key set), `workspaces-team-flow` (the same, opened at the Team flow section),
//! `workspaces-join` (the Work workspace not connected yet).
use serde_json::{Value, json};

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("workspaces-")
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    owns(state).then(|| match state {
        "workspaces-team-flow" => {
            json!({ "initialTab": "workspaces", "initialSection": "workspace-work-team-flow" })
        }
        _ => json!({ "initialTab": "workspaces" }),
    })
}

fn workspaces() -> Value {
    json!({
        "sidebarWorkspaces": {
            "order": ["work", "personal"],
            "workspaces": {
                "work": { "workspaceId": "work", "name": "Work", "letter": "W", "color": "#596fd1", "kind": "work", "claudeAccountId": null },
                "personal": { "workspaceId": "personal", "name": "Personal", "letter": "P", "color": "#3aa675", "kind": "personal", "claudeAccountId": null }
            }
        }
    })
}

fn team_status(state: &str) -> Value {
    if state == "workspaces-join" {
        return json!({ "connections": [] });
    }
    json!({
        "connections": [{
            "workspaceId": "work",
            "deploymentUrl": "https://happy-otter-123.convex.cloud",
            "siteUrl": "https://happy-otter-123.convex.site",
            "teamName": "ShortPoint",
            "memberName": "Yahia",
            "subscription": "live",
            "team": {
                "team": { "id": "t1", "name": "ShortPoint", "slackTeamId": "T04SHORTPT", "functionsVersion": 2 },
                "deployedFunctionsVersion": 2,
                "secrets": { "slackBotToken": true, "slackSigningSecret": true, "linearApiKey": false },
                "me": { "id": "m1", "name": "Yahia", "role": "owner", "slackUserId": "U012ABCDEF", "linearUserId": null },
                "members": [{}, {}, {}, {}, {}, {}]
            }
        }]
    })
}

fn slack_flow() -> Value {
    json!({
        "workingChannelId": "C07KEVINBOT",
        "watchOnlyChannelIds": ["C05SPRINTVAL"],
        "channelRepos": [
            { "channelId": "C01BUGS0001", "repo": "shortpoint/shortpoint", "project": null, "linearTeamKey": "SPX" },
            { "channelId": "C02WEBSITE1", "repo": "shortpoint/shortpoint-website", "project": null, "linearTeamKey": "SPX" },
            { "channelId": "C03DEV00001", "repo": null, "project": null, "linearTeamKey": null }
        ],
        "linearTeamKey": "SPX",
        "defaultRunPlace": "cloud",
        "qcOwnerSlackUserId": null,
        "instructions": "## Communication\n- During long tasks, post a short update as soon as each milestone lands.\n- Number report steps 1a, 1b, 2a…\n\n## Asking questions\n- Number the questions and letter the options; option A is always the recommended one.",
        "updatedAt": 1760000000000_i64,
        "canEdit": true
    })
}

fn team_flow_steps() -> Value {
    json!({
        "source": "builtIn",
        "steps": [
            { "id": "ticket", "label": "Ticket", "rule": { "kind": "ticketExists" } },
            { "id": "working-thread", "label": "Working thread", "rule": { "kind": "slackWorkingThread" } },
            { "id": "session", "label": "Session", "rule": { "kind": "sessionLinked" } },
            { "id": "pr", "label": "PR", "rule": { "kind": "pullRequestExists" } },
            { "id": "review-comments", "label": "Review comments", "rule": { "kind": "reviewCommentsResolved" } },
            { "id": "ci", "label": "CI", "rule": { "kind": "checksPassing" } },
            { "id": "video", "label": "Video", "rule": { "kind": "videoApproved" } },
            { "id": "qc-package", "label": "QC package", "rule": { "kind": "pullRequestLabel", "label": "READY-FOR-QC" } },
            { "id": "validation", "label": "Validation", "rule": { "kind": "slackValidationPost" } }
        ],
        "rules": [
            { "kind": "ticketExists", "description": "A Linear or GitHub ticket exists", "needsData": false },
            { "kind": "slackWorkingThread", "description": "A Slack working thread exists", "needsData": true },
            { "kind": "sessionLinked", "description": "A session is linked to the ticket", "needsData": false },
            { "kind": "pullRequestExists", "description": "A pull request exists", "needsData": false },
            { "kind": "reviewCommentsResolved", "description": "Every review comment is resolved", "needsData": false },
            { "kind": "pullRequestApproved", "description": "The pull request is approved", "needsData": false },
            { "kind": "checksPassing", "description": "Every check passes", "needsData": false },
            { "kind": "pullRequestLabel", "description": "The pull request has a label", "needsData": false },
            { "kind": "pullRequestMerged", "description": "The pull request is merged", "needsData": false },
            { "kind": "ticketState", "description": "The ticket reached a state", "needsData": false },
            { "kind": "videoApproved", "description": "The demo video is approved", "needsData": true },
            { "kind": "slackValidationPost", "description": "The validation request is posted in Slack", "needsData": true }
        ]
    })
}

/// The scripted daemon for these states; `None` for paths it does not know.
pub(super) fn rpc(state: &str, path: &str, params: &Value) -> Option<Result<Value, String>> {
    Some(Ok(match path {
        "/api/readWorkspaces" => workspaces(),
        "/api/readWorkModeStatus" => {
            json!({ "linearKeys": { "shared": true, "workspaces": ["work"] } })
        }
        "/api/agentAccounts" => json!({ "accounts": [] }),
        "/api/readTeamSyncStatus" => team_status(state),
        "/api/readSlackFlowSettings" | "/api/setSlackFlowSettings" => slack_flow(),
        "/api/readTeamFlow" | "/api/updateTeamFlow" => team_flow_steps(),
        "/api/joinTeamSync" => {
            return Some(Err(format!(
                "This invite link was already used: {}",
                params
                    .get("inviteLink")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )));
        }
        _ => return None,
    }))
}
