// When each word of a sentence is heard in the sound Kokoro made of it. Kokoro gives how long each sound lasts
// (one count of frames per token of its phonemes); a space token ends a spoken part. A word of its normalized text
// ("1.4" → "one point four") can be several parts ("FBI" is F, B, I), so `parts` says how many each one is (its
// phonemes on their own). The normalized words are then lined up with the sentence's own (Align).
// Checked against NVIDIA Parakeet's word times on the same sound (scratch tool, 18 Oct 2026).
import Foundation

public enum Timing {
  /// Where each spoken part starts, in frames: `ids` are the tokens Kokoro was given, `durations` how many frames
  /// each lasts. A space ends a part, and so does `edge`, the token Kokoro's text starts and ends with: a long
  /// text is made in pieces, each with its own pair and no space between them. A part of only punctuation isn't
  /// one (`marks`).
  public static func groups(ids: [Int32], durations: [Int32], space: Int32, edge: Int32, marks: Set<Int32>) -> [Int] {
    var out: [Int] = [], at = 0, start: Int?, spoken = false
    for (id, n) in zip(ids, durations) {
      if id == space || id == edge {
        if let s = start, spoken { out.append(s) }
        start = nil; spoken = false
      } else {
        if start == nil { start = at }
        if !marks.contains(id) { spoken = true }
      }
      at += Int(n)
    }
    if let s = start, spoken { out.append(s) }
    return out
  }

  /// The ms at which each of `mine` (a sentence's words) starts: `said` are Kokoro's words for it, `parts` how many
  /// spoken parts each of those is, `groups` where every part starts in frames; nil when it can't be told.
  public static func starts(mine: [String], said: [String], parts: [Int], groups: [Int], msPerFrame: Double) -> [Int]? {
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
    // Each of mine at its first said word; one said as nothing (a dash) where the next starts.
    return Align.runs(mine: mine, said: said).map { Int(Double(groups[first[min($0.lowerBound, said.count - 1)]]) * msPerFrame) }
  }
}
