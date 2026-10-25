// Which of the words Kokoro said each word of a sentence is. Kokoro reads its normalized text ("25 Oct at 9:30" is
// "twenty five October at nine thirty"), so a word of the sentence can be one said word, several ("25" is two,
// "FBI" three) or none (a dash). The two are lined up as a whole, by the cheapest way of giving every said word, in
// order, to one of the sentence's words: a word reads the same as its run (nothing), it's an abbreviation of it
// ("Nov", "Dr."), it has digits and its run is number words with the same letters left over ("120ms" is "one
// hundred and twenty MS"), or they just differ, by how much. Lining up word by word went wrong for good as soon as
// one word was missed: every word after it was off by one.
import Foundation

public enum Align {
  /// The said words each of `mine` is, in order: a run of `said` (empty when none is, its start where it would be).
  public static func runs(mine: [String], said: [String]) -> [Range<Int>] {
    let m = mine.count, s = said.count
    guard m > 0 else { return [] }
    let a = mine.map(key), b = said.map { Word(key: key($0), number: numberish($0)) }
    // A word takes at most this many: a number a long one ("1,234,567,890" is thirteen words) or a short sentence's
    // lot, any other a few ("FBI" is three, "e.g." two).
    let most = max(16, s - m + 1)
    // best[i][j]: the cheapest way of giving the first j said words to the first i of mine; took[i][j] how many
    // the i-th took.
    var best = [[Double]](repeating: [Double](repeating: .infinity, count: s + 1), count: m + 1)
    var took = [[Int]](repeating: [Int](repeating: 0, count: s + 1), count: m + 1)
    best[0][0] = 0
    for i in 1...m {
      let cap = a[i - 1].contains(where: \.isNumber) ? most : 6
      for j in 0...s {
        // Shortest run first: at a tie, the earlier word keeps what it was given ("25 30" is "twenty five" and
        // "thirty", not "twenty" and "five thirty").
        for n in 0...min(cap, j) where best[i - 1][j - n] < .infinity {
          let c = best[i - 1][j - n] + cost(a[i - 1], b[(j - n)..<j])
          if c < best[i][j] { best[i][j] = c; took[i][j] = n }
        }
      }
    }
    // No way within those bounds (hardly possible): the said words shared out evenly.
    guard best[m][s] < .infinity else { return (0..<m).map { ($0 * s / m)..<(($0 + 1) * s / m) } }
    var out = [Range<Int>](repeating: 0..<0, count: m), j = s
    for i in stride(from: m, through: 1, by: -1) {
      let n = took[i][j]
      out[i - 1] = (j - n)..<j
      j -= n
    }
    return out
  }

  /// A said word: its key, and whether it's part of a number read out.
  struct Word { let key: String; let number: Bool }

  /// A word as it's compared: its letters and digits, lowercased.
  public static func key(_ w: String) -> String { w.lowercased().filter { $0.isLetter || $0.isNumber } }

  /// What giving `run` of the said words to a word (its key `a`) costs: 0 when they read the same.
  static func cost(_ a: String, _ run: ArraySlice<Word>) -> Double {
    if run.isEmpty { return a.isEmpty ? 0 : 4 }   // said as nothing: fine for a dash, odd for a word
    if a.isEmpty { return 3 + Double(run.count) }
    let s = run.map(\.key).joined()
    if a == s { return 0 }
    if a.contains(where: \.isNumber) {
      // Its digits were read as number words, about one a digit (none there: they went unsaid, one apiece; more
      // or fewer: a little, so "1H 51M" is "one H" and "fifty one M", not "one H fifty" and "one M"); what's
      // left of it must be what's left of them ("1H": "one H").
      let digits = a.filter(\.isNumber).count, numbers = run.filter(\.number).count
      let off = numbers == 0 ? Double(digits) : 0.1 * Double(abs(numbers - digits))
      return off + distance(a.filter(\.isLetter), run.filter { !$0.number }.map(\.key).joined())
    }
    if run.count == 1 && s.hasPrefix(a) { return 1 }   // an abbreviation: "Nov", "Dr.", "vs."
    return distance(a, s)
  }

  /// How far apart two keys are: the edits between them, or just the length they differ by when that's already
  /// far (saves the work on the many runs that are plainly wrong).
  static func distance(_ a: String, _ b: String) -> Double {
    let (x, y) = (Array(a), Array(b))
    if x.isEmpty || y.isEmpty || abs(x.count - y.count) >= 8 { return Double(abs(x.count - y.count)) }
    var prev = Array(0...y.count), cur = [Int](repeating: 0, count: y.count + 1)
    for i in 1...x.count {
      cur[0] = i
      for j in 1...y.count { cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x[i - 1] == y[j - 1] ? 0 : 1)) }
      swap(&prev, &cur)
    }
    return Double(prev[y.count])
  }

  /// A said word that's part of a number read out ("twenty", "hundred", "and", "third", "point").
  static func numberish(_ w: String) -> Bool {
    w.lowercased().split(separator: "-").allSatisfy { numbers.contains(key(String($0))) }
  }
  static let numbers: Set<String> = {
    var s: Set<String> = ["zero", "oh", "point", "and", "the", "of", "a", "minus", "negative", "percent", "dozen", "half", "quarter", "quarters", "thirds", "oclock"]
    s.formUnion(["hundred", "thousand", "million", "billion", "trillion", "hundredth", "thousandth", "millionth", "billionth", "hundreds", "thousands", "millions", "billions"])
    s.formUnion(["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"])
    s.formUnion(["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"])
    s.formUnion(["twenties", "thirties", "forties", "fifties", "sixties", "seventies", "eighties", "nineties"])
    s.formUnion(["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth", "eleventh", "twelfth", "thirteenth", "fourteenth", "fifteenth", "sixteenth", "seventeenth", "eighteenth", "nineteenth"])
    s.formUnion(["twentieth", "thirtieth", "fortieth", "fiftieth", "sixtieth", "seventieth", "eightieth", "ninetieth"])
    return s
  }()
}
