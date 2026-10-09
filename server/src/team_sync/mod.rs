//! A Work workspace's connection to its team's own Convex project (`packages/team-sync/`): the
//! stored connection, invite links, one-shot calls, the live command subscription, and the deploy
//! the CLI runs for the person setting it up.

mod cloud_runner;
mod commands;
mod connections;
mod convex_http;
mod deploy;
mod invite_link;
mod operations;
mod runtime;
mod slack_flow_settings;
mod slack_post;
mod slack_request;
mod slack_requirements;
mod work_page;

pub(crate) use deploy::{deploy_team_functions, DeployOptions};
pub(crate) use invite_link::site_url;
pub(crate) use operations::*;
pub(crate) use runtime::{reload_team_sync, spawn_team_sync_task};
pub(crate) use slack_flow_settings::*;
pub(crate) use slack_post::post_slack_working_thread;
pub(crate) use work_page::*;
