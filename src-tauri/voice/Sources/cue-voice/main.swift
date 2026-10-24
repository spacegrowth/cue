// cue-voice: hold ⌥Space anywhere on the Mac, talk, let go, and it goes to the session you're on in Cue;
// its short answer is read out. A pill at the top of the screen shows your words as you say them, then the
// answer. Cue starts it (talk.rs) and they talk in JSON lines: Cue tells it who you'd be
// talking to and what came back; it tells Cue what you said. Everything you say stays on this Mac: Apple's
// on-device recognizer for the words as you go, NVIDIA's Parakeet for what's sent.
import AppKit

/// One line to Cue.
func emit(_ obj: [String: Any]) {
  guard let data = try? JSONSerialization.data(withJSONObject: obj), var line = String(data: data, encoding: .utf8) else { return }
  line += "\n"
  FileHandle.standardOutput.write(line.data(using: .utf8)!)
}

/// A line in Cue's voice log (Application Support/dev.spacegrowth.cue/voice.log): what it did, for when it doesn't.
func note(_ what: String) {
  let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("dev.spacegrowth.cue")
  let url = dir.appendingPathComponent("voice.log")
  let line = "\(ISO8601DateFormatter().string(from: Date())) \(what)\n"
  if let h = try? FileHandle(forWritingTo: url) { h.seekToEndOfFile(); h.write(line.data(using: .utf8)!); try? h.close() }
  else { try? line.write(to: url, atomically: true, encoding: .utf8) }
}

// `cue-voice listen`: dictation for Cue's text boxes instead (its own protocol, as cue-listen's).
if CommandLine.arguments.dropFirst().first == "listen" { Dictate.run() }

let app = NSApplication.shared
app.setActivationPolicy(.accessory)   // no Dock icon, no menu: it's Cue's
let voice = MainActor.assumeIsolated { Voice() }

// Cue's lines. Its stdin closing (Cue quit) ends this too.
Thread.detachNewThread {
  while let line = readLine() {
    guard let data = line.data(using: .utf8), let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { continue }
    DispatchQueue.main.async { MainActor.assumeIsolated { voice.heard(obj) } }
  }
  DispatchQueue.main.async { exit(0) }
}

app.run()
