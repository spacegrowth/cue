// swift-tools-version:5.9
// cue-voice: talking to a session from anywhere on the Mac (talk.rs starts it inside Cue.app). build.rs builds it
// into binaries/, which tauri.conf.json bundles next to Cue.
import PackageDescription

let package = Package(
  name: "cue-voice",
  platforms: [.macOS(.v14)],
  dependencies: [
    // NVIDIA's Parakeet on the Mac (the same FluidAudio as cue-speak's).
    .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7"),
  ],
  targets: [
    // Its pure parts, tested with `swift test`.
    .target(name: "VoiceCore"),
    .executableTarget(name: "cue-voice", dependencies: ["VoiceCore", .product(name: "FluidAudio", package: "FluidAudio")]),
    .testTarget(name: "VoiceCoreTests", dependencies: ["VoiceCore"]),
  ]
)
