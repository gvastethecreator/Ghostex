//! The gxserver HTTP router: the gate and dispatch (`dispatch.rs`) plus one sibling module per route area, each with its own `route_*_http` match.

use super::*;

mod agents;
mod chat;
mod control;
mod dispatch;
mod git;
mod projects;
mod prompts;
mod sessions;
mod sidebar;
mod work_mode;

use agents::route_agents_http;
use chat::route_chat_http;
use control::route_control_http;
pub(in crate::server) use dispatch::route_http;
use dispatch::RouteHttpRequest;
use git::route_git_http;
use projects::route_projects_http;
use prompts::route_prompts_http;
use sessions::route_sessions_http;
use sidebar::route_sidebar_http;
use work_mode::route_work_mode_http;
