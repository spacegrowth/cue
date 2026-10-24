// `cue-voice listen`: the mic in Cue's text boxes, with NVIDIA's Parakeet for the final text (Apple's on-device
// recognizer writes the words as you go). The same lines as Cue's own cue-listen, so Cue reads it the same way:
// {"listening": true}, {"text", "final"} as you talk, then the final text once Cue closes its stdin (or sends
// "stop"), and it exits. Problems come out as {"error"} and exit 1.
import Foundation

enum Dictate {
  static func run() -> Never {
    MainActor.assumeIsolated {
      let listen = Listen()
      listen.onWords = { emit(["text": $0, "final": false]) }
      // Loaded now, while you talk, so the final text doesn't wait for it.
      if Prefs.load().model != "apple", Parakeet.downloaded { Task { try? await Parakeet.shared.prepare { _ in } } }
      Task {
        if let why = await listen.allowed() { emit(["error": why]); exit(1) }
        do { try listen.start() } catch { emit(["error": "Couldn't start the microphone: \(error.localizedDescription)"]); exit(1) }
        emit(["listening": true])
        Thread.detachNewThread {
          while let line = readLine(), line.trimmingCharacters(in: .whitespaces) != "stop" {}
          Task { @MainActor in
            emit(["text": await listen.finish(wait: true), "final": true])
            exit(0)
          }
        }
      }
    }
    dispatchMain()
  }
}
