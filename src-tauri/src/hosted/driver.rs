//! A hosted agent's lines, made sense of: its `Protocol` (what they mean), the events they make (to the function the
//! session reports to), the lines Cue writes back, and what it last said on its error output (for saying why it
//! exited). Wherever the agent runs: as Cue's own child (session.rs), or under cue-host (hosts.rs), whose log Cue
//! reads back after a restart (`catch_up`).

use super::protocol::Protocol;
use super::{Answer, Ask, HostedEvent, Spec};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// How much of its error output is kept, for saying why it exited.
const LAST_WORDS: usize = 20;

/// Where its events go, each with the conversation's id (known only once it's open, for an agent that picks it).
pub type Sink = Arc<dyn Fn(&str, HostedEvent) + Send + Sync>;

pub struct Driver {
    /// Which agent, for saying so.
    pub name: &'static str,
    /// The id it was started under: the conversation's, unless the agent picks its own (its protocol's).
    started_as: String,
    /// What its lines mean. Never held while anything is written or reported.
    protocol: Mutex<Box<dyn Protocol>>,
    last_words: Mutex<VecDeque<String>>,
    sink: Sink,
}

/// A line of the agent's, or Cue's to it, as its host's log keeps it.
pub enum Logged {
    Out(String),
    Err(String),
    In(String),
}

impl Driver {
    pub fn new(spec: &Spec, sink: Sink) -> Driver {
        Driver { name: spec.agent.name(), started_as: spec.start.session_id().to_string(), protocol: Mutex::new(spec.agent.protocol(spec)), last_words: Mutex::new(VecDeque::new()), sink }
    }

    /// `f` with its protocol, let go before anything is written or reported.
    pub fn speak<R>(&self, f: impl FnOnce(&mut dyn Protocol) -> R) -> R {
        f(crate::locking::lock(&self.protocol, "hosted session: protocol").as_mut())
    }

    /// The conversation's id: the one it was started under, or the one the agent picked.
    pub fn session_id(&self) -> String {
        self.speak(|p| p.session_id()).unwrap_or_else(|| self.started_as.clone())
    }

    /// What's written first (a handshake, if the agent has one).
    pub fn opening(&self) -> Vec<String> {
        self.speak(|p| p.opening())
    }

    /// A line Cue wrote to it: its protocol told.
    pub fn wrote(&self, line: &str) {
        self.speak(|p| p.wrote(line));
    }

    /// One line of its output: its events reported, and the lines to write back at once.
    pub fn read(&self, line: &str) -> Vec<String> {
        let (events, replies) = self.speak(|p| p.read(line));
        let id = self.session_id();
        for e in events {
            (self.sink)(&id, e);
        }
        replies
    }

    /// One line of its error output, kept for saying why it exited.
    pub fn error_line(&self, line: String) {
        let mut kept = crate::locking::lock(&self.last_words, "hosted session: last words");
        if kept.len() == LAST_WORDS {
            kept.pop_front();
        }
        kept.push_back(line);
    }

    pub fn last_words(&self) -> String {
        crate::locking::lock(&self.last_words, "hosted session: last words").iter().cloned().collect::<Vec<_>>().join("\n")
    }

    /// Its process is gone: nothing waits for an answer any more, and it's said how it went.
    pub fn exited(&self, code: Option<i32>) {
        let during_turn = self.speak(|p| {
            p.gone();
            p.in_turn()
        });
        (self.sink)(&self.session_id(), HostedEvent::Exited { code, during_turn, last_words: self.last_words() });
    }

    /// What Cue had already read before it restarted, read again with nothing reported, so the protocol is as it
    /// was (its conversation, its turn, what it waits on). The asks it still waits on: their cards went with the Cue
    /// that had them (`ask_again`).
    pub fn catch_up(&self, read: impl IntoIterator<Item = Logged>) -> Vec<Ask> {
        let mut asked: Vec<Ask> = vec![];
        for l in read {
            match l {
                Logged::Out(line) => {
                    let (events, _) = self.speak(|p| p.read(&line));
                    asked.extend(events.into_iter().filter_map(|e| if let HostedEvent::Ask(a) = e { Some(a) } else { None }));
                }
                Logged::In(line) => self.wrote(&line),
                Logged::Err(line) => self.error_line(line),
            }
        }
        asked.into_iter().filter(|a| self.speak(|p| p.waits_on(&a.request_id))).collect()
    }

    /// Asks reported again (a card each).
    pub fn ask_again(&self, asks: Vec<Ask>) {
        let id = self.session_id();
        for a in asks {
            (self.sink)(&id, HostedEvent::Ask(a));
        }
    }

    /// A message of yours it will never get, and why (the lines carrying it couldn't be handed over).
    pub fn not_taken(&self, text: &str, why: &str) {
        (self.sink)(&self.session_id(), HostedEvent::NotTaken { text: text.to_string(), why: why.to_string() });
    }

    pub fn message(&self, text: &str) -> Vec<String> {
        self.speak(|p| p.message(text))
    }

    pub fn answer(&self, request_id: &str, a: &Answer) -> Result<Vec<String>, String> {
        self.speak(|p| p.answer(request_id, a))
    }

    pub fn interrupt(&self) -> Vec<String> {
        self.speak(|p| p.interrupt())
    }

    pub fn ready(&self) -> bool {
        self.speak(|p| p.ready())
    }

    pub fn in_turn(&self) -> bool {
        self.speak(|p| p.in_turn())
    }

    pub fn transcript(&self) -> Option<String> {
        self.speak(|p| p.transcript())
    }
}
