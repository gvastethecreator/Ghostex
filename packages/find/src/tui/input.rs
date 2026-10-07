use std::io::{Read, Write};

use crate::agent::Agent;

use super::*;

impl<'a> Tui<'a> {
    // -----------------------------------------------------------------------
    // input
    // -----------------------------------------------------------------------

    /// Returns the chosen Action, or None if cancelled.
    pub fn run(&mut self) -> Result<Option<Action>, String> {
        let stdin = std::io::stdin();
        self.refresh_winsize(&stdin);
        term::install_signal_handlers();
        term::enter_raw()?;
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(ENTER_TUI_SEQUENCE.as_bytes());
        let _ = stdout.flush();

        let outcome = self.event_loop(&stdin, &mut stdout);

        let _ = stdout.write_all(LEAVE_TUI_SEQUENCE.as_bytes());
        let _ = stdout.flush();
        term::leave_raw();
        outcome
    }

    fn event_loop(
        &mut self,
        stdin: &std::io::Stdin,
        stdout: &mut std::io::Stdout,
    ) -> Result<Option<Action>, String> {
        self.recompute();
        let mut handle = stdin.lock();
        let mut ibuf = [0u8; 256];
        loop {
            self.render(stdout, stdin).map_err(|e| e.to_string())?;
            let n = match handle.read(&mut ibuf) {
                Ok(0) | Err(_) => return Ok(None),
                Ok(n) => n,
            };
            let mut i = 0usize;
            while i < n {
                let c = ibuf[i];

                if self.filtering_project {
                    if c == 27 {
                        if let Some(consumed) = self.handle_escape_sequence(&ibuf[i..n]) {
                            i += consumed;
                            continue;
                        }
                        self.filtering_project = false;
                        i += 1;
                        continue;
                    }
                    match c {
                        13 => {
                            self.apply_project_filter_selection();
                            self.filtering_project = false;
                        }
                        b' ' => self.toggle_project_filter_selection(),
                        127 | 8 => self.backspace_project_query(),
                        14 => self.move_project_selection(1),
                        16 => self.move_project_selection(-1),
                        _ => {
                            if (32..127).contains(&c) || c >= 128 {
                                self.insert_project_query_byte(c);
                            }
                        }
                    }
                    i += 1;
                    continue;
                }

                // While picking an agent filter, digits choose the filter and any
                // other key (esc included) cancels back to normal browsing.
                if self.filtering_agent {
                    if c == 27 {
                        if let Some(consumed) = self.handle_escape_sequence(&ibuf[i..n]) {
                            i += consumed;
                            continue;
                        }
                        self.filtering_agent = false;
                        i += 1;
                        continue;
                    }
                    match c {
                        13 | b' ' => self.toggle_filter_selection(),
                        b'1'..=b'7' => {
                            self.filter_sel = (c - b'1') as usize;
                            self.toggle_filter_selection();
                        }
                        14 => self.move_filter_selection(1),
                        16 => self.move_filter_selection(-1),
                        _ => {}
                    }
                    i += 1;
                    continue;
                }

                // While picking a fork target, digits choose the agent and any
                // other key (esc included) cancels back to normal browsing.
                if self.forking {
                    self.forking = false;
                    if self.sel < self.hits.len() {
                        let agent = match c {
                            b'1' => Some(Agent::Claude),
                            b'2' => Some(Agent::Codex),
                            b'3' => Some(Agent::Pi),
                            b'4' => Some(Agent::Opencode),
                            b'5' => Some(Agent::Cursor),
                            b'6' => Some(Agent::Grok),
                            _ => None,
                        };
                        if let Some(fork_agent) = agent {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::Fork,
                                fork_agent,
                            }));
                        }
                    }
                    i += 1;
                    continue;
                }

                if c == 27 {
                    if let Some(consumed) = self.handle_escape_sequence(&ibuf[i..n]) {
                        i += consumed;
                        continue;
                    }
                    return Ok(None); // bare ESC quits
                }
                match c {
                    3 => return Ok(None),            // ctrl-c
                    4 => self.toggle_day_grouping(), // ctrl-d
                    13 => {
                        // Enter. CR only: ^j (byte 10) is the project picker, and
                        // raw mode clears ICRNL so Enter always arrives as CR.
                        if self.sel < self.hits.len() {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::ResumeSession,
                                fork_agent: Agent::Claude,
                            }));
                        }
                        return Ok(None);
                    }
                    6 => {
                        // ctrl-f
                        if self.preview_focus {
                            self.fullscreen_preview = !self.fullscreen_preview;
                        } else {
                            self.toggle_favorite();
                        }
                    }
                    5 => {
                        // ctrl-e: open selected prompt in $EDITOR
                        if self.sel < self.hits.len() {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::View,
                                fork_agent: Agent::Claude,
                            }));
                        }
                    }
                    25 => {
                        // ctrl-y: copy selected to clipboard
                        if self.sel < self.hits.len() {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::Copy,
                                fork_agent: Agent::Claude,
                            }));
                        }
                    }
                    9 => self.preview_focus = !self.preview_focus, // tab
                    15 => {
                        // ctrl-o: fork into another agent
                        if self.sel < self.hits.len() {
                            self.forking = true;
                        }
                    }
                    11 => self.kill_to_end(),                // ctrl-k
                    10 => self.open_project_filter_picker(), // ctrl-j
                    7 => self.open_agent_filter_picker(),    // ctrl-g
                    21 => self.kill_to_beginning(),          // ctrl-u
                    b'w' | b'W' => {
                        if self.preview_focus {
                            self.wrap_preview = !self.wrap_preview;
                        } else {
                            self.insert_query_byte(c);
                        }
                    }
                    b'f' | b'F' => {
                        if self.preview_focus {
                            self.fullscreen_preview = !self.fullscreen_preview;
                        } else {
                            self.insert_query_byte(c);
                        }
                    }
                    127 | 8 => self.backspace(),
                    14 => self.move_down(), // ctrl-n
                    16 => self.move_up(),   // ctrl-p
                    _ => {
                        // accept printable ASCII and any UTF-8 lead/continuation
                        // byte (>=128), but not DEL
                        if (32..127).contains(&c) || c >= 128 {
                            self.insert_query_byte(c);
                        }
                    }
                }
                i += 1;
            }
        }
    }

    fn handle_escape_sequence(&mut self, bytes: &[u8]) -> Option<usize> {
        if bytes.len() < 3 || bytes[0] != 27 || bytes[1] != b'[' {
            return None;
        }
        if bytes[2] == b'<' {
            if let Some((event, consumed)) = parse_sgr_mouse(bytes) {
                self.handle_mouse_event(event);
                return Some(consumed);
            }
            return Some(bytes.len());
        }
        if bytes[2] == b'5' && bytes.len() >= 4 && bytes[3] == b'~' {
            if self.preview_focus {
                self.scroll_preview(-1);
            } else if self.group_by_day {
                self.jump_day(-1);
            }
            return Some(4);
        }
        if bytes[2] == b'6' && bytes.len() >= 4 && bytes[3] == b'~' {
            if self.preview_focus {
                self.scroll_preview(1);
            } else if self.group_by_day {
                self.jump_day(1);
            }
            return Some(4);
        }
        if bytes[2] == b'3'
            && bytes.len() >= 6
            && bytes[3] == b';'
            && bytes[4] == b'5'
            && bytes[5] == b'~'
        {
            self.delete_word_forward(); // ctrl-delete
            return Some(6);
        }
        if bytes.len() >= 8 && &bytes[2..8] == b"127;5u" {
            self.delete_word_backward(); // ctrl-backspace (CSI u)
            return Some(8);
        }
        if bytes.len() >= 6 && &bytes[2..6] == b"8;5u" {
            self.delete_word_backward(); // ctrl-backspace (CSI u)
            return Some(6);
        }
        if bytes.len() >= 6 && bytes[2] == b'1' && bytes[3] == b';' && bytes[4] == b'5' {
            match bytes[5] {
                b'A' => {
                    if self.filtering_project {
                        self.move_project_selection(-1);
                    } else if self.filtering_agent {
                        self.move_filter_selection(-1);
                    } else if self.group_by_day {
                        self.jump_day(-1);
                    } else {
                        self.move_up();
                    }
                }
                b'B' => {
                    if self.filtering_project {
                        self.move_project_selection(1);
                    } else if self.filtering_agent {
                        self.move_filter_selection(1);
                    } else if self.group_by_day {
                        self.jump_day(1);
                    } else {
                        self.move_down();
                    }
                }
                b'C' => {
                    if self.preview_focus {
                        self.scroll_result_to_end();
                    } else {
                        self.move_word_right();
                    }
                }
                b'D' => {
                    if self.preview_focus {
                        self.result_scroll = 0;
                    } else {
                        self.move_word_left();
                    }
                }
                _ => {}
            }
            return Some(6);
        }
        match bytes[2] {
            b'A' => {
                if self.filtering_project {
                    self.move_project_selection(-1);
                } else if self.filtering_agent {
                    self.move_filter_selection(-1);
                } else {
                    self.move_up();
                }
            }
            b'B' => {
                if self.filtering_project {
                    self.move_project_selection(1);
                } else if self.filtering_agent {
                    self.move_filter_selection(1);
                } else {
                    self.move_down();
                }
            }
            b'C' => {
                if self.preview_focus {
                    self.scroll_result(8);
                } else {
                    self.move_right();
                }
            }
            b'D' => {
                if self.preview_focus {
                    self.scroll_result(-8);
                } else {
                    self.move_left();
                }
            }
            _ => {}
        }
        Some(3)
    }

    fn handle_mouse_event(&mut self, ev: MouseEvent) {
        if self.filtering_agent || self.filtering_project || self.forking {
            return;
        }
        // Wheel input moves the keyboard selection; clicks never select or resume.
        if (ev.button & 64) != 0 {
            match ev.button & 3 {
                0 => self.move_up(),
                1 => self.move_down(),
                _ => {}
            }
        }
    }
}

pub(super) fn parse_sgr_mouse(bytes: &[u8]) -> Option<(MouseEvent, usize)> {
    if bytes.len() < 6 || bytes[0] != 27 || bytes[1] != b'[' || bytes[2] != b'<' {
        return None;
    }
    let mut i = 3usize;
    let button = parse_mouse_number(bytes, &mut i)?;
    if i >= bytes.len() || bytes[i] != b';' {
        return None;
    }
    i += 1;
    let x = parse_mouse_number(bytes, &mut i)?;
    if i >= bytes.len() || bytes[i] != b';' {
        return None;
    }
    i += 1;
    let y = parse_mouse_number(bytes, &mut i)?;
    if i >= bytes.len() || (bytes[i] != b'M' && bytes[i] != b'm') {
        return None;
    }
    Some((
        MouseEvent {
            button,
            _x: x,
            _y: y,
        },
        i + 1,
    ))
}

fn parse_mouse_number(bytes: &[u8], index: &mut usize) -> Option<usize> {
    if *index >= bytes.len() || !bytes[*index].is_ascii_digit() {
        return None;
    }
    let mut value = 0usize;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        value = value * 10 + (bytes[*index] - b'0') as usize;
        *index += 1;
    }
    Some(value)
}
