// When each word of a sentence is heard in the sound Kokoro made of it. Kokoro gives how long each sound lasts
// (predictedDurations, one per token of its phonemes); a space token ends a spoken part. A word of its normalized
// text ("1.4" → "one point four") can be several parts ("FBI" is F, B, I), so `parts` says how many each one is
// (its phonemes on their own). The normalized words are then lined up with the sentence's own: a word that reads
// the same takes one, one that was spelled out takes all of them up to the next that reads the same.
// Checked against NVIDIA Parakeet's word times on the same sound (scratch tool, 18 Oct 2026).
import FluidAudio
import Foundation

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
    let msPerFrame = Double(r.samples.count) / 24.0 / Double(total)
    // Kokoro's words: where each starts, in frames.
    var groups: [Int] = [], at = 0, start: Int?, spoken = false
    for (id, n) in zip(r.inputIds, d) {
      if id == KokoroAneConstants.bosTokenId { at += Int(n); continue }
      if id == ids.space {
        if let s = start, spoken { groups.append(s) }
        start = nil; spoken = false
      } else {
        if start == nil { start = at }
        if !ids.marks.contains(id) { spoken = true }
      }
      at += Int(n)
    }
    if let s = start, spoken { groups.append(s) }
    let said = Words.said(text, r)
    let mine = text.split(whereSeparator: \.isWhitespace).map(String.init)
    guard !groups.isEmpty, !said.isEmpty, !mine.isEmpty else { return nil }
    // The said word `j` starts at its first part: counted from `parts` when they add up to what was heard, else
    // the part as far along.
    var first: [Int] = []
    if parts.count == said.count && parts.reduce(0, +) == groups.count {
      var g = 0
      for n in parts { first.append(min(g, groups.count - 1)); g += n }
    } else {
      first = said.indices.map { min(groups.count - 1, $0 * groups.count / said.count) }
    }
    let frameOf = { (j: Int) -> Int in groups[first[j]] }
    // Each of the sentence's words to the first said word it is (or that spells it out).
    let key = { (w: String) in w.lowercased().filter { $0.isLetter || $0.isNumber } }
    var out: [Int] = [], j = 0
    for (i, w) in mine.enumerated() {
      out.append(Int(Double(frameOf(min(j, said.count - 1))) * msPerFrame))
      guard j < said.count else { continue }
      if key(w) == key(said[j]) || key(w).isEmpty { if !key(w).isEmpty { j += 1 }; continue }
      // Spelled out: on to the said word the next of the sentence's words matches.
      let next = i + 1 < mine.count ? key(mine[i + 1]) : ""
      var k = j + 1
      while k < said.count && (next.isEmpty || key(said[k]) != next) { k += 1 }
      j = next.isEmpty ? said.count : max(j + 1, k)
    }
    return out
  }
}
