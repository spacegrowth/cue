# Hands-free voice on the Mac: which way to pick

**Picked: B, say "Hey Cue" (20 Nov 2026).** Below is what each option changes, what it costs, and what building B involves.

## What happens today

Voice on the Mac is push-to-talk. You hold right ⇧ (or ⌥Space) anywhere, talk, and let go. Your words go to the chat that's open in Cue, or else to the session that moved last. The island at the notch, or the caption bar, reads the answer out. That session's replies are also read for the next 30 minutes.

But nothing listens after an answer. To reply, you reach for the key again, every turn.

## The three options

**A, talk back and forth (medium).** Double-tap right ⇧ to start a conversation with the open session. A 1.5 second pause sends what you said. After each answer the island shows "Your turn" with an 8 second bar and listens again. Silence, "that's all", or a tap on ⇧ ends it. Holding ⇧ still does one turn, as it does today. Most of the pieces already exist, so this is the safest of the three.

**B, say "Hey Cue" (large).** A switch in Settings → Voice, "Listen for Hey Cue", keeps the mic armed. While it's on, a mark in the menu bar and the Mac's own orange mic dot show it. "Hey Cue, ask four ninety-seven…" opens the island and picks that session by name, or the open chat if you don't name one. After the answer it listens for 6 seconds, so you can reply without saying the word again. This is truly hands-free, but the mic stays open.

**C, hands-free with the Desk (medium to large).** Double-tap right ⇧ to talk to the Desk's lead instead of one session, and the lead sends each part where it belongs. While this is on, every reply that comes back is read in turn ("From cue-497 …") with Reply, Next and Skip all, plus a 6 second "Reply?" that goes back to that session. Between replies, the notch shows how many sessions are still working.

## What building B involves

- **The switch.** "Listen for Hey Cue" goes in Settings → Voice and is off by default. It turns off by itself when the screen locks, and turns back on when you unlock.
- **Showing it's listening.** A mark in the menu bar is shown whenever the mic is armed, next to the Mac's own orange dot.
- **The wake word.** "Hey Cue" is heard on the Mac itself, either by a small on-device model or by Apple's speech recognition running all the time. Nothing leaves the Mac until the word is heard.
- **Picking the session.** Spoken names ("four ninety-seven", a session's title) are matched to sessions. If there's no name, the words go to the open chat, as with push-to-talk today.
- **Replying.** After an answer is read, the mic listens for 6 seconds with no wake word. It never listens while it's reading, so it doesn't hear itself.
- **Push-to-talk stays.** Holding right ⇧ or ⌥Space still works as it does today.

## What B costs

A large amount of work. The parts that need the most care are false wake-ups (from a video or a call), the battery use of listening all the time, and matching spoken session names. The island, the reading and the mic are already there.

## Built: option B (#538)

Settings → Voice → Listen for “Hey Cue” (under Talk to sessions). cue-voice keeps Apple's recognizer listening on
the Mac while it's on, the Mac is unlocked and nothing else is using the mic; a ring-and-dot mark in the menu bar
shows it (its menu: Stop Listening). After "Hey Cue", what you say ends after 1.5 s of quiet (6 s with nothing
said: back to waiting). Cue matches the session's name, folder or number at the start ("ask four ninety-seven…",
"cue-497, …", "tell 497 to…"), else the chat open in Cue. After a hands-free answer is read out, "Reply?" waits
6 s for a reply to the same session. Esc cancels either. Tests: VoiceCoreTests (wake word, on/off, end of speech,
the reply window) and talk_tests (picking by name, the switch).
