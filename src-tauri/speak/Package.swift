// swift-tools-version:5.9
// cue-speak: Listen's natural voices (Kokoro, through FluidAudio), on this Mac. build.rs builds it into binaries/,
// which tauri.conf.json bundles next to Cue.
import PackageDescription

let package = Package(
  name: "cue-speak",
  platforms: [.macOS(.v14)],
  dependencies: [
    .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7"),
  ],
  targets: [
    // Its pure parts (when each word is heard), tested with `swift test`.
    .target(name: "SpeakCore"),
    .executableTarget(name: "cue-speak", dependencies: ["SpeakCore", .product(name: "FluidAudio", package: "FluidAudio")]),
    .testTarget(name: "SpeakCoreTests", dependencies: ["SpeakCore"]),
  ]
)
