//! A machine's session in the built-in terminal, its machine played by testing::FakeSsh: the client goes
//! through the same ssh as every other trip (machines::ssh_program), and its view there goes before it.

use super::support::until;
use super::*;
use crate::testing::FakeSsh;
use base64::Engine;
use std::sync::{Arc, Mutex as M};

/// A machine whose tmux finds pane %3 in session "work", window @1, and whose client prints a line and stays.
const BOX: &str = r#"case "$1" in
  *"command -v tmux"*) echo /opt/bin/tmux ;;
  *display-message*) echo 'work|@1' ;;
  *new-session*) echo CLIENT-UP; exec sleep 30 ;;
  *kill-session*) ;;
  *) exit 1 ;;
esac"#;

#[test]
fn a_machines_terminal_runs_its_client_through_the_machines_ssh_and_its_view_goes_first_on_close() {
    let ssh = FakeSsh::new();
    let host = format!("term-box-{}", std::process::id());
    ssh.host(&host, BOX);
    let printed = Arc::new(M::new(String::new()));
    let p = printed.clone();
    let sid = format!("remote-term-{}", std::process::id());
    open_with(&sid, "%3", Some(&host), 80, 12, move |v| {
        if let Some(d) = v["data"].as_str() {
            crate::locking::lock(&p, "printed").push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(d).unwrap()));
        }
    })
    .unwrap();
    assert!(until(|| crate::locking::lock(&printed, "printed").contains("CLIENT-UP")), "the client's output reaches the window");
    let client = ssh.calls().into_iter().find(|(_, c)| c.contains("new-session")).map(|(_, c)| c).unwrap();
    // Quoted twice over (for ssh's shell, then sh -c): its words with the quoting taken off.
    let words = client.replace("'\\''", "").replace('\'', "");
    assert!(words.starts_with("exec /bin/sh -c exec /opt/bin/tmux -u new-session -t work -s cue-view-"), "tmux by its path there, grouped with the pane's session: {client}");
    close(&sid);
    assert!(until(|| ssh.calls_with(&host, "kill-session") == 1), "its view is removed there");
}
