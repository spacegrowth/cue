//! Notification Center as the World has it: every notification posted and every one taken back, by id.

use crate::hub::ports::Notes;
use crate::notify::Note;
use std::sync::Mutex;

#[derive(Default)]
pub struct FakeNotes {
    shown: Mutex<Vec<String>>,
    taken_back: Mutex<Vec<String>>,
}

impl FakeNotes {
    /// The ids posted, in order.
    pub fn shown(&self) -> Vec<String> {
        crate::locking::lock(&self.shown, "fake notes shown").clone()
    }

    /// The ids taken back, in order.
    pub fn taken_back(&self) -> Vec<String> {
        crate::locking::lock(&self.taken_back, "fake notes taken back").clone()
    }
}

impl Notes for FakeNotes {
    fn show(&self, note: Note) {
        crate::locking::lock(&self.shown, "fake notes shown").push(note.id);
    }

    fn take_back(&self, ids: Vec<String>) {
        crate::locking::lock(&self.taken_back, "fake notes taken back").extend(ids);
    }
}
