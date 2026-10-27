// The pure parts of cue-voice, kept apart so they can be tested (`swift test`): where an answer goes by what you're
// doing, what's read out of an answer, how the captions line cuts and marks it, and how long a turn took.
import Foundation

/// What cue-voice is doing.
public enum VoicePhase: String, CaseIterable {
  case idle, listening, paused, sending, working, answered, saying, said

  /// An answer that comes now is put up at once; while you're talking, reading one, or one waits for you, it waits
  /// its turn (never dropped).
  public var takesAnswerNow: Bool { [.sending, .working, .idle, .said].contains(self) }
}

public enum VoiceText {
  /// An answer as it's read out: no code blocks, links said as "the link", no markdown marks.
  public static func plain(_ text: String) -> String {
    var s = text
    let rules: [(String, String)] = [
      ("```[\\s\\S]*?```", " "),                  // code blocks: not read
      ("\\[([^\\]]+)\\]\\([^)]+\\)", "$1"),       // [words](url): the words
      ("https?://\\S+", "the link"),
      ("`([^`]*)`", "$1"),
      ("[*_#>]+", ""),
    ]
    for (pattern, with) in rules { s = s.replacingOccurrences(of: pattern, with: with, options: .regularExpression) }
    return s.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  /// What was heard is only "yeah": the recognizer makes that out of noise near silence, so it's not sent.
  public static func strayYeah(_ said: String) -> Bool {
    said.lowercased().filter { $0.isLetter } == "yeah"
  }

  /// An answer as it's read out, with whose it is first when that's needed to know: it's from another session
  /// than the one read last, or more are waiting after it. `name` is the island's ("cue · voice"): its session's
  /// name is said (its project's, when the session has none of its own).
  public static func spoken(_ answer: String, name: String, sid: String, lastSid: String?, more: Int) -> String {
    let session = name.components(separatedBy: " · ").last ?? name
    guard !session.isEmpty, more > 0 || (lastSid != nil && lastSid != sid) else { return answer }
    return "From \(session). \(answer)"
  }

  /// How many more answers wait after this one, as the island's label ends: " · 2 MORE", or nothing.
  public static func more(_ n: Int) -> String { n > 0 ? " · \(n) MORE" : "" }

  /// How long a turn took, as the island says it: 40S, 4M, 1H 51M.
  public static func took(_ seconds: TimeInterval) -> String {
    let s = Int(seconds)
    if s < 60 { return "\(s)S" }
    if s < 3600 { return "\(s / 60)M" }
    return "\(s / 3600)H \(s % 3600 / 60)M"
  }

  /// How far into `plain` it has been read, at `at` (0–1) of it by length: to the end of the word it's on (all of
  /// it at 1). For when the word being heard isn't known (`wordEnds`).
  public static func readTo(_ plain: String, at: Double) -> Int {
    if at >= 1 { return plain.count }
    let cut = plain.index(plain.startIndex, offsetBy: min(plain.count, max(0, Int((at * Double(plain.count)).rounded(.up)))))
    let end = plain[cut...].firstIndex(where: \.isWhitespace) ?? plain.endIndex
    return plain.distance(from: plain.startIndex, to: end)
  }

  /// Where each of `text`'s words ends (split on whitespace, as cue-speak splits them): the character after it. So
  /// the word heard is marked whole, by its own place, not by a fraction of the length (a one-letter word's fell
  /// over on the next).
  public static func wordEnds(_ text: String) -> [Int] {
    var out: [Int] = [], i = 0, inWord = false
    for ch in text {
      if ch.isWhitespace { if inWord { out.append(i) }; inWord = false } else { inWord = true }
      i += 1
    }
    if inWord { out.append(i) }
    return out
  }

  /// `plain` as the captions line shows it: its line breaks as spaces (one each, so a place in it is the same).
  public static func oneLine(_ plain: String) -> String { String(plain.map { $0.isNewline ? " " : $0 }) }

  /// The captions line in three: what's been said, the word being said (yours: the newest), and what's to come.
  public static func captionParts(_ line: String, upTo: Int, yours: Bool) -> (said: String, word: String, ahead: String) {
    let end = yours ? line.endIndex : line.index(line.startIndex, offsetBy: min(max(0, upTo), line.count))
    let start = line[..<end].lastIndex(where: \.isWhitespace).map { line.index(after: $0) } ?? line.startIndex
    return (String(line[..<start]), String(line[start..<end]), String(line[end...]))
  }

  /// How much of what's been said is left off the front of the captions line, and how much of what's to come is
  /// drawn: only the stretch round the word. macOS draws nothing of a line wider than about 4,000 points (8,192
  /// pixels on a Retina screen): it went blank two sentences in. 160 to 239 characters said (about 2,600 points at
  /// most, more than the two thirds of a wide screen left of the word) and 80 to come (the third right of it) stay
  /// under that. Cut in steps of 80, so the cut moves seldom.
  public static func captionCut(saidCount: Int) -> Int { max(0, (saidCount / 80 - 2) * 80) }
  public static let captionAhead = 80

  /// The captions line jumps (rather than glides) to a new place: a new line (far off), or back (the words got
  /// fewer: you went back, or a new line started).
  public static func captionJumps(from x: Double, to target: Double) -> Bool { abs(target - x) > 600 || target > x + 40 }
}

/// A voice conversation's bookkeeping: a turn under way (what you said went, its answer isn't back), by the way
/// questions still being asked, and answers that came while you were busy with another. Answers are never
/// dropped: each is either put up at once or queued, and `next()` says what comes after you're done with one.
public struct VoiceTurns<Answer> {
  public private(set) var turnOn = false
  public private(set) var asking = 0
  public private(set) var pending: [Answer] = []
  public init() {}

  /// What you said went (or a turn under way was found again after a restart).
  public mutating func sent() { turnOn = true }
  /// The turn ended without an answer (stopped, or its session went).
  public mutating func stopped() { turnOn = false }
  public mutating func askedByTheWay() { asking += 1 }
  public mutating func byTheWayFailed() { asking = max(0, asking - 1) }

  /// An answer came: true when it's put up now, false when it's queued. A by the way's answer ends that question;
  /// a turn's answer ends the turn (a followed session's later reply ends nothing).
  public mutating func answered(_ a: Answer, byTheWay: Bool, follow: Bool, phase: VoicePhase) -> Bool {
    if byTheWay { asking = max(0, asking - 1) } else if !follow { turnOn = false }
    if phase.takesAnswerNow { return true }
    pending.append(a)
    return false
  }

  public enum Next { case present(Answer), work, idle }
  /// Done with an answer: the next one waiting, else back to the turn (or by the way) still under way, else nothing.
  public mutating func next() -> Next {
    if !pending.isEmpty { return .present(pending.removeFirst()) }
    if turnOn || asking > 0 { return .work }
    return .idle
  }
}
