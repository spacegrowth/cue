// What happens when: hold right ⇧ (or ⌥Space) and it listens (the pill shows your words); let go and what you said
// goes to the session (through Cue); while it works the island is small, round the notch, and words go to a line along the bottom of the screen; its answer comes back and the
// pill folds to a small tab that waits (Listen, Read it here, Later as icons; right ⇧, a tap or a hold, is Listen) until you pick, or reads at once (Settings → Voice: Start reading by itself). While holding: esc cancels,
// left ⇧ pauses (what you said waits; hold again to go on, and it all goes as one), and ↩ (or a click on the pill) goes
// to typing instead: what you said lands in that chat's box in Cue, unsent. While it reads: ⌥Space talks back. Esc,
// whenever the pill is up, gets out of it all. A click on the pill opens that session's chat in Cue.
import AppKit
import Carbon.HIToolbox
import VoiceCore

/// Who you'd be talking to, as Cue says: the session open in its window, else the latest one.
struct Target: Equatable {
  var sid: String, name: String, agent: String, look: Look
}

@MainActor
final class Voice {
  typealias Phase = VoicePhase

  private let pill = Pill()
  private let listen = Listen()
  private let speak = Speak()
  /// What's playing pauses while you talk and while it reads.
  private let hush = Hush()
  private let keys = HotKeys()
  private var phase = Phase.idle { didSet { syncEsc() } }
  /// The pill is up.
  private var shown = false
  /// When what you said went: how long it took.
  private var sentAt: Date?
  /// The voice the waiting answer is to be read in.
  private var voiceName = ""
  private var target: Target?
  /// This turn's session (the target can change while it works).
  private var talkingTo: Target?
  /// Paused: what you've said so far.
  private var draft = ""
  private var heard = ""
  private var answer = ""
  private var shiftWatch: Timer?
  /// The key is down now (it can come up before the mic has started: the first time, macOS asks first).
  private var holding = false
  private var model = "off"
  private var progress = 0.0

  /// Hot key numbers.
  private let talkKey: UInt32 = 1, escKey: UInt32 = 2, escOptKey: UInt32 = 3, escShiftKey: UInt32 = 4
  private let retKey: UInt32 = 5, retOptKey: UInt32 = 6, retShiftKey: UInt32 = 7
  /// → while holding right ⇧: what you say goes "by the way" (a side question; the session keeps working).
  private let arrowKey: UInt32 = 8, arrowShiftKey: UInt32 = 9
  private var btw = false
  /// By the way's own colors: blue, in the widest gap between the ten project swatches (cyan and violet), made
  /// as they are (OKLCH, hue 236), so it's never a project's.
  static let btwLook = Look(bg: "#27485B", onBg: "#FFFFFF", sel: "#ABDEFD", onSel: "#14242D")
  /// A turn under way (what you said went and its answer isn't back): its session and when it went. A by the way
  /// asked meanwhile goes to it, and the island goes back to it once that answer's dealt with.
  private var turns = VoiceTurns<Told>()
  private var turnOn: Bool { turns.turnOn }
  private var asking: Int { turns.asking }
  /// Who what you last said went to (it's confirmed as sent a moment later, by then you may be talking again).
  private var saidTo: Target?
  /// What the session was last said to be doing (the island goes back to it after a by the way).
  private var lastDoing = "Thinking"
  /// What you said is still being written down (after you let go, before it's sent): "sending" is yours till then,
  /// and nothing that comes from Cue meanwhile (a "sent" for what you said before, a "stopped") ends it.
  private var finishing = false
  /// Which `after` is the latest: an earlier one's time mustn't end what came after it.
  private var afters = 0
  private var turnWho: Target?
  private var turnSentAt: Date?
  /// Answers that came while you were busy with another (listening, reading one): told, one after another, once
  /// you're done. Never dropped.
  private struct Told { var text: String, voice: String, btw: Bool, who: Target? }
  /// The session whose answer was read out last: one from another is said to be from its project first.
  private var lastReadSid: String?
  /// What the island shows of the answer being read, with live captions off.
  private var readText = ""
  /// Right ⇧ held a third of a second on its own; left ⇧ pauses.
  private var rightShift: HeldKey?

  init() {
    listen.onWords = { [weak self] w in self?.words(w) }
    listen.onLevel = { [weak self] l in self?.pill.m.level = l }
    speak.onProgress = { [weak self] at in self?.reading(at) }
    speak.onDone = { [weak self] _ in self?.doneReading() }
    pill.onShown = { [weak self] on in self?.shown = on; self?.syncEsc() }
    pill.onClick = { [weak self] in
      guard let self else { return }
      if self.phase == .listening || self.phase == .paused { return self.toChat() }
      if self.phase == .answered { return self.openChat() }
      if let sid = (self.turnOn ? self.turnWho : nil)?.sid ?? (self.talkingTo ?? self.target)?.sid { emit(["open": sid]) }
    }
    talk(Prefs.load().talk)
  }

  /// Talking to sessions turned on (Settings → Voice): right ⇧ and ⌥Space are taken, the listening model got
  /// ready. Off: both are left to other apps.
  private var talking = false
  private func talk(_ on: Bool) {
    guard on != talking else { return }
    talking = on
    guard on else {
      keys.remove(talkKey)
      rightShift?.stop()
      rightShift = nil
      if phase == .listening || phase == .paused { holding = false; escape() }
      note("talking off")
      return report(keys: true)
    }
    let ok = keys.add(talkKey, key: kVK_Space, mods: optionKey) { [weak self] down in MainActor.assumeIsolated { down ? self?.down() : self?.up() } }
    rightShift = HeldKey(HeldKey.rightShift, after: 0.3, tap: { [weak self] in self?.tapped() }) { [weak self] down in down ? self?.down() : self?.up() }
    // → is taken from other apps from the moment right ⇧ goes down (not only once it listens): pressed together
    // for by the way, it went to the app in front first (a film skipped ahead).
    rightShift?.pressed = { [weak self] down in
      guard let self else { return }
      if down { self.takeArrow(true) } else if self.phase != .listening { self.takeArrow(false) }
    }
    note("talking on; ⌥Space \(ok ? "taken" : "not free")")
    report(keys: ok)
    pickModel()
  }

  /// NVIDIA's listening model, got and loaded only once it's picked (Settings → Voice → Listening): until then, or
  /// with Apple's picked, Apple's recognizer writes down what you say and nothing is downloaded.
  private func pickModel() {
    if Prefs.load().model == "parakeet" {
      if model == "off" { getModel() }
    } else if model == "off" {
      report()   // says Apple's is in use
    }
  }

  // MARK: Cue's lines

  func heard(_ o: [String: Any]) {
    if let on = o["talk"] as? Bool { talk(on) }
    if o["model"] != nil, talking { pickModel() }   // Listening picked in Settings: NVIDIA's comes now
    if o.keys.contains("target") {
      target = (o["target"] as? [String: Any]).map(Voice.target)
      if phase == .idle, let t = target { dress(t) }
      note("target \(target?.name ?? "none")")
    }
    // What you said went: the turn is under way, whatever you're doing now (you may be talking again already).
    if o["sent"] != nil {
      turns.sent()
      let sid = o["sid"] as? String
      turnWho = saidTo.flatMap { $0.sid == sid || sid == nil ? $0 : nil } ?? talkingTo
      turnSentAt = Date()
      lastDoing = "Thinking"
      // Sent: an answer that came while you talked comes up first, else the island goes to this turn.
      if phase == .sending, !finishing { settle() }
    }
    // What the session does now; a by the way being asked says so instead.
    if let d = o["doing"] as? String {
      lastDoing = d
      if asking == 0 { pill.m.doing = d; pill.place() }
    }
    if let e = o["error"] as? String {
      if o["btw"] != nil {
        // A by the way that couldn't be asked: said, without touching whatever you're doing now.
        turns.byTheWayFailed()
        note("by the way failed: \(e)")
        if phase == .working { settle() }   // back to the turn (its step, not "By the way…"), or away
      } else if phase == .sending || phase == .idle { fail(e) }
      else { note("error meanwhile: \(e)") }
    }
    // A turn under way when Cue restarted: back to waiting on it.
    if let w = o["resume"] as? [String: Any], phase == .idle {
      turns.sent()
      turnWho = Voice.target(w)
      turnSentAt = nil
      talkingTo = turnWho
      dress(talkingTo!)
      note("resumed waiting on \(talkingTo!.name)")
      work()
    }
    if let a = o["answer"] as? String {
      let btw = o["btw"] != nil
      let told = Told(text: a, voice: o["voice"] as? String ?? "", btw: btw, who: (o["who"] as? [String: Any]).map(Voice.target))
      // Told now when nothing else is on; else it waits its turn (never dropped).
      if turns.answered(told, byTheWay: btw, follow: o["follow"] != nil, phase: phase) { present(told) } else { note("answer waiting its turn") }
    }
    if o["stopped"] != nil {
      turns.stopped()
      if phase == .working { settle() }   // not while sending: what you said is on its way
    }
  }

  /// An answer up for you: whose it is, and how long its turn took.
  private func present(_ a: Told) {
    if let w = a.who { talkingTo = w } else if !a.btw, let w = turnWho { talkingTo = w }
    sentAt = a.btw || a.who != nil ? nil : turnSentAt
    answered(a.text, voice: a.voice, btw: a.btw)
  }

  /// Done with an answer (heard, read, put off): the next one waiting, else back to the turn still under way, else
  /// the island goes.
  private func settle() {
    switch turns.next() {
    case .present(let a): present(a)
    case .work:
      if let w = turnWho, turnOn { talkingTo = w; dress(w) }
      work()
    case .idle:
      phase = .idle
      pill.hide()
    }
  }

  private static func target(_ t: [String: Any]) -> Target {
    Target(sid: t["sid"] as? String ?? "", name: t["name"] as? String ?? "", agent: t["agent"] as? String ?? "",
           look: Look(bg: t["bg"] as? String ?? "#3F4A22", onBg: t["onBg"] as? String ?? "#FFFFFF", sel: t["sel"] as? String ?? "#D2DE96", onSel: t["onSel"] as? String ?? "#1E2410"))
  }

  // MARK: The key

  private func down() {
    note("key down, \(phase)")
    if phase == .listening || holding { return }   // held (a repeat)
    // An answer waiting that you haven't heard: holding reads it (as a tap does). Hold again while it reads to talk.
    if phase == .answered, Prefs.load().read { return listenNow() }
    holding = true
    // Talking back over it: it isn't reading any more (first: the stop's "done" mustn't play the media again).
    if phase == .saying { phase = .idle; readPaused = false }
    speak.stop()
    guard let t = phase == .paused ? (talkingTo ?? target) : target else {
      holding = false
      // An answer up for you stays as it is (nobody to talk back to now; Listen still works).
      if phase == .answered { return note("no session to talk to") }
      show(label: "", text: "No session to talk to: start one in Cue.", yours: false, keys: [])
      pill.show()
      if phase == .working || phase == .answered || phase == .said { phase = .idle }
      after(3) { $0.phase == .idle }   // then what was on (an answer waiting, a turn under way), else away
      return
    }
    hush.on()
    talkingTo = t
    dress(t)
    // At once, so the key is felt to work even while macOS asks for the microphone the first time.
    show(label: "LISTENING", text: draft.isEmpty ? "…" : draft, yours: true, keys: [])
    pill.show()
    Task {
      if let why = await listen.allowed() { return fail(why) }
      note("allowed")
      do { try listen.start() } catch { note("mic: \(error)"); return fail("Couldn't start the microphone: \(error.localizedDescription)") }
      phase = .listening
      heard = ""
      pill.m.since = Date()
      words("")
      takeReturn(true)
      takeArrow(true)
      // Pressed with right ⇧ (both at once): by the way from the start.
      btw = CGEventSource.keyState(.combinedSessionState, key: CGKeyCode(kVK_RightArrow)) || rightShift?.arrowSeen == true
      if btw { pill.m.look = Voice.btwLook }
      // Let go already (while macOS asked): what was heard, which is nothing, ends it.
      if !holding { return up() }
      // Left ⇧ pauses: its state is read as you hold (no key permission needed).
      shiftWatch = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
        MainActor.assumeIsolated { if HeldKey.down(HeldKey.leftShift) { self?.pause() } }
      }
    }
  }

  private func up() {
    note("key up, \(phase)")
    holding = false
    guard phase == .listening else { return }
    stopWatch()
    takeReturn(false)
    takeArrow(false)
    phase = .sending
    hush.off()
    show(label: "SENDING", text: joined(draft, heard), yours: true, keys: [])
    pill.m.since = nil
    // Who it goes to, as you let go (talking again while it's written down may pick someone else).
    let to = talkingTo
    finishing = true
    Task {
      let said = joined(draft, await listen.finish())
      finishing = false
      draft = ""
      guard phase == .sending else { return }   // esc meanwhile
      guard !said.isEmpty, let t = to else { btw = false; return settle() }   // nothing said: back to what was on
      if VoiceText.strayYeah(said) { note("only \"yeah\" heard, not sent"); btw = false; return settle() }
      if btw {
        // Asked of a copy of its conversation: the session keeps going; the answer comes back as any does. With a
        // turn under way, it's about that turn's session.
        btw = false
        let who = turnOn ? (turnWho ?? t) : t
        talkingTo = who
        pill.m.name = who.name   // its name; the colors stay by the way's blue while the bar goes
        turns.askedByTheWay()
        note("by the way, \(said.count) characters")
        emit(["btw": said, "sid": who.sid])
        work()
        pill.m.doing = "By the way…"
        return
      }
      show(label: "SENDING", text: said, yours: true, keys: [])
      note("said \(said.count) characters")
      saidTo = t
      emit(["said": said, "sid": t.sid])
    }
  }

  private func pause() {
    guard phase == .listening else { return }
    stopWatch()
    phase = .paused
    pill.m.since = nil
    Task {
      draft = joined(draft, await listen.finish())
      show(label: "PAUSED", text: draft, yours: true, keys: [("right ⇧", "hold to go on"), ("↩", "type it"), ("esc", "drop it")])
      pill.show()
    }
  }

  /// Esc, while the pill is up for a moment (never while it works or an answer waits: it isn't taken then): out of
  /// it. Listening, what you said is dropped; reading, it stops. Then the next answer waiting, or back to a turn
  /// still under way, else the pill goes.
  private func escape() {
    btw = false
    switch phase {
    case .listening, .paused:
      stopWatch()
      listen.cancel()
      draft = ""
    case .saying:
      phase = .idle   // first: its "done" would show what it said
      speak.stop()
    default: break
    }
    phase = .idle
    takeReturn(false)
    takeArrow(false)
    hush.off()
    pill.m.speaking = false
    settle()
  }

  /// Typing instead: listening stops, and what you said so far goes to that chat's box in Cue (unsent), which
  /// comes forward with the cursor in it.
  private func toChat() {
    btw = false
    guard phase == .listening || phase == .paused, let t = talkingTo else { return }
    let listening = phase == .listening
    stopWatch()
    takeReturn(false)
    takeArrow(false)
    hush.off()
    phase = .idle
    show(label: "TO CHAT", text: draft, yours: true, keys: [])
    pill.m.since = nil
    Task {
      let said = joined(draft, listening ? await listen.finish() : "")
      draft = ""
      if phase == .idle { settle() }   // back to a turn still under way, unless an answer came up meanwhile
      note("to chat, \(said.count) characters")
      emit(["draft": said, "sid": t.sid])
    }
  }

  /// → is taken from other apps only while right ⇧ is down or it listens (with right ⇧ held it would select text
  /// there). Pressed before it listens, it's seen by HeldKey (arrowSeen): by the way from the start.
  private func takeArrow(_ on: Bool) {
    if on {
      for (n, mods) in [(arrowKey, 0), (arrowShiftKey, shiftKey)] {
        keys.add(n, key: kVK_RightArrow, mods: mods) { [weak self] down in
          if down { MainActor.assumeIsolated { guard let self, self.phase == .listening else { return }; self.btw = true; self.pill.m.look = Voice.btwLook; self.words(self.heard) } }
        }
      }
    } else {
      keys.remove(arrowKey)
      keys.remove(arrowShiftKey)
    }
  }

  /// ↩ is taken from other apps only while listening (with ⌥ or ⇧ too: you may still hold one).
  private func takeReturn(_ on: Bool) {
    if on {
      for (n, mods) in [(retKey, 0), (retOptKey, optionKey), (retShiftKey, shiftKey)] {
        keys.add(n, key: kVK_Return, mods: mods) { [weak self] down in if down { MainActor.assumeIsolated { self?.toChat() } } }
      }
    } else {
      keys.remove(retKey)
      keys.remove(retOptKey)
      keys.remove(retShiftKey)
    }
  }

  /// Esc is taken from other apps only while the pill is up (with ⌥ or ⇧ too: you may still hold one).
  private func takeEsc(_ on: Bool) {
    if on {
      keys.add(escKey, key: kVK_Escape, mods: 0) { [weak self] down in if down { MainActor.assumeIsolated { self?.escape() } } }
      keys.add(escOptKey, key: kVK_Escape, mods: optionKey) { [weak self] down in if down { MainActor.assumeIsolated { self?.escape() } } }
      keys.add(escShiftKey, key: kVK_Escape, mods: shiftKey) { [weak self] down in if down { MainActor.assumeIsolated { self?.escape() } } }
    } else {
      keys.remove(escKey)
      keys.remove(escOptKey)
      keys.remove(escShiftKey)
    }
  }

  /// Esc is taken while the pill is up, except while it works (small, round the notch) or with an answer waiting: those stay up for long, and
  /// other apps need their esc meanwhile.
  private func syncEsc() { takeEsc(shown && phase != .working && phase != .answered) }

  private func stopWatch() { shiftWatch?.invalidate(); shiftWatch = nil }

  // MARK: What the pill shows

  private func words(_ w: String) {
    heard = w
    guard phase == .listening else { return }
    show(label: btw ? "BY THE WAY" : "LISTENING", text: joined(draft, w).isEmpty ? "…" : joined(draft, w), yours: true,
         keys: btw ? [("esc", "cancel"), ("let go", "ask it")] : [("esc", "cancel"), ("left ⇧", "pause"), ("↩", "type it"), ("→ too", "by the way"), ("let go", "send")])
    pill.m.showLevel = true
    pill.show()
  }

  /// Sent: the island goes small, round the notch, and stays while it works (its clock from when the turn went).
  private func work() {
    phase = .working
    if asking == 0 { pill.m.doing = turnOn ? lastDoing : "Thinking" }
    pill.m.since = turnOn ? (turnSentAt ?? Date()) : Date()
    show(label: "WORKING", text: "", yours: false, keys: [], compact: true)
    pill.show()
  }

  /// The answer is back: with a sound, a small tab says so (captions on, in from the right edge; off, the island
  /// at the notch's height) with Listen, Read it here and Later as icons, each acting at once; nothing is read
  /// until you pick, unless Settings → Voice reads it straight away. A click on its dot (or ANSWERED) opens the chat.
  private func answered(_ text: String, voice: String, btw: Bool = false) {
    answer = text
    voiceName = voice
    phase = .answered
    pill.m.since = nil
    if let t = talkingTo { dress(t) }
    if btw { pill.m.look = Voice.btwLook }   // a by the way's answer in its own blue, as it was asked
    let asking = !btw && text.hasPrefix("It's asking:")
    let took = btw ? nil : sentAt.map { Voice.took(Date().timeIntervalSince($0)) }
    let out = Prefs.load().read   // Settings → Voice: answers read out, or only shown
    NSSound(named: "Tink")?.play()
    // Settings → Voice: read it out at once, always or while Cue is in front (no tab first).
    let auto = Prefs.load().auto
    if out, auto == "always" || (auto == "front" && NSWorkspace.shared.frontmostApplication?.bundleIdentifier == "dev.spacegrowth.cue") {
      note("answer back, read at once (\(text.count) characters)")
      return listenNow()
    }
    var buttons = [PillButton(title: "Read it here", icon: "text.alignleft", primary: !out) { [weak self] in self?.readHere() },
                   PillButton(title: "Later", icon: "clock") { [weak self] in self?.later() }]
    if out { buttons.insert(PillButton(title: "Listen", icon: "speaker.wave.2.fill", primary: true) { [weak self] in self?.listenNow() }, at: 0) }
    pill.m.short = btw ? "BY THE WAY" : asking ? "ASKING YOU" : "ANSWERED" + (took.map { " · \($0)" } ?? "")
    pill.m.more = turns.pending.count
    show(label: (btw ? "BY THE WAY · ANSWERED" : asking ? "\(agentWord) IS ASKING YOU" : "\(agentWord) ANSWERED\(took.map { " · TOOK \($0)" } ?? "")") + VoiceText.more(turns.pending.count), text: "", yours: false,
         keys: [out ? ("hold right ⇧", "listen") : ("hold right ⇧", "talk back")], buttons: buttons, folded: true)
    pill.show()
    note("answer back, waiting (\(text.count) characters)")
  }

  /// Right ⇧ tapped (not held): with an answer waiting, Listen (or Read it here when answers aren't read out);
  /// while it reads, pause and go on.
  private func tapped() {
    if phase == .saying { return pauseReading(!readPaused) }
    guard phase == .answered else { return }
    Prefs.load().read ? listenNow() : readHere()
  }

  /// Reading held where it is (a tap on right ⇧), or going on.
  private var readPaused = false
  private func pauseReading(_ on: Bool) {
    readPaused = on
    if on { speak.pause() } else { speak.resume() }
    pill.m.speaking = !on
    show(label: (on ? "PAUSED" : "\(agentWord) IS SAYING") + VoiceText.more(turns.pending.count), text: "", yours: false, keys: sayingKeys, line: true)
    if !pill.m.ticker { pill.m.text = readText }   // no live captions: what was read stays in the island
  }
  private var sayingKeys: [(String, String)] { [("tap right ⇧", readPaused ? "go on" : "pause"), ("hold right ⇧", "talk back"), ("esc", turns.pending.isEmpty ? "stop" : "next")] }

  /// The answer, read out in the voice Cue sent (its Settings → Voice: "" the Mac's own, else a natural one).
  private func listenNow() {
    guard phase == .answered else { return }
    phase = .saying
    readPaused = false
    // Whose it is, said first when that's needed to know (another session's than the last, or more waiting).
    if let w = talkingTo {
      answer = VoiceText.spoken(answer, name: w.name, sid: w.sid, lastSid: lastReadSid, more: turns.pending.count)
      lastReadSid = w.sid
    }
    show(label: "\(agentWord) IS SAYING" + VoiceText.more(turns.pending.count), text: "", yours: false, keys: sayingKeys, line: true)
    pill.m.speaking = true
    hush.on()
    speak.say(answer, voice: voiceName)
    reading(0)
    pill.show()
  }

  /// The answer, shown whole and not read out; a while, then it goes.
  private func readHere() {
    guard phase == .answered else { return }
    phase = .said
    show(label: "\(agentWord) SAID", text: Speak.plain(answer), yours: false, keys: [("esc", "close"), ("right ⇧", "talk back"), ("click", "open the chat")])
    pill.show()
    after(30) { $0.phase == .said }
  }

  /// `seconds` on, if `still` holds then (nothing else happened meanwhile): done with it.
  private func after(_ seconds: Double, _ still: @escaping (Voice) -> Bool) {
    afters += 1
    let this = afters
    DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
      MainActor.assumeIsolated { if let self, this == self.afters, still(self) { self.settle() } }
    }
  }

  private func openChat() {
    if let sid = (talkingTo ?? target)?.sid { emit(["open": sid]) }
    later()
  }

  /// Put away: the answer is in the chat.
  private func later() {
    settle()
  }

  /// How long a turn took, as the pill says it: 40S, 4M, 1H 51M.
  static func took(_ s: TimeInterval) -> String { VoiceText.took(s) }

  /// The answer on the line, read to the end of the word it's on.
  private func reading(_ at: Double) {
    guard phase == .saying else { return }
    let plain = Speak.plain(answer)
    let upTo = VoiceText.readTo(plain, at: at)
    let end = plain.index(plain.startIndex, offsetBy: upTo)
    pill.m.line = plain
    pill.m.upTo = upTo
    pill.m.left = max(0, speak.total * (1 - at))
    readText = String(plain[..<end])
    if !pill.m.ticker { pill.m.text = readText }   // no live captions: the island shows it as it's read
    pill.place()
  }

  private func doneReading() {
    guard phase == .saying else { return }
    // Done: it goes a second later, so you have everything back (hold right ⇧ to talk back as ever).
    phase = .said
    pill.m.speaking = false
    hush.off()
    pill.show()
    after(1) { $0.phase == .said }
  }

  /// Who answers: Claude Code, Codex, Pi.
  private var agentWord: String { ((talkingTo ?? target)?.agent).flatMap { $0.isEmpty ? nil : $0.uppercased() } ?? "IT" }

  private func fail(_ why: String) {
    btw = false
    note("failed: \(why)")
    holding = false
    stopWatch()
    listen.cancel()
    takeReturn(false)
    takeArrow(false)
    hush.off()
    phase = .idle
    show(label: "", text: why, yours: false, keys: [])
    pill.show()
    after(5) { $0.phase == .idle }   // then back to a turn still under way, if one is
  }

  private func dress(_ t: Target) {
    pill.m.name = t.name
    pill.m.look = t.look
  }

  /// What the island shows. Your words (`yours`) go to the line along the bottom instead, as its answer does while
  /// it reads (`line`: the line keeps what `reading` put in it).
  private func show(label: String, text: String, yours: Bool, keys: [(String, String)], buttons: [PillButton] = [], compact: Bool = false, line: Bool = false, folded: Bool = false) {
    pill.m.compact = compact
    pill.m.waiting = folded
    pill.m.buttons = buttons
    pill.m.label = label
    // Live captions (Settings → Voice): the bar along the bottom is all there is (who, what's happening, the words,
    // the keys and buttons) and the island never shows; while it works, only a dot breathes in the corner. Off: the
    // island shows it all, the words too.
    let bar = Prefs.load().captions
    pill.m.bar = bar
    pill.m.text = (yours && bar) || line ? "" : text
    pill.m.yours = yours
    pill.m.ticker = bar && !compact && !folded   // folded: the tab at the right edge instead (Pill)
    if yours { pill.m.line = text; pill.m.upTo = text.count } else if !line { pill.m.line = ""; pill.m.upTo = 0 }
    // The bar shows one of the keys, picked at random each time they change, so they're learned over time; half the
    // time none at all (-1), so it's not always there.
    if keys.map(\.0) != pill.m.keys.map(\.0) { pill.m.hint = Bool.random() ? keys.indices.randomElement() ?? -1 : -1 }
    pill.m.keys = keys
    pill.m.showLevel = false
    if phase != .saying { pill.m.speaking = false; pill.m.left = nil }
    pill.place()
  }

  private func joined(_ a: String, _ b: String) -> String {
    [a, b].map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }.joined(separator: " ")
  }

  // MARK: The listening model

  private func getModel() {
    model = Parakeet.downloaded ? "loading" : "downloading"
    report()
    Task {
      do {
        try await Parakeet.shared.prepare { at in DispatchQueue.main.async { MainActor.assumeIsolated { self.progress = at; self.report() } } }
        model = "ready"
      } catch {
        model = "failed: \(error.localizedDescription)"
      }
      report()
    }
  }

  private var keysOk = true
  private var reported = Date.distantPast
  /// How it is, for Cue's Settings: the key, and the listening model.
  private func report(keys ok: Bool? = nil) {
    if let ok { keysOk = ok }
    // Download progress: at most once a second.
    if model == "downloading", Date().timeIntervalSince(reported) < 1 { return }
    reported = Date()
    // "pick": which one writes down what you say, so Settings shows it when you've never picked one.
    emit(["status": ["key": keysOk, "model": model, "progress": progress, "pick": Prefs.load().model]])
  }
}
