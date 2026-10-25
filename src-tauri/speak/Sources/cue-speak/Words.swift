// When each word of a sentence is heard in the sound Kokoro made of it: Kokoro's own timings (predictedDurations,
// one per token of inputIds) and its normalized text, lined up with the sentence's words (SpeakCore: Timing, Align).
import FluidAudio
import Foundation
import SpeakCore

enum Words {
  /// Punctuation, as Kokoro's phonemes carry it.
  static let marks = ";:,.!?—…\"()“”«»¡¿-"
  /// Kokoro's ids for a space and for punctuation (a group of only punctuation isn't a word), from its vocab.
  static let ids: (space: Int32, marks: Set<Int32>) = {
    let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".cache/fluidaudio/Models/kokoro-82m-coreml/ANE/vocab.json")
    let map = (try? KokoroAneVocab.load(from: url))?.map ?? [:]
    let marks = Set(Words.marks.compactMap { map[$0] })
    return (map[" "] ?? 16, marks)
  }()

  /// The normalized words Kokoro said for `r` (the sentence `text`).
  static func said(_ text: String, _ r: KokoroAneSynthesisResult) -> [String] {
    (r.normalizedText ?? text).split(whereSeparator: \.isWhitespace).map(String.init).filter { $0.contains(where: \.isLetter) || $0.contains(where: \.isNumber) }
  }

  /// How many spoken parts `phonemes` (one word's) has.
  static func parts(_ phonemes: String) -> Int {
    phonemes.split(separator: " ").filter { $0.unicodeScalars.contains { !marks.unicodeScalars.contains($0) } }.count
  }

  /// The ms at which each of `text`'s words (split on whitespace) starts in `r`'s sound, `parts` being how many
  /// spoken parts each of `said(text, r)` is; nil when it can't be told.
  static func starts(_ text: String, _ r: KokoroAneSynthesisResult, parts: [Int]) -> [Int]? {
    let d = r.predictedDurations, total = d.reduce(0) { $0 + Int($1) }
    guard total > 0, d.count == r.inputIds.count else { return nil }
    let groups = Timing.groups(ids: r.inputIds, durations: d, space: ids.space, edge: KokoroAneConstants.bosTokenId, marks: ids.marks)
    let mine = text.split(whereSeparator: \.isWhitespace).map(String.init)
    return Timing.starts(mine: mine, said: said(text, r), parts: parts, groups: groups, msPerFrame: Double(r.samples.count) / 24.0 / Double(total))
  }
}
