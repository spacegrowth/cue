// Your voice settings (Cue's Settings → Voice writes them; talk.rs): whether answers are read out at all, and which
// recognizer writes down what's sent. In Cue's data folder, voice.json. (The voice they're read in comes with each
// answer, from Cue's own setting; `voice` is the Mac voice picked here before that, still used for the Mac's own.)
import Foundation

struct Prefs {
  /// A Mac voice's identifier, picked here before Cue had its own voice setting ("": the best English one).
  var voice = ""
  var read = true
  /// "parakeet" (NVIDIA's, once it's here) or "apple".
  var model = "parakeet"
  /// An answer back is read out at once: "always", "front" (while Cue is the app in front) or "never" (it waits).
  var auto = "never"
  /// Live captions: your words and its answer along the bottom of the screen (off: in the island itself).
  var captions = false
  /// Talking to sessions at all (Settings → Voice): off, right ⇧ and ⌥Space are left alone and nothing is downloaded.
  var talk = false

  static var file: URL {
    FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("dev.spacegrowth.cue/voice.json")
  }

  static func load() -> Prefs {
    var p = Prefs()
    guard let data = try? Data(contentsOf: file), let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return p }
    p.voice = o["voice"] as? String ?? ""
    p.read = o["read"] as? Bool ?? true
    p.model = o["model"] as? String ?? "parakeet"
    p.auto = o["auto"] as? String ?? "never"
    p.captions = o["captions"] as? Bool ?? false
    p.talk = o["talk"] as? Bool ?? false
    return p
  }
}
