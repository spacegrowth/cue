import XCTest
@testable import VoiceCore

final class VoiceCoreTests: XCTestCase {
  func testAnAnswerWaitsWhileYouTalkOrHearAnotherButIsNeverDropped() {
    let now = VoicePhase.allCases.filter(\.takesAnswerNow)
    XCTAssertEqual(Set(now), [.sending, .working, .idle, .said])
    // Every other phase queues it: listening, paused, an answer up for you, one being read.
    XCTAssertEqual(Set(VoicePhase.allCases).subtracting(now), [.listening, .paused, .answered, .saying])
  }

  func testWhatIsReadOutOfAnAnswer() {
    XCTAssertEqual(VoiceText.plain("Run `cargo test`, see [the docs](https://x.y/z)."), "Run cargo test, see the docs.")
    XCTAssertEqual(VoiceText.plain("Done:\n```\nlet x = 1\n```\nall **good**"), "Done:\n \nall good")
    XCTAssertEqual(VoiceText.plain("Open https://example.com/a now"), "Open the link now")
  }

  func testHowLongATurnTook() {
    XCTAssertEqual(VoiceText.took(40), "40S")
    XCTAssertEqual(VoiceText.took(4 * 60 + 59), "4M")
    XCTAssertEqual(VoiceText.took(3600 + 51 * 60), "1H 51M")
  }

  func testReadingMarksTheWholeWordItsOn() {
    let p = "The backfill is done"
    XCTAssertEqual(VoiceText.readTo(p, at: 0), 3, "the first word")
    XCTAssertEqual(VoiceText.readTo(p, at: 0.3), 12, "mid-word: to that word's end")
    XCTAssertEqual(VoiceText.readTo(p, at: 1), p.count)
    XCTAssertEqual(VoiceText.readTo("", at: 0.5), 0)
  }

  func testTheCaptionsLineMarksTheWordBeingSaid() {
    let line = "Row counts match prod on all 14 tables."
    let p = VoiceText.captionParts(line, upTo: 31, yours: false)
    XCTAssertEqual(p.said, "Row counts match prod on all ")
    XCTAssertEqual(p.word, "14")
    XCTAssertEqual(p.ahead, " tables.")
    // Yours: the newest word is the one marked, nothing ahead.
    let y = VoiceText.captionParts("run the backfill", upTo: 0, yours: true)
    XCTAssertEqual([y.said, y.word, y.ahead], ["run the ", "backfill", ""])
    // Out of range never crashes.
    XCTAssertEqual(VoiceText.captionParts("abc", upTo: 99, yours: false).word, "abc")
  }

  func testOnlyTheStretchRoundTheWordIsDrawnCutInBigSteps() {
    XCTAssertEqual(VoiceText.captionCut(saidCount: 0), 0)
    XCTAssertEqual(VoiceText.captionCut(saidCount: 239), 0)
    XCTAssertEqual(VoiceText.captionCut(saidCount: 240), 80)
    XCTAssertEqual(VoiceText.captionCut(saidCount: 2000), 1840)
    // What's left drawn: never under 160 said once cutting (the left of a wide screen), never 240 or more.
    for n in stride(from: 240, to: 5000, by: 13) { XCTAssert((160..<240).contains(n - VoiceText.captionCut(saidCount: n)), "\(n)") }
    // Never more than what's been said, so the word being said is always drawn.
    for n in stride(from: 0, to: 5000, by: 37) { XCTAssertLessThanOrEqual(VoiceText.captionCut(saidCount: n), n) }
  }

  func testTheCaptionsLineGlidesForwardAndJumpsForANewLine() {
    XCTAssertFalse(VoiceText.captionJumps(from: 0, to: -30), "the next word: glide")
    XCTAssertTrue(VoiceText.captionJumps(from: -800, to: 200), "back to the start of a new line")
    XCTAssertTrue(VoiceText.captionJumps(from: 0, to: -900), "far off at once")
  }
}

final class VoiceTurnsTests: XCTestCase {
  private func next(_ t: inout VoiceTurns<String>) -> String {
    switch t.next() { case .present(let a): return a; case .work: return "work"; case .idle: return "idle" }
  }

  func testATurnsAnswerEndsItAndNothingIsLeft() {
    var t = VoiceTurns<String>()
    t.sent()
    XCTAssertTrue(t.answered("done", byTheWay: false, follow: false, phase: .working))
    XCTAssertFalse(t.turnOn)
    XCTAssertEqual(next(&t), "idle")
  }

  func testAByTheWayAnsweredFirstGoesBackToTheTurnThenItsAnswerComes() {
    var t = VoiceTurns<String>()
    t.sent()
    t.askedByTheWay()
    XCTAssertTrue(t.answered("btw", byTheWay: true, follow: false, phase: .working), "up at once")
    XCTAssertTrue(t.turnOn, "the turn is still under way")
    XCTAssertEqual(next(&t), "work", "heard it: back to the turn")
    XCTAssertTrue(t.answered("turn", byTheWay: false, follow: false, phase: .working))
    XCTAssertEqual(next(&t), "idle")
  }

  func testTheTurnsAnswerFirstThenTheByTheWaysWaitsItsTurn() {
    var t = VoiceTurns<String>()
    t.sent()
    t.askedByTheWay()
    XCTAssertTrue(t.answered("turn", byTheWay: false, follow: false, phase: .working))
    // You haven't heard it yet (it's up, waiting for Listen) when the by the way's answer comes.
    XCTAssertFalse(t.answered("btw", byTheWay: true, follow: false, phase: .answered), "queued, not dropped")
    XCTAssertEqual(next(&t), "btw")
    XCTAssertEqual(next(&t), "idle")
  }

  func testAnswersThatComeWhileYouTalkOrListenAreToldInOrder() {
    var t = VoiceTurns<String>()
    XCTAssertFalse(t.answered("one", byTheWay: false, follow: true, phase: .saying))
    XCTAssertFalse(t.answered("two", byTheWay: false, follow: true, phase: .listening))
    XCTAssertEqual(next(&t), "one")
    XCTAssertEqual(next(&t), "two")
    XCTAssertEqual(next(&t), "idle")
  }

  func testAFollowedReplyNeverEndsATurnUnderWay() {
    var t = VoiceTurns<String>()
    t.sent()
    _ = t.answered("later reply", byTheWay: false, follow: true, phase: .working)
    XCTAssertTrue(t.turnOn)
  }

  func testAByTheWayThatFailedLeavesNothingWaiting() {
    var t = VoiceTurns<String>()
    t.askedByTheWay()
    XCTAssertEqual(next(&t), "work", "still asking")
    t.byTheWayFailed()
    t.byTheWayFailed()   // never below none
    XCTAssertEqual(t.asking, 0)
    XCTAssertEqual(next(&t), "idle")
  }

  func testAnAnswerWaitingComesBeforeGoingBackToTheTurn() {
    var t = VoiceTurns<String>()
    XCTAssertFalse(t.answered("earlier", byTheWay: false, follow: true, phase: .listening))
    t.sent()   // what you were saying went
    XCTAssertEqual(next(&t), "earlier", "the answer that came while you talked, first")
    XCTAssertEqual(next(&t), "work", "then the turn you just started")
  }

  func testAByTheWayStillOutOutlivesTheTurnStopping() {
    var t = VoiceTurns<String>()
    t.sent()
    t.askedByTheWay()
    t.stopped()
    XCTAssertEqual(next(&t), "work", "still waiting on the by the way")
  }

  func testAByTheWayAnswerWhileYouTalkWaitsThenTheTurnGoesOn() {
    var t = VoiceTurns<String>()
    t.sent()
    t.askedByTheWay()
    XCTAssertFalse(t.answered("btw", byTheWay: true, follow: false, phase: .listening))
    XCTAssertTrue(t.turnOn)
    XCTAssertEqual(next(&t), "btw")
    XCTAssertEqual(next(&t), "work")
  }

  func testAFollowedReplyWhileAnAnswerIsUpWaitsAndEndsNothing() {
    var t = VoiceTurns<String>()
    t.sent()
    XCTAssertFalse(t.answered("follow", byTheWay: false, follow: true, phase: .answered))
    XCTAssertTrue(t.turnOn)
    XCTAssertEqual(next(&t), "follow")
  }

  func testATurnThatStoppedWithoutAnAnswerIsOver() {
    var t = VoiceTurns<String>()
    t.sent()
    t.stopped()
    XCTAssertEqual(next(&t), "idle")
  }

  func testStrayYeahIsNotSent() {
    XCTAssertTrue(VoiceText.strayYeah("Yeah."))
    XCTAssertTrue(VoiceText.strayYeah(" yeah "))
    XCTAssertFalse(VoiceText.strayYeah("Okay"))
    XCTAssertFalse(VoiceText.strayYeah("Yeah, do it"))
  }

  func testWhoseAnswerIsSaidWhenItIsNeeded() {
    XCTAssertEqual(VoiceText.spoken("Done.", name: "cue · voice", sid: "a", lastSid: nil, more: 0), "Done.", "the only one")
    XCTAssertEqual(VoiceText.spoken("Done.", name: "cue · voice", sid: "a", lastSid: "a", more: 0), "Done.", "same session again")
    XCTAssertEqual(VoiceText.spoken("Done.", name: "cue · voice", sid: "b", lastSid: "a", more: 0), "From cue. Done.", "another session")
    XCTAssertEqual(VoiceText.spoken("Done.", name: "web", sid: "a", lastSid: nil, more: 1), "From web. Done.", "more waiting")
    XCTAssertEqual(VoiceText.more(0), "")
    XCTAssertEqual(VoiceText.more(2), " · 2 MORE")
  }
}
