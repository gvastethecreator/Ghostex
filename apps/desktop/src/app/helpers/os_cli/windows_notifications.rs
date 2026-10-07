//! Windows toast notifications: the attention notifications and the banked reset expiry warnings
//! the macOS build shows through UserNotifications (`GpuiSettingsNotifications.m`).
//!
//! CDXC:Notifications 2026-10-05 DECISION:
//! User: the banked reset warnings must be "a new notification for the system" on Windows too, and Ghostex had no system notifications on Windows at all. Windows gets toasts through the same delivery path the macOS banners take, so Attention Notifications and Reset Expiry Notifications both work here, and a click lands where a macOS banner click does.
//!
//! CDXC:Notifications 2026-10-05 WHY:
//! A toast needs an AppUserModelID Windows knows. A Velopack install already gives the process one (`VelopackApp::run` sets `velopack.<packId>`, the id on the Start Menu shortcut Velopack creates), so the toast shows as Ghostex with its icon; the process id is never changed here, because a different one would split the taskbar button from the pinned shortcut. The per-user local start takes the same id (`windows_updater::adopt_release_app_user_model_id`). A build without it (a `--machine` local start, a portable copy) registers `Ghostex.Desktop` under `HKCU\Software\Classes\AppUserModelId`, which Windows accepts for toasts without a shortcut. Clicks arrive through the toast's in-process `Activated` event, so no COM activator or URL scheme is needed while Ghostex runs.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use ::windows::Data::Xml::Dom::XmlDocument;
use ::windows::Foundation::TypedEventHandler;
use ::windows::UI::Notifications::{
    NotificationSetting, ToastNotification, ToastNotificationManager,
};
use ::windows::core::{HSTRING, IInspectable};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::app::helpers::*;
use crate::*;

const FALLBACK_APP_USER_MODEL_ID: &str = "Ghostex.Desktop";
/// Toasts kept alive so their `Activated` handler still runs when one is clicked from the
/// notification center later.
const KEPT_TOASTS: usize = 20;

static CLICKS: Mutex<Option<UnboundedSender<String>>> = Mutex::new(None);
static SHOWN: Mutex<VecDeque<ToastNotification>> = Mutex::new(VecDeque::new());

fn app_user_model_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        process_app_user_model_id().unwrap_or_else(|| {
            register_fallback_app_user_model_id();
            FALLBACK_APP_USER_MODEL_ID.to_string()
        })
    })
}

fn process_app_user_model_id() -> Option<String> {
    unsafe {
        let id = ::windows::Win32::UI::Shell::GetCurrentProcessExplicitAppUserModelID().ok()?;
        if id.is_null() {
            return None;
        }
        let text = id.to_string().ok();
        ::windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
        text.filter(|text| !text.trim().is_empty())
    }
}

fn register_fallback_app_user_model_id() {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let key = wide(&format!(
        "Software\\Classes\\AppUserModelId\\{FALLBACK_APP_USER_MODEL_ID}"
    ));
    let name = wide("DisplayName");
    let value = wide("Ghostex");
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        );
    }
}

fn xml_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn show_toast(
    title: &str,
    body: &str,
    click: Option<(String, &str)>,
) -> ::windows::core::Result<()> {
    let xml = format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual><audio silent="true"/></toast>"#,
        xml_text(title),
        xml_text(body),
    );
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    if let Some((target, group)) = click {
        // A newer toast for the same session or account replaces the older one in the notification center.
        let tag: String = target
            .chars()
            .rev()
            .take(64)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        toast.SetTag(&HSTRING::from(tag))?;
        toast.SetGroup(&HSTRING::from(group))?;
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, _| {
                if let Some(clicks) = CLICKS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    let _ = clicks.unbounded_send(target.clone());
                }
                Ok(())
            },
        ))?;
    }
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_user_model_id()))?
        .Show(&toast)?;
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    shown.push_back(toast);
    while shown.len() > KEPT_TOASTS {
        shown.pop_front();
    }
    Ok(())
}

/// Shows one attention or reset-warning toast; a click opens what the macOS banner click opens.
pub(crate) fn gpui_deliver_windows_notification(
    candidate: GpuiSessionAttentionNotificationCandidate,
) -> GpuiMacOSNotificationDeliveryResult {
    let group = if candidate
        .session_id
        .starts_with(crate::notification_feed::reset_banners::ACCOUNT_RESET_BANNER_PREFIX)
    {
        "accountReset"
    } else {
        "attention"
    };
    match show_toast(
        &candidate.title,
        &candidate.body,
        Some((candidate.session_id.clone(), group)),
    ) {
        Ok(()) => GpuiMacOSNotificationDeliveryResult::Sent,
        Err(_) => GpuiMacOSNotificationDeliveryResult::Failed,
    }
}

/// The Settings test: one generic toast with no click target, like the macOS test banner.
pub(crate) fn gpui_deliver_windows_test_notification() -> GpuiMacOSNotificationDeliveryResult {
    match gpui_windows_notification_status() {
        GpuiMacOSNotificationAuthorizationStatus::Authorized => {}
        GpuiMacOSNotificationAuthorizationStatus::Denied => {
            return GpuiMacOSNotificationDeliveryResult::PermissionDenied;
        }
        _ => return GpuiMacOSNotificationDeliveryResult::Unknown,
    }
    match show_toast(
        "Agent task complete",
        "This is a Ghostex notification test.",
        None,
    ) {
        Ok(()) => GpuiMacOSNotificationDeliveryResult::Sent,
        Err(_) => GpuiMacOSNotificationDeliveryResult::Failed,
    }
}

/// Whether Windows lets Ghostex show toasts (Settings > System > Notifications).
pub(crate) fn gpui_windows_notification_status() -> GpuiMacOSNotificationAuthorizationStatus {
    let setting =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_user_model_id()))
            .and_then(|notifier| notifier.Setting());
    match setting {
        Ok(NotificationSetting::Enabled) => GpuiMacOSNotificationAuthorizationStatus::Authorized,
        Ok(_) => GpuiMacOSNotificationAuthorizationStatus::Denied,
        Err(_) => GpuiMacOSNotificationAuthorizationStatus::Unknown,
    }
}

/// Clicks reach the lead window through this channel: the `Activated` event runs on a Windows
/// thread, and the click is handled on the app's own.
pub(crate) fn register_gpui_windows_notification_click_target(
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    let (sender, mut receiver): (UnboundedSender<String>, UnboundedReceiver<String>) = unbounded();
    *CLICKS.lock().unwrap_or_else(|e| e.into_inner()) = Some(sender);
    cx.spawn(async move |this, cx| {
        use futures::StreamExt as _;
        while let Some(target) = receiver.next().await {
            if this
                .update_in(cx, |this, window, cx| {
                    this.activate_system_notification(target, window, cx);
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

pub(crate) fn unregister_gpui_windows_notification_click_target() {
    *CLICKS.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
