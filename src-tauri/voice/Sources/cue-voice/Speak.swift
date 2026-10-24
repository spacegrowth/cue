// Reading an answer out, in the voice picked in Cue's Settings → Voice. A natural one (Kokoro) is read by
// cue-speak, Cue's own helper next to this program, kept running while answers come; if it can't (not there, or it
// fails), the Mac's voice reads it instead. The Mac's own: the one picked here before (Prefs), else the best English
// one on this Mac (a Premium or Enhanced one if you've downloaded one in System Settings → Accessibility → Spoken
// Content). Markdown is read as words: a link is "the link".
import AVFoundation
import VoiceCore

@MainActor
final class Speak: NSObject, AVSpeechSynthesizerDelegate {
  private let synth = AVSpeechSynthesizer()
  /// How far through it is (0–1), and that it's done (`true`: to the end; `false`: stopped).
  var onProgress: (Double) -> Void = { _ in }
  var onDone: (Bool) -> Void = { _ in }
  private var length = 1
  /// How long what's being read takes, in seconds: the natural voice's once its sound is made, the Mac's guessed
  /// from its length (about 14 characters a second). With how far it is, how long is left.
  private(set) var total = 0.0
  var on: Bool { synth.isSpeaking || natural != nil }
  /// cue-speak while it runs, the answer it's reading (its id, words and length in ms), and its output so far.
  private var helper: Process?
  private var helperIn: FileHandle?
  private var natural: (id: Int, words: String, ms: Double)?
  /// Where each of the answer's words (split on spaces) starts, in ms (cue-speak's, from Kokoro's own timings),
  /// and where it starts in the text: so the word marked is the one heard, not a guess by length.
  private var starts: [(ms: Double, at: Int)] = []
  private var sent = 0
  private var buffer = Data()

  override init() {
    super.init()
    synth.delegate = self
  }

  /// `text` read out in `voice` ("" the Mac's own, else a natural one: "af_heart").
  func say(_ text: String, voice: String) {
    stop()
    let words = Speak.plain(text)
    length = max(1, words.utf16.count)
    total = Double(length) / 14
    if !voice.isEmpty, let to = running() {
      sent += 1
      natural = (sent, words, 0)
      starts = []
      if let d = try? JSONSerialization.data(withJSONObject: ["say": words, "voice": voice, "speed": 1, "id": sent]) {
        to.write(d + Data("\n".utf8))
        return
      }
      natural = nil
    }
    apple(words)
  }

  private func apple(_ words: String) {
    let u = AVSpeechUtterance(string: words)
    u.voice = Speak.voice(Prefs.load().voice)
    synth.speak(u)
  }

  /// Hold where it is, and go on from there.
  func pause() {
    if synth.isSpeaking { synth.pauseSpeaking(at: .word) }
    if natural != nil, let d = try? JSONSerialization.data(withJSONObject: ["pause": true]) { helperIn?.write(d + Data("\n".utf8)) }
  }
  func resume() {
    if synth.isPaused { synth.continueSpeaking() }
    if natural != nil, let d = try? JSONSerialization.data(withJSONObject: ["resume": true]) { helperIn?.write(d + Data("\n".utf8)) }
  }

  func stop() {
    if synth.isSpeaking { synth.stopSpeaking(at: .immediate) }
    if natural != nil, let d = try? JSONSerialization.data(withJSONObject: ["stop": true]) { helperIn?.write(d + Data("\n".utf8)) }
  }

  // MARK: cue-speak

  /// cue-speak's stdin, starting it if it isn't running (nil: it isn't next to this program).
  private func running() -> FileHandle? {
    if let h = helper, h.isRunning { return helperIn }
    let exe = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath().deletingLastPathComponent().appendingPathComponent("cue-speak")
    guard FileManager.default.isExecutableFile(atPath: exe.path) else { return nil }
    let p = Process(), stdin = Pipe(), stdout = Pipe()
    p.executableURL = exe
    p.standardInput = stdin
    p.standardOutput = stdout
    p.standardError = FileHandle.nullDevice
    stdout.fileHandleForReading.readabilityHandler = { h in
      let d = h.availableData
      DispatchQueue.main.async { MainActor.assumeIsolated { self.output(d) } }
    }
    // It quit (every 100 sentences) or crashed: an answer it was reading goes on in the Mac's voice.
    p.terminationHandler = { _ in DispatchQueue.main.async { MainActor.assumeIsolated { self.gone() } } }
    do { try p.run() } catch { return nil }
    helper = p
    helperIn = stdin.fileHandleForWriting
    return helperIn
  }

  private func output(_ d: Data) {
    buffer.append(d)
    while let nl = buffer.firstIndex(of: 0x0A) {
      let line = buffer[buffer.startIndex..<nl]
      buffer.removeSubrange(buffer.startIndex...nl)
      guard let o = try? JSONSerialization.jsonObject(with: line) as? [String: Any], let n = natural else { continue }
      if o["playing"] as? Int == n.id, let ms = o["ms"] as? Double {
        natural?.ms = ms
        total = ms / 1000
        starts = Speak.starts(n.words, o["words"] as? [Double] ?? [])
      } else if o["at"] as? Int == n.id, let ms = o["ms"] as? Double, n.ms > 0 {
        // On the word being heard (just inside it, so it's marked whole); by length when the times don't fit.
        if let w = starts.last(where: { $0.ms <= ms }) { onProgress(min(1, Double(w.at + 1) / Double(max(1, n.words.count)))) }
        else { onProgress(min(1, ms / n.ms)) }
      }
      else if o["done"] as? Int == n.id { natural = nil; onDone(o["end"] as? Bool ?? false) }
      else if o["error"] != nil, o["id"] as? Int == n.id { natural = nil; note("natural voice: \(o["error"] ?? "")"); apple(n.words) }
    }
  }

  private func gone() {
    helper = nil
    helperIn = nil
    buffer = Data()
    if let n = natural { natural = nil; note("natural voice stopped"); apple(n.words) }
  }

  nonisolated func speechSynthesizer(_ s: AVSpeechSynthesizer, willSpeakRangeOfSpeechString r: NSRange, utterance: AVSpeechUtterance) {
    let at = Double(r.location + r.length)
    DispatchQueue.main.async { MainActor.assumeIsolated { self.onProgress(min(1, at / Double(self.length))) } }
  }
  nonisolated func speechSynthesizer(_ s: AVSpeechSynthesizer, didFinish u: AVSpeechUtterance) {
    DispatchQueue.main.async { MainActor.assumeIsolated { self.onDone(true) } }
  }
  nonisolated func speechSynthesizer(_ s: AVSpeechSynthesizer, didCancel u: AVSpeechUtterance) {
    DispatchQueue.main.async { MainActor.assumeIsolated { self.onDone(false) } }
  }

  /// The one you picked, else English in your region if there is one, the best quality first; never a novelty voice.
  static func voice(_ picked: String) -> AVSpeechSynthesisVoice? {
    if !picked.isEmpty, let v = AVSpeechSynthesisVoice(identifier: picked) { return v }
    return best
  }
  private static let best: AVSpeechSynthesisVoice? = {
    let lang = Locale.current.identifier.replacingOccurrences(of: "_", with: "-")
    let all = AVSpeechSynthesisVoice.speechVoices().filter { $0.language.hasPrefix("en") && !$0.voiceTraits.contains(.isNoveltyVoice) }
    func rank(_ v: AVSpeechSynthesisVoice) -> Int { (v.quality == .premium ? 0 : v.quality == .enhanced ? 2 : 4) + (v.language == lang ? 0 : 1) }
    return all.min { rank($0) < rank($1) } ?? AVSpeechSynthesisVoice(language: "en-US")
  }()

  /// `times` (ms) paired with where each of `text`'s words starts in it; none when they don't match one to one.
  static func starts(_ text: String, _ times: [Double]) -> [(ms: Double, at: Int)] {
    var at: [Int] = [], i = 0, inWord = false
    for ch in text {
      if ch.isWhitespace { inWord = false } else if !inWord { inWord = true; at.append(i) }
      i += 1
    }
    return at.count == times.count ? Array(zip(times, at)).map { (ms: $0.0, at: $0.1) } : []
  }

  /// What to say for `text` written in Markdown: its words, not its marks.
  static func plain(_ text: String) -> String { VoiceText.plain(text) }
}
