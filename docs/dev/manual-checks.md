# Manual checks before a release

What the test suites can't reach: the real microphone, speaker and Notification Center. Three checks, a
minute or two each, on the build about to ship (stability plan §4 Voice / speak, §9). Everything around
them (the ask added once, Settings → Voice from cue-voice's status lines, a stand-in cue-voice's lines,
notifications posted and taken back) is covered by `tests/run.sh unit`.

1. **Talk and hear the answer.** Settings → Voice: Talk to sessions on, Read answers out on. Open a
   session's chat, hold right ⇧, say "say the word banana", let go. The chip shows it working; when it
   answers, tap right ⇧: the answer is read out in the picked voice, and the session's chat shows what you
   said without the "(Voice: …)" line.
2. **Dictate into a text box.** Click the mic in a reply box, say "run the tests", stop. The words appear
   in the box as you speak and stay once it stops; nothing is sent until you press ↩.
3. **A notification comes and goes.** With Cue in the background, finish a turn in any session: one
   notification with Reply on it. Reply from Cue's window instead: the notification leaves Notification
   Center within a couple of seconds.
