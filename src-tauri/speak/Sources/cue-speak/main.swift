// cue-speak: Listen's natural voices. Kokoro (an open voice model, Apache 2.0) through FluidAudio, on this Mac:
// nothing it reads leaves it. Each voice comes from Hugging Face when you ask for it (Settings → Voice), the first
// with the voice engine they all use (about 125 MB; a voice is 0.5 MB), into FluidAudio's cache
// (~/.cache/fluidaudio/Models).
//
// Cue starts it while you listen (or download) and they talk in JSON lines, one per line each way:
//   {"status": true}                                → {"status": {"ready", "risky", "have": [voice], "mb"}}
//   {"get": voice}                                  → {"progress": 0–1} as it comes, then {"got": voice}
//   {"remove": true}                                → all of it: {"removed": true}
//   {"say": text, "voice", "speed", "id", "next"?}  → {"playing": id, "ms": how long, "words": [ms each of the text's
//                                                     words starts at] (Words.swift)}, {"at": id, "ms": how far} as
//                                                     it plays (what you hear: the output's delay counted), then
//                                                     {"done": id, "end": true}
//   {"pause": true} / {"resume": true}              → what's playing holds where it is / goes on from there
//   {"stop": true}                                  → what's playing ends: {"done": id, "end": false}
// "next" is the sentence after, made while this one plays so there's no gap between them. Problems come out as
// {"error": "…", "id"?}.
//
// It quits on its own after 100 sentences ({"bye": true}), and Cue starts a fresh one when it next needs it: on
// macOS 26.4 and 26.5 an Apple bug breaks Kokoro after about 150 sentences in one process (FluidAudio #667).
import AVFoundation
import FluidAudio
import Foundation

func emit(_ obj: [String: Any]) {
  guard let data = try? JSONSerialization.data(withJSONObject: obj), let line = String(data: data, encoding: .utf8) else { return }
  FileHandle.standardOutput.write((line + "\n").data(using: .utf8)!)
}

/// FluidAudio's cache: Kokoro's models with the voices (ANE/<voice>.bin), and its pronouncer.
let cacheDirs = ["kokoro-82m-coreml", "kokoro"].map {
  FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".cache/fluidaudio/Models/\($0)")
}
let voiceDir = cacheDirs[0].appendingPathComponent("ANE")

/// The voice engine is here: Kokoro's models and vocab, and its pronouncer (whoever downloaded them).
func engineHere() -> Bool {
  let fm = FileManager.default
  let models = ["KokoroAlbert", "KokoroAlignment", "KokoroNoise_v2", "KokoroPostAlbert", "KokoroProsody_v2", "KokoroTail_v2", "KokoroVocoder"]
  return fm.fileExists(atPath: voiceDir.appendingPathComponent("vocab.json").path)
    && models.allSatisfy { fm.fileExists(atPath: voiceDir.appendingPathComponent("\($0).mlmodelc").path) }
    && !((try? fm.contentsOfDirectory(atPath: cacheDirs[1].path)) ?? []).isEmpty
}

/// The voices here ("af_bella"…), and how much it all takes, in MB.
func onDisk() -> (have: [String], mb: Int) {
  let fm = FileManager.default
  let have = ((try? fm.contentsOfDirectory(atPath: voiceDir.path)) ?? []).filter { $0.hasSuffix(".bin") && $0.contains("_") }.map { String($0.dropLast(4)) }.sorted()
  var bytes = 0
  for d in cacheDirs {
    let e = fm.enumerator(at: d, includingPropertiesForKeys: [.fileSizeKey])
    while let u = e?.nextObject() as? URL { bytes += (try? u.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0 }
  }
  return (have, Int((Double(bytes) / 1_048_576).rounded()))
}
/// The macOS versions with the Apple bug that can stop Kokoro (FluidAudio's own check).
let risky: Bool = {
  let v = ProcessInfo.processInfo.operatingSystemVersion
  return v.majorVersion == 26 && (4...5).contains(v.minorVersion)
}()

@MainActor
final class Speaker {
  private let kokoro = KokoroAneManager()
  private var loaded = false
  private let engine = AVAudioEngine()
  private let player = AVAudioPlayerNode()
  private let format = AVAudioFormat(standardFormatWithSampleRate: 24_000, channels: 1)!
  /// Sentences made or being made ("voice|speed|text"), the newest few: the next one is made while this one
  /// plays, and its say waits for that rather than making it again.
  private var made: [(key: String, task: Task<Made, Error>)] = []
  /// The sentence playing, and whether it was stopped.
  private var playing: Int?
  private var stopped = false
  private var said = 0
  private var ticking: DispatchSourceTimer?

  init() {
    engine.attach(player)
    engine.connect(player, to: engine.mainMixerNode, format: format)
  }

  func heard(_ o: [String: Any]) async {
    if o["status"] != nil {
      let d = onDisk()
      emit(["status": ["ready": engineHere(), "risky": risky, "have": engineHere() ? d.have : [], "mb": d.mb]])
    } else if let voice = o["get"] as? String {
      await get(voice)
    } else if o["remove"] != nil {
      stop()
      for d in cacheDirs { try? FileManager.default.removeItem(at: d) }
      emit(["removed": true])
    } else if o["stop"] != nil {
      stop()
    } else if o["pause"] != nil {
      if playing != nil { player.pause() }
    } else if o["resume"] != nil {
      if playing != nil { player.play() }
    } else if let text = o["say"] as? String {
      await say(text, voice: o["voice"] as? String ?? "af_heart", speed: Float(o["speed"] as? Double ?? 1), id: o["id"] as? Int ?? 0, next: o["next"] as? String)
    }
  }

  /// A voice: the first time, the engine with it (the model and the pronouncer, with how far along; the model is
  /// most of it); after that, just its 0.5 MB.
  private func get(_ voice: String) async {
    do {
      if !engineHere() {
        try await KokoroAneResourceDownloader.ensureModels(progressHandler: { p in emit(["progress": p.fractionCompleted * 0.7]) })
        try await KokoroAneResourceDownloader.ensureG2PAssets(progressHandler: { p in emit(["progress": 0.7 + p.fractionCompleted * 0.28]) })
        try await load()
      }
      try await KokoroAneResourceDownloader.ensureVoicePack(voice, repoDirectory: voiceDir)
      emit(["got": voice])
    } catch {
      emit(["error": "Couldn't get that voice: \(error.localizedDescription)", "get": voice])
    }
  }

  private func load() async throws {
    if loaded { return }
    try await kokoro.initialize(preloadVoices: ["af_heart"])
    loaded = true
  }

  /// A sentence's sound, with the silence Kokoro puts around it cut to a short pause (it starts every sentence
  /// with about 300 ms of it), and when each of its words starts.
  struct Made { let samples: [Float]; let words: [Int]? }

  private func made(_ text: String, voice: String, speed: Float) -> Task<Made, Error> {
    let key = "\(voice)|\(speed)|\(text)"
    if let hit = made.first(where: { $0.key == key }) { return hit.task }
    let task = Task { () throws -> Made in
      try await self.load()
      let r = try await self.kokoro.synthesizeDetailed(text: text, voice: voice, speed: speed)
      var parts: [Int] = []
      for w in Words.said(text, r) { parts.append(Words.parts((try? await self.kokoro.englishPhonemes(for: w)) ?? w)) }
      let (s, cut) = Speaker.trimmed(r.samples)
      return Made(samples: s, words: Words.starts(text, r, parts: parts)?.map { max(0, $0 - cut) })
    }
    made.append((key, task))
    if made.count > 3 { made.removeFirst() }
    return task
  }

  /// `s` with its silence before and after cut to 50 ms and 120 ms, and how many ms went from its start.
  static func trimmed(_ s: [Float]) -> ([Float], Int) {
    let win = 240   // 10 ms
    let loud = { (i: Int) -> Bool in
      let end = min(s.count, i + win)
      return sqrt(s[i..<end].reduce(0) { $0 + $1 * $1 } / Float(max(1, end - i))) > 0.01
    }
    guard let first = stride(from: 0, to: s.count, by: win).first(where: loud),
          let last = stride(from: s.count - 1, through: 0, by: -win).first(where: { loud(max(0, $0 - win + 1)) }) else { return (s, 0) }
    let a = max(0, first - 1_200), b = min(s.count, last + 2_880)
    return (Array(s[a..<b]), a / 24)
  }

  private func say(_ text: String, voice: String, speed: Float, id: Int, next: String?) async {
    stop()
    let m: Made
    do { m = try await made(text, voice: voice, speed: speed).value } catch {
      emit(["error": "Kokoro couldn't say it: \(error.localizedDescription)", "id": id])
      return
    }
    let s = m.samples
    guard let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(max(1, s.count))) else { return }
    buf.frameLength = AVAudioFrameCount(s.count)
    s.withUnsafeBufferPointer { src in buf.floatChannelData![0].update(from: src.baseAddress!, count: s.count) }
    do { if !engine.isRunning { try engine.start() } } catch {
      emit(["error": "Couldn't play it: \(error.localizedDescription)", "id": id])
      return
    }
    playing = id
    stopped = false
    said += 1
    // Its clock from zero: the player's time runs on after a sentence ends, so without this the next one would
    // seem over at once.
    player.stop()
    player.scheduleBuffer(buf, completionHandler: nil)
    player.play()
    emit(["playing": id, "ms": Int(Double(s.count) / 24.0), "words": m.words ?? []])
    tick(id, total: Double(s.count) / 24.0)
    // The next one, made while this one plays.
    if let next, !next.isEmpty { _ = made(next, voice: voice, speed: speed) }
  }

  /// Where what you hear is, every 80 ms: the player's position less the output's delay. It's done when that
  /// reaches its end: the player's own "played back" comes seconds late on Bluetooth.
  private func tick(_ id: Int, total: Double) {
    ticking?.cancel()
    let lag = engine.outputNode.presentationLatency
    let t = DispatchSource.makeTimerSource(flags: .strict, queue: .main)
    t.schedule(deadline: .now(), repeating: .milliseconds(80), leeway: .milliseconds(5))
    t.setEventHandler {
      MainActor.assumeIsolated {
        guard self.playing == id, let now = self.player.lastRenderTime, let at = self.player.playerTime(forNodeTime: now) else { return }
        // Done when it has played it all, once the output's delay has gone by.
        let played = Double(at.sampleTime) / at.sampleRate * 1000
        if played >= total {
          t.cancel()
          DispatchQueue.main.asyncAfter(deadline: .now() + lag) { MainActor.assumeIsolated { self.ended(id) } }
        } else if played - lag * 1000 > 0 { emit(["at": id, "ms": Int(played - lag * 1000)]) }
      }
    }
    t.resume()
    ticking = t
  }

  private func ended(_ id: Int) {
    guard playing == id else { return }
    playing = nil
    ticking?.cancel()
    emit(["done": id, "end": !stopped])
    if said >= 100 && !stopped { emit(["bye": true]); exit(0) }
  }

  private func stop() {
    guard playing != nil else { return }
    stopped = true
    player.stop()
    if let id = playing { ended(id) }
  }
}

// It runs in the background, where macOS would let its timers slip by seconds (App Nap): the marks follow the
// voice, so it says its timing matters.
let activity = ProcessInfo.processInfo.beginActivity(options: [.userInitiated, .latencyCritical], reason: "Reading a reply out")
let speaker = MainActor.assumeIsolated { Speaker() }
// Cue's lines, in order. Its stdin closing (Cue quit, or Listen stopped) ends this too.
Thread.detachNewThread {
  while let line = readLine() {
    guard let data = line.data(using: .utf8), let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { continue }
    // One at a time, in order (a say until its sound is made and playing, so a stop after it stops it); getting a
    // voice goes on by itself.
    let sem = DispatchSemaphore(value: 0)
    Task { @MainActor in
      await speaker.heard(o)
      sem.signal()
    }
    if o["get"] == nil { sem.wait() }
  }
  DispatchQueue.main.async { exit(0) }
}
dispatchMain()
