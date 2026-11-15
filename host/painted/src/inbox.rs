//! What the reader's other threads tell exact2 while its own is held
//! (LLP 1115.001 D5). `Native.answer`, `changed`, `activate`, `journal`,
//! `presence`, `orientationApplied`, `picture` and `pictureChanged` push
//! here from the thread the news is on, and the next turn applies it
//! ([`drain`], run by `Presenter::pump`, which every wait and every
//! reader frame takes). Pushed, a reply lands inside the op that holds
//! the thread; posted to exact2's handler it could only run after it —
//! an agent's `clock` used to wait out its bound on a module answer
//! queued behind it.

use crate::presenter::Presenter;
use exact_runner::DataSource;
use serde_json::Value as Json;
use std::collections::VecDeque;
#[cfg(not(test))]
use std::sync::Mutex;

/// One thing the reader's side said: enough of its call to run it on the
/// host's thread, where the host lives.
pub enum In {
    /// `Native.answer`: the answer to a request, or its cancellation.
    Answer {
        /// The request's number.
        id: u32,
        /// Its answer's JSON, `None` its cancellation.
        answer: Option<String>,
    },
    /// `Native.changed`: an extension announced a topic; its readers are
    /// asked again.
    Changed(String),
    /// `Native.activate`: a person's act on what an extension posted on the
    /// platform's own UI.
    Activated {
        /// The extension's name.
        extension: String,
        /// The event's name.
        event: String,
        /// The event's arguments, a JSON array.
        args: String,
        /// The declared action's record a notification's action carried.
        action: Option<String>,
    },
    /// `Native.journal`: a line for the session's journal.
    Journal(String),
    /// `Native.presence`: how a presence check the reader made ended.
    Presence(String),
    /// `Native.orientationApplied`: what the activity asked for the
    /// session's orientation lock.
    Orientation {
        /// What was applied, as JSON.
        applied: String,
        /// Whether it fought the app's request.
        conflict: bool,
        /// A line its resolution asks journaled, or none.
        line: Option<String>,
    },
    /// `Native.picture`: a package's picture for a request, or the
    /// implementation's error.
    Picture {
        /// The request's number.
        id: u32,
        /// The decoded pixels, or the error.
        answer: Result<tiny_skia::Pixmap, String>,
    },
    /// `Native.pictureChanged`: a picture's source changed or was revoked.
    PictureChanged {
        /// The picture's source.
        source: String,
        /// Whether it was revoked rather than changed.
        revoked: bool,
    },
}

/// The calls, in the order the readers' threads made them.
#[cfg(not(test))]
static INBOX: Mutex<VecDeque<In>> = Mutex::new(VecDeque::new());

/// A test's calls are its own thread's: a presenter's pump does not take
/// what another test pushed.
#[cfg(test)]
thread_local! {
    static INBOX: std::cell::RefCell<VecDeque<In>> =
        const { std::cell::RefCell::new(VecDeque::new()) };
}

/// Keep `what` for the next drain. Any thread; the caller that wants a turn
/// soon asks the reader for one (`Exact.wake`, an empty task on exact2's
/// handler, which turns when the thread is free).
pub fn push(what: In) {
    #[cfg(not(test))]
    INBOX
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back(what);
    #[cfg(test)]
    INBOX.with(|i| i.borrow_mut().push_back(what));
}

/// Apply what was pushed since the last drain, in the order it was pushed:
/// whether the host should turn again for it (`CanvasHost::force`).
pub fn drain<D: DataSource>(p: &mut Presenter<D>) -> bool {
    #[cfg(not(test))]
    let queued: Vec<In> = INBOX
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect();
    #[cfg(test)]
    let queued: Vec<In> = INBOX.with(|i| i.borrow_mut().drain(..).collect());
    let mut again = false;
    for what in queued {
        again |= what.apply(p);
    }
    again
}

impl In {
    /// The call, run on `p` as the turn that once took it ran it.
    fn apply<D: DataSource>(self, p: &mut Presenter<D>) -> bool {
        match self {
            In::Answer { id, answer } => p.answer_request(id, answer.as_deref()),
            In::Changed(topic) => {
                p.host().runner().native_slot().changed(&topic);
                false
            }
            In::Activated {
                extension,
                event,
                args,
                action,
            } => {
                let runner = p.host_mut().runner_mut();
                if let Some(action) = action {
                    let record = serde_json::from_str(&action).unwrap_or(Json::Null);
                    if !runner.declares_action(&extension, &record) {
                        let name = record["action"].as_str().unwrap_or("");
                        runner.log(format!(
                            "{extension}: the action \"{name}\" is not one this version of the app declares, and raised nothing"
                        ));
                        return true;
                    }
                }
                if let Err(e) = runner.raise_module_event(
                    &extension,
                    &event,
                    &args,
                    exact_runner::CallOrigin::App,
                ) {
                    runner.log(e);
                }
                true
            }
            In::Journal(line) => {
                p.host_mut().runner_mut().log(line);
                true
            }
            In::Presence(answer) => {
                p.host_mut().runner_mut().presence_reply(answer.as_bytes());
                true
            }
            In::Orientation {
                applied,
                conflict,
                line,
            } => {
                if let Some(line) = line {
                    p.host_mut().runner_mut().log(line);
                }
                p.orientation_applied(applied, conflict);
                true
            }
            In::Picture { id, answer } => p.picture_answered(id, answer),
            In::PictureChanged { source, revoked } => {
                p.picture_changed(&source, revoked);
                true
            }
        }
    }
}
