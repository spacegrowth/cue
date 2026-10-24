// NVIDIA's Parakeet (v2, English) on the Mac, through FluidAudio: what's sent is its text once
// it's here. Downloaded once (about 450 MB, from Hugging Face) into Application Support/FluidAudio/Models, then
// loaded into memory. Nothing you say leaves the Mac.
import FluidAudio
import Foundation

@MainActor
final class Parakeet {
  static let shared = Parakeet()
  nonisolated private static let version: AsrModelVersion = .v2

  private(set) var manager: AsrManager?
  private var loading: Task<Void, Error>?
  var ready: Bool { manager != nil }

  nonisolated static var downloaded: Bool {
    AsrModels.modelsExist(at: AsrModels.defaultCacheDirectory(for: version), version: version)
  }

  /// Download it if it isn't here, then load it; `progress` gets 0–1 along the way (on any thread). A second
  /// call while the first is under way joins it.
  func prepare(progress: @escaping @Sendable (Double) -> Void) async throws {
    if manager != nil { return }
    if let loading { return try await loading.value }
    let task = Task {
      let models = try await AsrModels.downloadAndLoad(version: Self.version, progressHandler: { progress($0.fractionCompleted) })
      let m = AsrManager(config: .default)
      try await m.loadModels(models)
      self.manager = m
    }
    loading = task
    defer { loading = nil }
    try await task.value
  }

  /// What was said in `samples` (mono, at `rate` Hz). Fails when it isn't loaded.
  func transcribe(_ samples: [Float], rate: Double) async throws -> String {
    guard let m = manager else { throw NSError(domain: "Parakeet", code: 1, userInfo: [NSLocalizedDescriptionKey: "the listening model isn't loaded"]) }
    let audio = try AudioConverter().resample(samples, from: rate)
    var state = TdtDecoderState.make(decoderLayers: await m.decoderLayerCount)
    return try await m.transcribe(audio, decoderState: &state).text
  }
}
