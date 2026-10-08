//! Windows: the display claims and the viewer-detach handshake of a local `wmx attach` viewer,
//! sent over the attach's control pipe instead of into its ConPTY.
//!
//! CDXC:Zmx 2026-10-08 WHY:
//! ConPTY drops every OSC it does not know, so `ZMX_VISIBLE` / `ZMX_CHAT` / `ZMX_HIDDEN` and `ZMX_DETACH` written into a Windows viewer's console never reached `wmx attach`: every desktop client read `visible` in `wmx grid`, the daemon only followed the console size through a 100 ms poll, and a viewer could never be released. The viewer's spawn carries `WMX_ATTACH_CONTROL=<pipe>`; wmx creates that pipe, and the same bytes go through it. The sequences stay byte-identical to the other emitters; only their route changes.
//! SEE-ALSO: .dependencies/wmx/src/control.rs, .dependencies/wmx/src/attachment.rs, apps/desktop/src/app/terminal_sync/gpui_engine_terminal_viewers.rs.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::viewer_detach::{ViewerDetachParser, ViewerDetachState};
use super::{TerminalEvent, TerminalEventSink};

/// The environment variable `wmx attach` reads its control pipe name from.
pub(crate) const WMX_ATTACH_CONTROL_ENV: &str = "WMX_ATTACH_CONTROL";
/// How long the viewer's PowerShell may take to start `wmx attach` and its pipe.
const CONNECT_DEADLINE: Duration = Duration::from_secs(60);
const CONNECT_RETRY: Duration = Duration::from_millis(20);
/// Replies follow requests, so the pipe is read promptly for a while after traffic only.
const POLL: Duration = Duration::from_millis(5);
const POLL_AFTER_TRAFFIC: Duration = Duration::from_secs(5);
const IDLE_WAIT: Duration = Duration::from_secs(1);

/// A pipe name unique to one viewer spawn.
pub(crate) fn new_wmx_attach_control_pipe_name() -> String {
    format!(
        r"\\.\pipe\ghostex-wmx-attach-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    )
}

/// The writing half; claims sent before the pipe is up go out, in order, once it is.
pub(super) struct ZmxControl {
    tx: mpsc::Sender<Vec<u8>>,
}

impl ZmxControl {
    pub(super) fn start(
        name: String,
        viewer_detach: Arc<Mutex<ViewerDetachState>>,
        events: TerminalEventSink,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::Builder::new()
            .name("ghostex-wmx-attach-control".into())
            .spawn(move || run(name, rx, viewer_detach, events))?;
        Ok(Self { tx })
    }

    pub(super) fn send(&self, bytes: &[u8]) -> bool {
        self.tx.send(bytes.to_vec()).is_ok()
    }
}

fn run(
    name: String,
    rx: mpsc::Receiver<Vec<u8>>,
    viewer_detach: Arc<Mutex<ViewerDetachState>>,
    events: TerminalEventSink,
) {
    let mut queued: Vec<Vec<u8>> = Vec::new();
    let deadline = Instant::now() + CONNECT_DEADLINE;
    let mut pipe: File = loop {
        match OpenOptions::new().read(true).write(true).open(&name) {
            Ok(pipe) => break pipe,
            Err(_) if Instant::now() < deadline => {}
            Err(_) => return,
        }
        match rx.recv_timeout(CONNECT_RETRY) {
            Ok(bytes) => queued.push(bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    };
    for bytes in queued {
        if pipe.write_all(&bytes).is_err() {
            return;
        }
    }
    let mut parser = ViewerDetachParser::default();
    let mut buffer = [0u8; 1024];
    let mut polling_until = Instant::now() + POLL_AFTER_TRAFFIC;
    loop {
        let wait = if Instant::now() < polling_until {
            POLL
        } else {
            IDLE_WAIT
        };
        match rx.recv_timeout(wait) {
            Ok(bytes) => {
                if pipe.write_all(&bytes).and_then(|()| pipe.flush()).is_err() {
                    return;
                }
                polling_until = Instant::now() + POLL_AFTER_TRAFFIC;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        // A blocking read would hold the writes above until the attachment spoke.
        let Some(available) = peek(&pipe) else {
            return;
        };
        if available == 0 {
            continue;
        }
        let wanted = (available as usize).min(buffer.len());
        let Ok(count) = pipe.read(&mut buffer[..wanted]) else {
            return;
        };
        if count == 0 {
            return;
        }
        polling_until = Instant::now() + POLL_AFTER_TRAFFIC;
        let changed = parser.feed(
            &buffer[..count],
            &mut viewer_detach.lock().expect("terminal detach lock poisoned"),
        );
        if changed {
            events(TerminalEvent::Wakeup);
        }
    }
}

/// Bytes waiting in the pipe, or `None` once the attachment closed it.
fn peek(pipe: &File) -> Option<u32> {
    let mut available = 0u32;
    let ok = unsafe {
        windows_sys::Win32::System::Pipes::PeekNamedPipe(
            pipe.as_raw_handle() as _,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    } != 0;
    ok.then_some(available)
}
