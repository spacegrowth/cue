// The microphone, while you hold the key. Apple's recognizer (on this Mac only) writes your words down as you go,
// for the pill; the sound is kept too, and what's sent is Parakeet's text of it once Parakeet is ready (Apple's
// final words until then). The mic is on only while you hold.
import AVFoundation
import Foundation
import Speech

@MainActor
final class Listen {
  /// Your words so far (Apple's), and how loud you are (0–1), as they change.
  var onWords: (String) -> Void = { _ in }
  var onLevel: (Float) -> Void = { _ in }

  private var engine: AVAudioEngine?
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var words = ""
  private var final: CheckedContinuation<Void, Never>?
  /// The sound heard (mono, at `rate`), filled on the audio thread.
  private let kept = Kept()
  private var rate = 0.0

  var on: Bool { engine != nil }

  /// Ask for the microphone and speech recognition, once; nil when allowed, else why not.
  func allowed() async -> String? {
    let speech = await withCheckedContinuation { c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) } }
    // Without Apple's recognizer it still works (Parakeet alone), only without the words as you go.
    _ = speech
    let mic = await AVCaptureDevice.requestAccess(for: .audio)
    return mic ? nil : "Cue isn't allowed to use the microphone. Turn it on in System Settings → Privacy & Security → Microphone."
  }

  func start() throws {
    let engine = AVAudioEngine()
    let input = engine.inputNode
    let format = input.outputFormat(forBus: 0)
    guard format.sampleRate > 0, format.channelCount > 0 else { throw NSError(domain: "Listen", code: 1, userInfo: [NSLocalizedDescriptionKey: "No microphone found."]) }
    rate = format.sampleRate
    kept.reset()
    words = ""
    // Apple's recognizer, only where it stays on this Mac.
    let recognizer = SFSpeechRecognizer(locale: Locale.current) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))
    var request: SFSpeechAudioBufferRecognitionRequest?
    if let recognizer, recognizer.isAvailable, recognizer.supportsOnDeviceRecognition, SFSpeechRecognizer.authorizationStatus() == .authorized {
      let r = SFSpeechAudioBufferRecognitionRequest()
      r.shouldReportPartialResults = true
      r.addsPunctuation = true
      r.requiresOnDeviceRecognition = true
      request = r
      task = recognizer.recognitionTask(with: r) { [weak self] result, _ in
        let text = result?.bestTranscription.formattedString
        let done = result?.isFinal ?? true
        DispatchQueue.main.async {
          MainActor.assumeIsolated {
            guard let self else { return }
            if let text { self.words = text; self.onWords(text) }
            if done { self.final?.resume(); self.final = nil }
          }
        }
      }
    }
    self.request = request
    let kept = kept
    let level = onLevel
    input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
      request?.append(buffer)
      guard let ch = buffer.floatChannelData?[0] else { return }
      let n = Int(buffer.frameLength)
      let samples = Array(UnsafeBufferPointer(start: ch, count: n))
      kept.add(samples)
      let rms = sqrt(samples.reduce(0) { $0 + $1 * $1 } / Float(max(n, 1)))
      let loud = min(1, max(0, (20 * log10(max(rms, 1e-6)) + 50) / 40))   // -50 dB … -10 dB
      DispatchQueue.main.async { level(loud) }
    }
    engine.prepare()
    try engine.start()
    self.engine = engine
  }

  /// Stop listening; what you said. Parakeet's text when it's picked and ready (`wait`: load it first if it
  /// isn't loaded yet), else Apple's.
  func finish(wait: Bool = false) async -> String {
    stopMic()
    request?.endAudio()
    let parakeet = Prefs.load().model != "apple"
    if parakeet, wait, !Parakeet.shared.ready, Parakeet.downloaded { try? await Parakeet.shared.prepare { _ in } }
    if parakeet, Parakeet.shared.ready {
      task?.cancel()
      let text = try? await Parakeet.shared.transcribe(kept.all(), rate: rate)
      if let text, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return text }
      return words
    }
    // Apple's final words: they land a moment after the sound stops (don't wait long if they don't).
    if task != nil {
      await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
        final = c
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { MainActor.assumeIsolated { self.final?.resume(); self.final = nil } }
      }
    }
    task = nil
    return words
  }

  /// Stop listening and drop it.
  func cancel() {
    stopMic()
    task?.cancel()
    task = nil
    request = nil
  }

  private func stopMic() {
    guard let engine else { return }
    engine.stop()
    engine.inputNode.removeTap(onBus: 0)
    self.engine = nil
    onLevel(0)
  }
}

/// The sound heard so far, added to on the audio thread and read on the main one.
final class Kept: @unchecked Sendable {
  private let lock = NSLock()
  private var samples: [Float] = []
  func reset() { lock.lock(); samples = []; lock.unlock() }
  func add(_ s: [Float]) { lock.lock(); samples += s; lock.unlock() }
  func all() -> [Float] { lock.lock(); defer { lock.unlock() }; return samples }
}
