// The island at the notch: a black shape that grows out of the top of the screen, in the colors of the session
// you're talking to: whose it is, what's happening, and the keys and buttons that do something now. While it works
// it's small (`compact`), round the notch: a dot and the clock. A screen with no notch gets the same island at the
// top middle. With live captions on (`bar`) the island never shows: the bar along the bottom of the screen has it
// all, whose it is, what's happening, the words (one line, edge to edge, moving right to left as they're said, the
// word being said marked), the keys and buttons; while it works only a dot breathes in the bottom right corner.
// All are floating panels that never take the keyboard from the app you're in; a click on the island or the dot
// opens the chat in Cue, and the bar takes clicks only for its buttons.
import AppKit
import SwiftUI
import VoiceCore

/// A session's colors as Cue's talk.rs sends them (#rrggbb): its window's green and the color on it.
struct Look: Equatable {
  var bg = "#3F4A22", onBg = "#FFFFFF", sel = "#D2DE96", onSel = "#1E2410"
}

@MainActor
final class PillModel: ObservableObject {
  @Published var name = ""
  @Published var look = Look()
  @Published var label = ""
  /// Shown in the island itself (an answer read here, an error): words being said go to the line instead.
  @Published var text = ""
  /// Your words, not its answer.
  @Published var yours = false
  @Published var level: Float = 0
  @Published var showLevel = false
  @Published var speaking = false
  /// While it reads: how long is left, in seconds (the bar counts it down).
  @Published var left: Double?
  /// Since when the clock beside the label counts (nil: no clock).
  @Published var since: Date?
  @Published var keys: [(String, String)] = []
  /// Which of `keys` the bar shows (one at a time).
  @Published var hint = 0
  /// Buttons under the label (an answer waiting: Listen, Read it here…).
  @Published var buttons: [PillButton] = []
  /// Round the notch while a session works, and what it's doing ("Running: cargo test").
  @Published var compact = false
  @Published var doing = ""
  /// Live captions on: the bar along the bottom instead of the island, the dot while it works.
  @Published var bar = false
  /// The line along the bottom: shown, its words, and how many of them are said (the word ending there is marked).
  @Published var ticker = false
  /// The height of `text` above the bar's row (an answer read here, a problem), when there's text.
  @Published var textHeight: CGFloat = 0
  @Published var line = ""
  @Published var upTo = 0
  /// The notch of the screen it's on (none: a stand-in the menu bar's height).
  @Published var notch = CGSize(width: 0, height: 32)
}

/// A button in the island: the first, `primary`, in the session's own color.
struct PillButton {
  var title: String
  var icon: String? = nil
  var primary = false
  var action: () -> Void
}

/// A panel the system leaves where it's put, over the menu bar too.
final class TopPanel: NSPanel {
  override func constrainFrameRect(_ r: NSRect, to screen: NSScreen?) -> NSRect { r }
}

/// Takes the first click while another app is in front (a film playing): without it that click only woke the panel
/// and the app behind kept the keyboard.
final class FirstClick<V: View>: NSHostingView<V> {
  override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

@MainActor
final class Pill {
  let m = PillModel()
  var onClick: () -> Void = {}
  /// It came up (true) or went (false).
  var onShown: (Bool) -> Void = { _ in }
  private let panel: TopPanel
  private let host: NSHostingView<PillView>
  private let linePanel: TopPanel
  private let lineHost: NSHostingView<LineView>
  private let dotPanel: TopPanel
  private var hideTimer: Timer?
  /// Going (sliding up; shown again meanwhile: it comes back down).
  private var hiding = false
  /// The island sliding in or out: `fit` leaves its frame to the slide meanwhile.
  private var sliding = false
  /// Where the island and the line go, on the screen you're working on.
  private var islandFrame = NSRect.zero
  private var lineFrame = NSRect.zero
  /// The line rolling in or out (it rolls in from the right edge and back out to it): which way (nil: still),
  /// and which roll is the latest (an earlier one's end mustn't act on a later one).
  private var rollingIn: Bool?
  private var rolls = 0
  private var rolling: Bool { rollingIn != nil }
  static let width: CGFloat = 560
  static let lineHeight: CGFloat = 54
  static let dotSize: CGFloat = 28

  init() {
    panel = Pill.makePanel()
    host = NSHostingView(rootView: PillView(m: m, click: {}))
    linePanel = Pill.makePanel()
    lineHost = FirstClick(rootView: LineView(m: m))
    dotPanel = Pill.makePanel()
    host.rootView = PillView(m: m, click: { [weak self] in self?.onClick() })
    lineHost.rootView = LineView(m: m, click: { [weak self] in self?.onClick() })
    dotPanel.contentView = FirstClick(rootView: Dot(m: m, click: { [weak self] in self?.onClick() }))
    // Only `place` sizes the panel (centred at the top): left to itself the view resizes its window from the
    // window's corner, off-centre.
    host.sizingOptions = [.intrinsicContentSize]
    panel.contentView = host
    // The line keeps the screen's width inside a box that grows from the right edge, so rolling in and out only
    // shows more or less of it: it's never laid out narrow.
    lineHost.sizingOptions = []
    lineHost.autoresizingMask = [.minXMargin]   // its right edge stays on the box's; its height never changes
    let box = NSView(frame: NSRect(x: 0, y: 0, width: 1, height: Pill.lineHeight))
    box.wantsLayer = true
    box.layer?.masksToBounds = true
    box.addSubview(lineHost)
    linePanel.contentView = box
  }

  private static func makePanel() -> TopPanel {
    let p = TopPanel(contentRect: NSRect(x: 0, y: 0, width: Pill.width, height: 120), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
    p.isFloatingPanel = true
    p.level = .statusBar
    p.backgroundColor = .clear
    p.isOpaque = false
    p.hasShadow = false
    p.hidesOnDeactivate = false
    p.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
    return p
  }

  /// Show it (or keep it shown), sized to what's in it, on the screen you're working on; `for` seconds, then
  /// it goes by itself (nil: it stays). It slides down from the top edge.
  func show(for seconds: Double? = nil) {
    hideTimer?.invalidate()
    hideTimer = seconds.map { s in Timer.scheduledTimer(withTimeInterval: s, repeats: false) { _ in MainActor.assumeIsolated { self.hide() } } }
    let coming = !panel.isVisible || hiding
    if coming {
      hiding = false
      onShown(true)
    }
    fit()
    if coming {
      if !panel.isVisible { panel.setFrame(islandFrame.offsetBy(dx: 0, dy: islandFrame.height), display: false) }
      panel.alphaValue = m.bar ? 0 : 1
      panel.orderFrontRegardless()
      slide(to: islandFrame) {}
    }
    DispatchQueue.main.async { MainActor.assumeIsolated { self.fit() } }
  }

  /// It slides back up into the top edge, and the line rolls out to the right.
  func hide() {
    hideTimer?.invalidate()
    guard panel.isVisible, !hiding else { return }
    hiding = true
    onShown(false)
    showLine(false)
    dotPanel.orderOut(nil)
    slide(to: islandFrame.offsetBy(dx: 0, dy: islandFrame.height)) { if self.hiding { self.hiding = false; self.panel.orderOut(nil) } }
  }

  /// Where the slide in is going (what it shows can grow while it slides: it goes there instead).
  private var slideTarget = NSRect.zero
  private var slides = 0
  private func slide(to r: NSRect, then: @escaping () -> Void) {
    sliding = true
    slideTarget = r
    slides += 1
    let this = slides
    NSAnimationContext.runAnimationGroup({ c in
      c.duration = 0.22
      c.timingFunction = CAMediaTimingFunction(name: .easeOut)
      panel.animator().setFrame(r, display: true)
    }, completionHandler: { MainActor.assumeIsolated {
      guard this == self.slides else { return }   // a later slide took over
      self.sliding = false
      then()
      if self.panel.isVisible, !self.hiding { self.fit() }
    } })
  }

  /// Its size follows what it shows; it sits at the top middle of the screen the mouse is on, the line along
  /// that screen's bottom.
  func place() {
    fit()
    // SwiftUI takes what changed on its next pass (the island growing): fit again then.
    DispatchQueue.main.async { MainActor.assumeIsolated { self.fit() } }
  }

  private func fit() {
    guard let screen = NSScreen.screens.first(where: { NSMouseInRect(NSEvent.mouseLocation, $0.frame, false) }) ?? NSScreen.main else { return }
    let f = screen.frame
    let notch: CGSize
    if screen.safeAreaInsets.top > 0 {
      let w = f.width - (screen.auxiliaryTopLeftArea?.width ?? f.width) - (screen.auxiliaryTopRightArea?.width ?? 0)
      notch = CGSize(width: max(0, w), height: screen.safeAreaInsets.top)
    } else {
      notch = CGSize(width: 0, height: max(24, f.maxY - screen.visibleFrame.maxY))
    }
    if m.notch != notch { m.notch = notch }
    host.layoutSubtreeIfNeeded()
    let size = host.fittingSize
    islandFrame = NSRect(x: f.midX - size.width / 2, y: f.maxY - size.height, width: size.width, height: size.height)
    // SwiftUI sizes what changed a moment later, so the size can change mid-slide: slide to the new one then.
    if panel.isVisible, !hiding {
      if !sliding { panel.setFrame(islandFrame, display: true) } else if slideTarget != islandFrame { slide(to: islandFrame) {} }
    }
    // With the bar, the island is there for its timing (shown, hidden) but not seen, and takes no clicks.
    panel.alphaValue = m.bar ? 0 : 1
    panel.ignoresMouseEvents = m.bar
    // Text in the bar (an answer read here, a problem): up to six lines above its row.
    let textHeight = m.text.isEmpty ? 0 : min(6 * 22, NSAttributedString(string: m.text, attributes: [.font: LineView.textFont])
      .boundingRect(with: CGSize(width: f.width - 56, height: .greatestFiniteMagnitude), options: [.usesLineFragmentOrigin]).height.rounded(.up)) + 14
    if m.textHeight != textHeight { m.textHeight = textHeight }
    let height = Pill.lineHeight + textHeight
    lineFrame = NSRect(x: f.minX, y: f.minY, width: f.width, height: height)
    let box = linePanel.contentView?.bounds ?? .zero
    let full = NSRect(x: box.width - f.width, y: 0, width: f.width, height: height)
    if lineHost.frame != full { lineHost.frame = full }
    if linePanel.isVisible, !rolling { linePanel.setFrame(lineFrame, display: true) }
    // A click on it opens the chat, never the app under it.
    linePanel.ignoresMouseEvents = false
    showLine(m.ticker && panel.isVisible && !hiding)
    let d = Pill.dotSize
    dotPanel.setFrame(NSRect(x: f.maxX - d - 36, y: f.minY + 10, width: d, height: d), display: false)
    let dot = m.bar && m.compact && panel.isVisible && !hiding
    if dot != dotPanel.isVisible { if dot { dotPanel.orderFrontRegardless() } else { dotPanel.orderOut(nil) } }
  }

  /// The line rolls in from the right edge to the left, and goes back the same way (its left end running right).
  private func showLine(_ on: Bool) {
    let edge = NSRect(x: lineFrame.maxX - 1, y: lineFrame.minY, width: 1, height: lineFrame.height)
    if on, !linePanel.isVisible {
      linePanel.setFrame(edge, display: false)
      linePanel.alphaValue = 1
      linePanel.orderFrontRegardless()
      roll(in: true, to: lineFrame)
    } else if on, rollingIn == false {
      roll(in: true, to: lineFrame)   // asked back while rolling out: back in
    } else if !on, linePanel.isVisible, rollingIn != false {
      roll(in: false, to: edge)   // once: asked again while it rolls out, it just goes on rolling
    }
  }

  private func roll(in going: Bool, to r: NSRect) {
    rollingIn = going
    rolls += 1
    let this = rolls
    NSAnimationContext.runAnimationGroup({ c in
      c.duration = 0.3
      c.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
      linePanel.animator().setFrame(r, display: true)
    }, completionHandler: { MainActor.assumeIsolated {
      guard this == self.rolls else { return }   // a later roll took over
      self.rollingIn = nil
      if !going { self.linePanel.orderOut(nil) }
    } })
  }
}

private let island = Color(red: 13 / 255, green: 15 / 255, blue: 8 / 255)
private let onIsland = Color(red: 236 / 255, green: 238 / 255, blue: 227 / 255)

struct PillView: View {
  @ObservedObject var m: PillModel
  let click: () -> Void
  private var ink: Color { onIsland }

  var body: some View {
    Group { if m.compact { small } else { big } }
      .contentShape(Rectangle())
      .onTapGesture(perform: click)
  }

  /// While it works: round the notch, small: the time so far on its left, and what it's doing crawling right to
  /// left from its right edge, behind the notch, to the time.
  private var small: some View {
    // Left of the notch: the clock, still; the right side is as wide, so the notch stays centred.
    let left: CGFloat = 34, right: CGFloat = 70
    return HStack(spacing: 0) {
      TimelineView(.periodic(from: m.since ?? Date(), by: 1)) { t in
        Text(m.since.map { clock(t.date.timeIntervalSince($0)) } ?? "")
          .font(.system(size: 10.5, weight: .semibold).monospacedDigit())
          .foregroundStyle(ink.opacity(0.65))
      }
      .frame(width: left, alignment: .leading)
      // From the right edge across to the clock: it slides behind the notch (nothing shows there) and out again.
      let across = (right - left) + max(m.notch.width, 110) + right
      Group {
        if m.doing.isEmpty || m.doing.hasPrefix("Thinking") || m.doing.hasPrefix("By the way") {
          // Only thinking ("Thinking…"): no step to show yet, so it stays put, centred beside the notch, its dots
          // counting 1, 2, 3 in a space of their own (the word never moves).
          TimelineView(.periodic(from: .now, by: 0.4)) { t in
            HStack(spacing: 0) {
              Text(m.doing.hasPrefix("By the way") ? "By the way" : "Thinking")
              Text(String(repeating: ".", count: Int(t.date.timeIntervalSinceReferenceDate / 0.4) % 4 + 1)).frame(width: 12, alignment: .leading)
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(ink.opacity(0.85))
            .lineLimit(1)
            .fixedSize()   // one line always: it wrapped when the space was a little short
            .frame(width: m.notch.width > 0 ? right : across)
          }
          // Beside the notch when the screen has one (nothing shows in it); else in the island's very middle.
          .frame(width: across, alignment: .trailing)
          .offset(x: m.notch.width > 0 ? 0 : -left / 2)
        } else {
          // Every frame the screen draws, so it moves evenly.
          TimelineView(.animation) { t in
            Crawl(text: m.doing, at: t.date, width: across, ink: ink)
          }
        }
      }
      .frame(width: across, height: m.notch.height)
      .clipped()
    }
    .padding(.horizontal, 14)
    .frame(height: m.notch.height)
    .background(UnevenRoundedRectangle(bottomLeadingRadius: 12, bottomTrailingRadius: 12).fill(island))
    .fixedSize()
  }

  private var name: some View {
    HStack(spacing: 6) {
      Circle().fill(Color(hex: m.look.bg)).frame(width: 7, height: 7)
      Text(m.name).font(.system(size: 12, weight: .semibold)).lineLimit(1)
    }
    .foregroundStyle(Color(hex: m.look.onSel))
    .padding(.horizontal, 9).padding(.vertical, 3)
    .background(Capsule().fill(Color(hex: m.look.sel)))
  }

  @ViewBuilder private var label: some View {
    if !m.label.isEmpty {
      Group {
        if let since = m.since {
          TimelineView(.periodic(from: since, by: 1)) { t in Text("\(m.label) · \(clock(t.date.timeIntervalSince(since)))") }
        } else { Text(m.label) }
      }
      .font(.system(size: 11, weight: .bold)).tracking(1.2).opacity(0.8)
    }
  }

  /// Grown out of the notch: whose it is, what's happening, and what you can do now.
  private var big: some View {
    VStack(alignment: .leading, spacing: 8) {
      Color.clear.frame(height: max(0, m.notch.height - 6))
      HStack(spacing: 10) {
        name
        label
        Spacer(minLength: 0)
        if m.showLevel { Bars(level: m.level).foregroundStyle(Color(hex: m.look.sel)) }
        if m.speaking { Image(systemName: "speaker.wave.2.fill").font(.system(size: 13)).foregroundStyle(Color(hex: m.look.sel)) }
      }
      if !m.text.isEmpty {
        Text(m.text)
          .font(.system(size: 16, weight: m.yours ? .medium : .semibold))
          .lineSpacing(3)
          .opacity(m.yours ? 0.7 : 1)
          .lineLimit(6)
          .truncationMode(.head)
          .fixedSize(horizontal: false, vertical: true)
      }
      if !m.buttons.isEmpty {
        HStack(spacing: 8) {
          ForEach(Array(m.buttons.enumerated()), id: \.offset) { _, b in
            Button(action: b.action) {
              HStack(spacing: 6) {
                if let icon = b.icon { Image(systemName: icon).font(.system(size: 12)) }
                Text(b.title).font(.system(size: 13, weight: .semibold))
              }
              .foregroundStyle(b.primary ? Color(hex: m.look.onSel) : ink)
              .padding(.horizontal, 14).padding(.vertical, 6)
              .background(Capsule().fill(b.primary ? Color(hex: m.look.sel) : ink.opacity(0.14)))
              .contentShape(Capsule())
            }
            .buttonStyle(.plain)
          }
        }
      }
      if !m.keys.isEmpty {
        HStack(spacing: 14) {
          ForEach(Array(m.keys.enumerated()), id: \.offset) { _, k in
            HStack(spacing: 5) {
              Text(k.0).font(.system(size: 11, weight: .semibold)).padding(.horizontal, 6).padding(.vertical, 1)
                .background(RoundedRectangle(cornerRadius: 5).fill(ink.opacity(0.16)))
              Text(k.1).font(.system(size: 12))
            }
          }
        }
        .opacity(0.75)
      }
    }
    .foregroundStyle(ink)
    .padding(.horizontal, 18).padding(.bottom, 14)
    .frame(width: max(Pill.width, m.notch.width + 200), alignment: .leading)
    .background(UnevenRoundedRectangle(bottomLeadingRadius: 24, bottomTrailingRadius: 24).fill(island))
  }

  private func clock(_ s: TimeInterval) -> String { String(format: "%d:%02d", Int(s) / 60, Int(s) % 60) }
}

/// A short text crawling right to left round and round in a narrow space, at an even pace.
struct Crawl: View {
  let text: String
  let at: Date
  /// The space it crawls in: copies enough to fill it, end to end.
  let width: CGFloat
  var ink: Color = onIsland
  /// Digits all as wide, so the clock ticking doesn't change the text's width (where it is, is a remainder of it).
  private static let font = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .medium)

  var body: some View {
    let w = max(1, NSAttributedString(string: text, attributes: [.font: Crawl.font]).size().width + 24)
    let x = -CGFloat((at.timeIntervalSinceReferenceDate * 28).truncatingRemainder(dividingBy: Double(w)))
    HStack(spacing: 24) {
      ForEach(0..<(Int(width / w) + 2), id: \.self) { _ in Text(text) }
    }
    .font(.system(size: 11, weight: .medium).monospacedDigit())
    .foregroundStyle(ink.opacity(0.85))
    .lineLimit(1)
    .fixedSize()
    .offset(x: x)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
  }
}

/// The bar along the bottom of the screen: whose it is and what's happening at its left end, then the words on one
/// line that moves right to left as they're said, so the word being said stays in one place (about two thirds
/// across), then the keys and buttons. What's been said fades off to the left; its answer's word being said is
/// marked. Text to read (an answer read here, a problem) sits above the row.
struct LineView: View {
  @ObservedObject var m: PillModel
  /// A click anywhere on it (but its buttons): the chat opens, as a click on the island does.
  var click: () -> Void = {}
  @StateObject private var follow = Follower()
  private static let size: CGFloat = 22
  private static let font = NSFont.systemFont(ofSize: size, weight: .semibold)
  static let textFont = NSFont.systemFont(ofSize: 16, weight: .semibold)
  private var ink: Color { Color(hex: m.look.onSel) }

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      if !m.text.isEmpty {
        Text(m.text)
          .font(.system(size: 16, weight: .semibold))
          .lineSpacing(3)
          .lineLimit(6)
          .truncationMode(.head)
          .foregroundStyle(ink)
          .padding(.horizontal, 28).padding(.top, 12)
          .frame(maxWidth: .infinity, maxHeight: m.textHeight, alignment: .topLeading)
      }
      row.frame(height: Pill.lineHeight)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
    .background(Color(hex: m.look.sel))
    .contentShape(Rectangle())
    .onTapGesture(perform: click)
  }

  private var row: some View {
    HStack(spacing: 12) {
      HStack(spacing: 6) {
        // Breathing while someone talks (you, or it reading), still otherwise.
        TimelineView(.animation(minimumInterval: 1 / 20, paused: !talking)) { t in
          Circle().fill(Color(hex: m.look.sel)).frame(width: 7, height: 7)
            .opacity(talking ? 0.35 + 0.65 * (0.5 + 0.5 * sin(t.date.timeIntervalSinceReferenceDate * 2 * .pi / 1.6)) : 1)
        }
        .frame(width: 7, height: 7)
        Text(m.yours ? "You · to \(m.name)" : m.name).font(.system(size: 12, weight: .semibold)).lineLimit(1)
      }
      .foregroundStyle(Color(hex: m.look.onBg))
      .padding(.horizontal, 10).padding(.vertical, 3)
      .background(Capsule().fill(Color(hex: m.look.bg)))
      .fixedSize()
      // The time: up while you talk, down while it reads (how long is left).
      Group {
        if m.showLevel, let since = m.since {
          TimelineView(.periodic(from: since, by: 1)) { t in Text(LineView.clock(t.date.timeIntervalSince(since))) }
        } else if let left = m.left {   // held where it is while paused
          Text(LineView.clock(left.rounded(.up)))
        }
      }
      .font(.system(size: 12, weight: .semibold).monospacedDigit())
      .foregroundStyle(Color(hex: m.look.bg))
      .fixedSize()
      if m.line.isEmpty { Spacer(minLength: 0) } else { words }
      if m.bar { end }
    }
    .padding(.leading, 14)
  }

  /// The buttons at the right end when there are any (an answer waiting), and one of the keys, a different one each
  /// time, so they're learned over time without all of them at once.
  private var end: some View {
    HStack(spacing: 12) {
      ForEach(Array(m.buttons.enumerated()), id: \.offset) { _, b in
        Button(action: b.action) {
          HStack(spacing: 6) {
            if let icon = b.icon { Image(systemName: icon).font(.system(size: 11)) }
            Text(b.title).font(.system(size: 12.5, weight: .semibold))
          }
          .foregroundStyle(b.primary ? Color(hex: m.look.onBg) : ink)
          .padding(.horizontal, 12).padding(.vertical, 5)
          .background(Capsule().fill(b.primary ? Color(hex: m.look.bg) : ink.opacity(0.1)))
          .contentShape(Capsule())
        }
        .buttonStyle(.plain)
      }
      if m.keys.indices.contains(m.hint) {
        let k = m.keys[m.hint]
        HStack(spacing: 4) {
          Text(k.0).font(.system(size: 11, weight: .semibold)).padding(.horizontal, 6).padding(.vertical, 1)
            .background(RoundedRectangle(cornerRadius: 5).fill(ink.opacity(0.12)))
          Text(k.1).font(.system(size: 12))
        }
        .foregroundStyle(Color(hex: m.look.bg))
      }
    }
    .lineLimit(1)
    .fixedSize()
    .padding(.horizontal, 12)
    .frame(height: Pill.lineHeight)
    .background(Color(hex: m.look.sel))
  }

  private var words: some View {
      GeometryReader { g in
        let (said, word, ahead) = parts
        // The newest word always sits at the same spot from the very first word: the words move, your eyes don't.
        let x = g.size.width * 0.66 - LineView.width(said + word)
        // Only the stretch round the word is drawn (a line too wide isn't drawn at all: VoiceText.captionCut).
        // What's cut off the front is cut in steps, its width added back to where the line is drawn, so the cut
        // never shows.
        let cut = VoiceText.captionCut(saidCount: said.count)
        let front = LineView.width(String(said.prefix(cut)))
        // One text, the word marked inside it: measured and drawn alike, so no part slips against another.
        Text(styled(String(said.dropFirst(cut)), word, String(ahead.prefix(VoiceText.captionAhead))))
        .font(.system(size: LineView.size, weight: .semibold))
        .lineLimit(1)
        .fixedSize()
        .offset(x: follow.x + front)
        .frame(width: g.size.width, height: g.size.height, alignment: .leading)
        .onAppear { follow.jump(to: x) }
        .onChange(of: x) { _, nx in follow.go(to: nx) }
      }
      .clipped()
  }


  /// You're talking, or it's reading (not paused).
  private var talking: Bool { m.showLevel || m.speaking }

  static func clock(_ s: TimeInterval) -> String { String(format: "%d:%02d", Int(s) / 60, Int(s) % 60) }

  private func styled(_ said: String, _ word: String, _ ahead: String) -> AttributedString {
    var s = AttributedString(said)
    s.foregroundColor = Color(hex: m.look.onSel).opacity(m.yours ? 0.8 : 0.5)
    var w = AttributedString(word)
    w.foregroundColor = Color(hex: m.look.onBg)
    w.backgroundColor = Color(hex: m.look.bg)
    var a = AttributedString(ahead)
    a.foregroundColor = Color(hex: m.look.onSel).opacity(0.4)
    return s + w + a
  }

  /// Said so far, the word being said (yours: the newest), and what's to come.
  private var parts: (String, String, String) {
    let p = VoiceText.captionParts(m.line, upTo: m.upTo, yours: m.yours)
    return (p.said, p.word, p.ahead)
  }

  /// The line is drawn every frame while it glides; the same few texts are measured over and over, so the last
  /// few measures are kept.
  @MainActor private static var measured: [(String, CGFloat)] = []
  @MainActor private static func width(_ s: String) -> CGFloat {
    if let hit = measured.first(where: { $0.0 == s }) { return hit.1 }
    let w = NSAttributedString(string: s, attributes: [.font: font]).size().width
    measured.append((s, w))
    if measured.count > 6 { measured.removeFirst() }
    return w
  }
}

/// Where the line is drawn, moving to where it should be at a steady speed, frame by frame: the words come a
/// word (or a burst of words) at a time, and following them at an even pace makes the line slide letter by letter
/// instead of hopping.
@MainActor
final class Follower: ObservableObject {
  @Published private(set) var x: CGFloat = 0
  private var target: CGFloat = 0
  private var speed: CGFloat = 0
  private var timer: Timer?
  private var last = Date()

  func jump(to t: CGFloat) { target = t; x = t; speed = 0 }

  func go(to t: CGFloat) {
    // A new line (or a big jump back): there, at once.
    if VoiceText.captionJumps(from: Double(x), to: Double(t)) { return jump(to: t) }
    target = t
    guard timer == nil else { return }
    last = Date()
    let tm = Timer(timeInterval: 1 / 60, repeats: true) { [weak self] _ in MainActor.assumeIsolated { self?.tick() } }
    RunLoop.main.add(tm, forMode: .common)
    timer = tm
  }

  private func tick() {
    let now = Date(), dt = CGFloat(min(0.05, now.timeIntervalSince(last)))
    last = now
    let gap = target - x
    // The pace that closes the gap in about a second, smoothed hard so it stays even between bursts of words.
    speed += (gap / 0.9 - speed) * min(1, dt * 2)
    x += speed * dt
    if abs(gap) < 0.5, abs(speed) < 5 {
      x = target
      speed = 0
      timer?.invalidate()
      timer = nil
    }
  }
}

/// While it works, with live captions on: a dot in the session's color in the bottom right corner, fading slowly in
/// and out. A click opens the chat.
struct Dot: View {
  @ObservedObject var m: PillModel
  let click: () -> Void
  var body: some View {
    TimelineView(.animation(minimumInterval: 1 / 20)) { t in
      let breath = 0.5 + 0.5 * sin(t.date.timeIntervalSinceReferenceDate * 2 * .pi / 2.4)
      Circle()
        .fill(Color(hex: m.look.sel))
        .frame(width: 10, height: 10)
        .overlay(Circle().stroke(Color(hex: m.look.bg).opacity(0.5), lineWidth: 1))
        .opacity(0.25 + 0.65 * breath)
    }
    .frame(width: Pill.dotSize, height: Pill.dotSize)
    .contentShape(Rectangle())
    .onTapGesture(perform: click)
  }
}

/// How loud you are: bars that rise with your voice.
struct Bars: View {
  let level: Float
  private static let shape: [CGFloat] = [0.3, 0.5, 0.8, 1, 0.7, 0.4, 0.85, 1, 0.6, 0.3, 0.7, 0.9, 0.5, 0.3]
  var body: some View {
    HStack(spacing: 2) {
      ForEach(Array(Bars.shape.enumerated()), id: \.offset) { _, s in
        RoundedRectangle(cornerRadius: 1.5).frame(width: 3, height: max(4, 24 * s * CGFloat(0.25 + 0.75 * level)))
      }
    }
    .frame(height: 24)
    .animation(.easeOut(duration: 0.08), value: level)
  }
}

extension Color {
  /// #rrggbb.
  init(hex: String) {
    let v = UInt64(hex.trimmingCharacters(in: CharacterSet(charactersIn: "#")), radix: 16) ?? 0
    self.init(red: Double((v >> 16) & 0xFF) / 255, green: Double((v >> 8) & 0xFF) / 255, blue: Double(v & 0xFF) / 255)
  }
}
