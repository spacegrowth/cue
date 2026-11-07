// What the e2e soak counts and samples, written as the JSON `cargo run -p xtask -- soak-report` judges (the
// same shape src-tauri/src/testing/soak writes): the window's answer times, memory and the window's state
// over time, each decision's trip to its hook, and what Cue's own probe counted.
export class Tally {
  constructor() {
    this.start = performance.now();
    this.counts = { eventsSent: 0, eventsFailed: 0, typingSent: 0, typingFailed: 0, polls: 0, asks: 0, opened: 0, closed: 0 };
    this.answerMs = [];
    /** What the window's main thread spent its time on, by what it was doing (load.js timePage). */
    this.pageMs = {};
    this.decisionsMs = [];
    this.undecided = 0;
    this.memory = [];
    this.snapshot = [];
    this.refused = {};
    /** A part of the load that stopped on an error of its own (the run goes on without it). */
    this.errors = [];
  }

  /** Seconds since the run started. */
  at() {
    return (performance.now() - this.start) / 1000;
  }

  refuse(why) {
    this.counts.typingFailed++;
    this.refused[why] = (this.refused[why] || 0) + 1;
  }

  sample(residentBytes, snapshotBytes) {
    this.memory.push([this.at(), residentBytes]);
    this.snapshot.push([this.at(), snapshotBytes]);
  }

  /** The run as xtask::soak judges it: `about` with what was measured, Cue's last `probe`, its `panics` and `log`. */
  json(about, probe, panics, log) {
    const c = this.counts;
    return {
      ...about, seconds: this.at(), answer_ms: this.answerMs, draws_ms: this.pageMs.setState || [], page_ms: this.pageMs, decisions_ms: this.decisionsMs, undecided: this.undecided,
      memory: this.memory, snapshot: this.snapshot,
      events: { sent: c.eventsSent, failed: c.eventsFailed }, typing: { sent: c.typingSent, failed: c.typingFailed, refused: this.refused },
      polls: c.polls, asks: c.asks, terminals: { opened: c.opened, closed: c.closed }, probe, panics, log, errors: this.errors,
    };
  }
}
