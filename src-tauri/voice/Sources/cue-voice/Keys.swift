// Keys that work whatever app is in front: Carbon's hot keys, which tell both press and release and need no
// Accessibility or Input Monitoring permission. Each takes its key away from other apps while it's registered,
// so the ones only needed now and then (esc) are registered only then.
import Carbon.HIToolbox

final class HotKeys {
  /// What each key does, by its number: `true` on press, `false` on release.
  nonisolated(unsafe) fileprivate static var actions: [UInt32: (Bool) -> Void] = [:]
  private var refs: [UInt32: EventHotKeyRef] = [:]

  init() {
    var kinds = [
      EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed)),
      EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyReleased)),
    ]
    InstallEventHandler(GetApplicationEventTarget(), { _, event, _ in
      var id = EventHotKeyID()
      GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil, MemoryLayout<EventHotKeyID>.size, nil, &id)
      let down = GetEventKind(event) == UInt32(kEventHotKeyPressed)
      HotKeys.actions[id.id]?(down)
      return noErr
    }, 2, &kinds, nil, nil)
  }

  /// Take `key` (a kVK_ code) with `mods` (Carbon's optionKey and so on) as number `n`. False when another app
  /// has it already.
  @discardableResult
  func add(_ n: UInt32, key: Int, mods: Int, _ action: @escaping (Bool) -> Void) -> Bool {
    if refs[n] != nil { return true }
    var ref: EventHotKeyRef?
    let id = EventHotKeyID(signature: OSType(0x4355_4556), id: n)   // "CUEV"
    guard RegisterEventHotKey(UInt32(key), UInt32(mods), id, GetApplicationEventTarget(), 0, &ref) == noErr, let ref else { return false }
    refs[n] = ref
    HotKeys.actions[n] = action
    return true
  }

  /// Give number `n`'s key back to other apps.
  func remove(_ n: UInt32) {
    if let ref = refs.removeValue(forKey: n) { UnregisterEventHotKey(ref) }
    HotKeys.actions[n] = nil
  }
}

/// A modifier held on its own (right ⇧): read from the keyboard's state as it's held, which needs no permission
/// either. By its device flag (IOLLEvent.h's NX_DEVICE…KEYMASK): the key code says "Shift" for either one. It counts once held `after` seconds with no other key pressed meanwhile, so a capital typed with it
/// never does. `action`: `true` then, `false` when it's let go. `tap`: let go before that, with no other key or
/// click meanwhile (a shift-click isn't one).
@MainActor
final class HeldKey {
  private var timer: Timer?
  private var since: Date?
  /// Another key went down while it was held: typing, not this.
  private var typing = false
  private var on = false
  /// → was down at some point during this hold (right ⇧ and → together: by the way), even if it's up again.
  private(set) var arrowSeen = false
  /// The key went down (true) or up (false), whether or not it counts: for keys to take from other apps at once.
  var pressed: (Bool) -> Void = { _ in }

  /// Right ⇧ and left ⇧.
  static let rightShift: UInt64 = 0x4, leftShift: UInt64 = 0x2

  init(_ flag: UInt64, after: TimeInterval, tap: (() -> Void)? = nil, _ action: @escaping (Bool) -> Void) {
    let t = Timer(timeInterval: 0.05, repeats: true) { [weak self] _ in
      MainActor.assumeIsolated {
        guard let self else { return }
        guard HeldKey.down(flag) else {
          if self.since != nil { self.pressed(false) }
          if self.on { action(false) }
          else if let since = self.since, !self.typing, !HeldKey.other(since: since) { tap?() }
          self.on = false
          self.since = nil
          self.arrowSeen = false   // this hold's: not the next one's (nor ⌥Space's)
          return
        }
        let since = self.since ?? Date()
        if self.since == nil { self.since = since; self.typing = false; self.arrowSeen = false; self.pressed(true) }
        if CGEventSource.keyState(.combinedSessionState, key: 124) { self.arrowSeen = true }
        guard !self.on, !self.typing else { return }
        let held = Date().timeIntervalSince(since)
        // Another key meanwhile is typing, except → held with it (right ⇧ and → together: by the way).
        if HeldKey.other(since: since), !self.arrowSeen { self.typing = true }
        else if held >= after { self.on = true; action(true) }
      }
    }
    t.tolerance = 0.02
    RunLoop.main.add(t, forMode: .common)
    timer = t
  }

  /// Stop watching the key.
  func stop() { timer?.invalidate(); timer = nil }

  /// A key pressed or a click since `since` (with the key held: not this key on its own).
  static func other(since: Date) -> Bool {
    let held = Date().timeIntervalSince(since)
    return [CGEventType.keyDown, .leftMouseDown, .rightMouseDown].contains { CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: $0) < held }
  }

  /// Down now (held, whether or not it counts yet).
  static func down(_ flag: UInt64) -> Bool { CGEventSource.flagsState(.combinedSessionState).rawValue & flag != 0 }
}
