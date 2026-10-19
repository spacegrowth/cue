// cue-speak on an Intel Mac: Kokoro runs on Apple silicon only, so this one just says it isn't there and Listen
// keeps the Mac's own voice.
import Foundation

while let line = readLine() {
  if line.contains("\"status\"") { print(#"{"status":{"ready":false,"risky":false,"none":true}}"#) }
  else { print(#"{"error":"Natural voices need a Mac with Apple silicon."}"#) }
  fflush(stdout)
}
