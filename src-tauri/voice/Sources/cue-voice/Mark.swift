// The mark in the menu bar while "Hey Cue" is on (Settings → Voice): a ring with a dot, next to the Mac's own
// orange dot for the mic, so you can see it's listening. Drawn as a template image, so the menu bar colors it for
// light and dark. A click on it: a menu to stop listening.
import AppKit

@MainActor
final class Mark: NSObject {
  /// "Stop listening for Hey Cue" picked in its menu.
  var onStop: () -> Void = {}
  private var item: NSStatusItem?

  /// Shown (true) or gone.
  func show(_ on: Bool) {
    if on, item == nil {
      let it = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
      it.button?.image = Mark.image
      it.button?.setAccessibilityLabel("Cue is listening for Hey Cue")
      let menu = NSMenu()
      let head = NSMenuItem(title: "Listening for “Hey Cue”", action: nil, keyEquivalent: "")
      head.isEnabled = false
      menu.addItem(head)
      menu.addItem(.separator())
      let stop = NSMenuItem(title: "Stop Listening", action: #selector(stopPicked), keyEquivalent: "")
      stop.target = self
      menu.addItem(stop)
      it.menu = menu
      item = it
    } else if !on, let it = item {
      NSStatusBar.system.removeStatusItem(it)
      item = nil
    }
  }

  @objc private func stopPicked() { onStop() }

  /// A ring with a dot in it, 16 points, as the mock draws it.
  private static let image: NSImage = {
    let img = NSImage(size: NSSize(width: 16, height: 16), flipped: false) { r in
      NSColor.black.set()
      let ring = NSBezierPath(ovalIn: r.insetBy(dx: 2.25, dy: 2.25))
      ring.lineWidth = 1.6
      ring.stroke()
      NSBezierPath(ovalIn: NSRect(x: r.midX - 2, y: r.midY - 2, width: 4, height: 4)).fill()
      return true
    }
    img.isTemplate = true
    return img
  }()
}
