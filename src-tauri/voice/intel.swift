// cue-voice on an Intel Mac: NVIDIA's model runs on Apple silicon only, so this one just says so (Settings → Voice
// shows it) and takes no keys.
import Foundation

print(#"{"status":{"key":true,"model":"failed: it needs a Mac with Apple silicon","progress":0}}"#)
fflush(stdout)
while readLine() != nil {}
