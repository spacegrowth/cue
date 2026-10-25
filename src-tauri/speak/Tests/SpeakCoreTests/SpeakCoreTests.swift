import XCTest
@testable import SpeakCore

final class SpeakCoreTests: XCTestCase {
  /// The said word each of the sentence's words starts at.
  private func at(_ mine: String, _ said: String) -> [Int] {
    let m = mine.split(separator: " ").map(String.init), s = said.split(separator: " ").map(String.init)
    let runs = Align.runs(mine: m, said: s)
    XCTAssertEqual(runs.map(\.count).reduce(0, +), s.count, "every said word is someone's")
    return runs.map(\.lowerBound)
  }

  func testAWordReadAsSeveralTakesThemAll() {
    XCTAssertEqual(at("on 25 Oct it ran", "on twenty five October it ran"), [0, 1, 3, 4, 5])
    XCTAssertEqual(at("It took 1H 51M on 3 Nov", "It took one H fifty one M on the third of november"), [0, 1, 2, 4, 7, 8, 11])
    XCTAssertEqual(at("p95 of 120ms vs. 80ms.", "p ninety five of one hundred and twenty MS versus eighty MS."), [0, 3, 4, 9, 10])
    XCTAssertEqual(at("said 1,234,567 rows at", "said one million two hundred thirty four thousand five hundred and sixty seven rows at"), [0, 1, 13, 14])
    XCTAssertEqual(at("the FBI said", "the F B I said"), [0, 1, 4])
  }

  func testADashIsSaidAsNothingAndAbbreviationsAreReadOut() {
    // The dash takes no said word and sits where the next starts.
    XCTAssertEqual(at("Nov — then Dr. Smith", "november then doctor Smith"), [0, 1, 1, 2, 3])
    XCTAssertEqual(at("at 9:30 a.m. were fine, e.g. the API's", "at nine thirty AM were fine, for example the API's"), [0, 1, 3, 4, 5, 6, 8, 9])
  }

  func testTheWholeOfANumberHeavySentenceLinesUpToItsEnd() {
    let mine = "It took 1H 51M on 3 Nov — then Dr. Smith said 1,234,567 rows at 9:30 a.m. were fine, e.g. the API's p95 of 120ms vs. 80ms."
    let said = "It took one H fifty one M on the third of november then doctor Smith said one million two hundred thirty four thousand five hundred and sixty seven rows at nine thirty AM were fine, for example the API's p ninety five of one hundred and twenty MS versus eighty MS."
    let got = at(mine, said)
    XCTAssertEqual(got, [0, 1, 2, 4, 7, 8, 11, 12, 12, 13, 14, 15, 16, 28, 29, 30, 32, 33, 34, 35, 37, 38, 39, 42, 43, 48, 49])
    // The last word is at the last said words, not stuck half way (word by word it froze at the twelve-word number).
    XCTAssertEqual(got.last, 49)
  }

  func testWordsTieBreakTowardTheEarlierOne() {
    XCTAssertEqual(at("25 30", "twenty five thirty"), [0, 2])
  }

  func testPartsAreCutAtSpacesAndAtThePiecesKokoroMadeSeparately() {
    // Tokens: 0 is BOS/EOS, 16 a space, 5 a comma, others letters; each lasts the frames in `d`.
    let ids: [Int32] = [0, 1, 2, 16, 3, 4, 5, 0, 0, 6, 7, 16, 8, 0]
    let d: [Int32] = [12, 3, 4, 2, 5, 5, 3, 6, 10, 4, 4, 2, 7, 5]
    XCTAssertEqual(Timing.groups(ids: ids, durations: d, space: 16, edge: 0, marks: [5]), [12, 21, 50, 60], "the second piece's first word starts after its own BOS")
    // A part of only punctuation isn't a word.
    XCTAssertEqual(Timing.groups(ids: [0, 1, 16, 5, 16, 2, 0], durations: [1, 1, 1, 1, 1, 1, 1], space: 16, edge: 0, marks: [5]), [1, 5])
  }

  func testStartsOfALongAnswerRunToItsEnd() {
    // Over 450 characters: Kokoro makes it in two pieces. Every third word a number, read as two words.
    var mine: [String] = [], said: [String] = []
    for i in 0..<90 {
      if i % 3 == 2 { mine.append("\(20 + i)"); said += ["twenty", "two"] } else { mine.append("tables"); said.append("tables") }
    }
    XCTAssertGreaterThan(mine.joined(separator: " ").count, 450)
    // One part per said word, 10 frames each: the said word j starts at frame 10 j.
    let groups = said.indices.map { $0 * 10 }
    let starts = Timing.starts(mine: mine, said: said, parts: said.map { _ in 1 }, groups: groups, msPerFrame: 25)!
    XCTAssertEqual(starts.count, mine.count)
    XCTAssertEqual(starts, starts.sorted(), "never backwards")
    XCTAssertEqual(starts.last, (said.count - 2) * 10 * 25, "the last word at the last said words")
    XCTAssertEqual(starts[2], 2 * 10 * 25)
    XCTAssertEqual(starts[3], 4 * 10 * 25, "after a number read as two words")
  }

  func testStartsCountPartsWhenTheyAddUp() {
    // "FBI" is three parts: its word starts at the first; "said" after all three.
    let starts = Timing.starts(mine: ["the", "FBI", "said"], said: ["the", "FBI", "said"], parts: [1, 3, 1], groups: [0, 10, 20, 30, 40], msPerFrame: 25)
    XCTAssertEqual(starts, [0, 250, 1000])
    // Parts that don't add up: the said words spread over what was heard.
    XCTAssertEqual(Timing.starts(mine: ["a", "b"], said: ["a", "b"], parts: [1, 1], groups: [0, 10, 20, 30], msPerFrame: 1), [0, 20])
    XCTAssertNil(Timing.starts(mine: ["a"], said: ["a"], parts: [1], groups: [], msPerFrame: 1))
  }
}
