// Whatever is playing (a video in the browser, music) pauses while you talk and while an answer is read out, and
// plays again after. It asks the Mac's Now Playing (the MediaRemote framework, as the media keys and Control Centre
// do) to pause and to play, which needs no permission; only when Now Playing says it's playing, so it never
// starts something you'd paused. A browser keeps sending sound out with its video paused, so that alone isn't
// enough; and macOS no longer tells an app like this one what's playing, so it asks through osascript, which
// it still tells (about 80 ms, off the main thread).
import Foundation
import CoreAudio

@MainActor
final class Hush {
  /// Paused by us: played again once you're done.
  private var paused = false
  /// Each on() and off(): an answer from Now Playing that comes after you're done already is dropped.
  private var turn = 0

  func on() {
    guard !paused, Hush.othersPlaying() else { return }
    turn += 1
    let mine = turn
    DispatchQueue.global(qos: .userInitiated).async {
      let playing = Hush.nowPlaying()
      DispatchQueue.main.async { MainActor.assumeIsolated { self.pause(mine, playing) } }
    }
  }

  private func pause(_ mine: Int, _ playing: Bool?) {
    guard mine == turn, !paused else { return }
    guard playing == true else { return note(playing == nil ? "hush: couldn't ask Now Playing" : "hush: nothing playing") }
    guard Hush.command(Hush.pause) else { return note("hush: Now Playing didn't take pause") }
    paused = true
    note("hush: paused what was playing")
  }

  func off() {
    turn += 1
    guard paused else { return }
    paused = false
    _ = Hush.command(Hush.play)
    note("hush: playing again")
  }

  /// Whether the Mac's Now Playing is playing, asked through osascript (JavaScript for Automation); nil when it
  /// couldn't be asked.
  nonisolated private static func nowPlaying() -> Bool? {
    let js = "ObjC.import('Foundation'); $.NSBundle.bundleWithPath('/System/Library/PrivateFrameworks/MediaRemote.framework/').load; $.NSClassFromString('MRNowPlayingRequest').localIsPlaying ? 'yes' : 'no'"
    let p = Process(), out = Pipe()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    p.arguments = ["-l", "JavaScript", "-e", js]
    p.standardOutput = out
    p.standardError = FileHandle.nullDevice
    guard (try? p.run()) != nil else { return nil }
    let said = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    p.waitUntilExit()
    return said == "yes" ? true : said == "no" ? false : nil
  }

  /// MediaRemote's commands: play 0, pause 1.
  private static let play: Int32 = 0, pause: Int32 = 1
  private typealias Send = @convention(c) (Int32, CFDictionary?) -> Bool
  private static let send: Send? = {
    guard let h = dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_NOW),
          let f = dlsym(h, "MRMediaRemoteSendCommand") else { return nil }
    return unsafeBitCast(f, to: Send.self)
  }()

  private static func command(_ c: Int32) -> Bool { send?(c, nil) ?? false }

  /// Another app is sending sound out now (not this helper, and not Cue's own voices).
  static func othersPlaying() -> Bool {
    guard #available(macOS 14.2, *) else { return false }
    let me = getpid()
    return processes().contains { p in
      guard running(p), let pid = pid(p), pid != me else { return false }
      var name = [CChar](repeating: 0, count: 256)
      proc_name(pid, &name, UInt32(name.count))
      return !String(cString: name).hasPrefix("cue-")
    }
  }

  @available(macOS 14.2, *)
  private static func processes() -> [AudioObjectID] {
    var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyProcessObjectList, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size) == noErr, size > 0 else { return [] }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &ids) == noErr else { return [] }
    return ids
  }

  @available(macOS 14.2, *)
  private static func running(_ p: AudioObjectID) -> Bool {
    var addr = AudioObjectPropertyAddress(mSelector: kAudioProcessPropertyIsRunningOutput, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var on: UInt32 = 0
    var size = UInt32(MemoryLayout<UInt32>.size)
    return AudioObjectGetPropertyData(p, &addr, 0, nil, &size, &on) == noErr && on != 0
  }

  @available(macOS 14.2, *)
  private static func pid(_ p: AudioObjectID) -> pid_t? {
    var addr = AudioObjectPropertyAddress(mSelector: kAudioProcessPropertyPID, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var pid: pid_t = 0
    var size = UInt32(MemoryLayout<pid_t>.size)
    return AudioObjectGetPropertyData(p, &addr, 0, nil, &size, &pid) == noErr ? pid : nil
  }
}
