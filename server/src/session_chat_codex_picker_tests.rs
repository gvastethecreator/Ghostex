use super::*;

#[test]
fn ultra_composer_is_ready_but_advanced_picker_is_not() {
    use crate::session_chat_composer::{
        detect_session_chat_composer_ready, SessionChatComposerState,
    };
    assert_eq!(
        detect_session_chat_composer_ready(
            Some("codex"),
            "» Ask Codex to do anything\n  gpt-6-astra ultra"
        )
        .state,
        SessionChatComposerState::Ready
    );
    let advanced = "Advanced Reasoning\n⚠ Consumes usage limits faster\n› 1. Max  Higher usage\n  2. Ultra  Highest usage\nPress enter to confirm or esc to go back";
    assert!(any_picker_open(advanced));
    assert_eq!(advanced_effort_picker_rows(advanced).unwrap().len(), 2);
    assert_eq!(
        detect_session_chat_composer_ready(Some("codex"), advanced).state,
        SessionChatComposerState::NotReady
    );
}

/// Run this test binary from the packaged Web directory so bundled zmx resolves.
/// Set `GHOSTEX_CODEX_PICKER_TEST_ZMX=ghostex-effort-check-...` to a dedicated idle Codex TUI and pass `live_zmx_effort_round_trip --ignored --nocapture`.
#[tokio::test]
#[ignore = "requires an explicitly supplied disposable Codex zmx session"]
async fn live_zmx_effort_round_trip() {
    let name = std::env::var("GHOSTEX_CODEX_PICKER_TEST_ZMX")
        .expect("set the dedicated test session name");
    assert!(name.starts_with("ghostex-effort-check-"));
    let screen = capture_session_terminal_text(&name).await.unwrap();
    let initial = detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen).unwrap();
    let model = initial.model.unwrap().value;
    let original_effort = initial.effort.unwrap().value;
    let warning = "Max and Ultra are available under";
    let warning_count = screen.matches(warning).count();
    let mut failure = None;
    for effort in [
        "high",
        "xhigh",
        "max",
        "ultra",
        "max",
        "low",
        "ultra",
        "high",
        original_effort.as_str(),
    ] {
        let started = std::time::Instant::now();
        let job_id = register_job(CodexPickerPlan {
            model: model.clone(),
            effort: effort.to_string(),
            ..Default::default()
        });
        let send = execute_session_chat_send(
            "effort-check",
            &name,
            &name,
            "effort-check",
            vec![SessionChatSendStep::DriveCodexModelPicker { job_id }],
        )
        .await;
        let result = take_job_outcome(job_id).expect("picker worker outcome");
        if let Err(error) = result {
            failure = Some(error.message);
            break;
        }
        assert!(send.is_ok());
        let screen = capture_session_terminal_text(&name).await.unwrap();
        let selection =
            detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen).unwrap();
        assert_eq!(selection.model.unwrap().value, model);
        assert_eq!(selection.effort.unwrap().value, effort);
        assert_eq!(
            screen.matches(warning).count(),
            warning_count,
            "shifted arrows must not attempt Max/Ultra"
        );
        assert!(!any_picker_open(&screen));
        println!(
            "{model} {effort}: {}ms, footer confirmed and picker closed",
            started.elapsed().as_millis()
        );
    }
    if let Some(error) = failure {
        panic!(
            "{error}\n{}",
            capture_session_terminal_text(&name)
                .await
                .unwrap_or_default()
        );
    }
}
