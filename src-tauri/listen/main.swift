// cue-listen: dictation for Cue's text boxes, with Apple's on-device speech recognition.
//
// Cue starts it when you click the mic. It listens to the microphone and prints one JSON line per
// update on stdout: {"text": "what you said so far", "final": false}. Closing its stdin (Cue does,
// when you click the mic again or send) stops listening; it prints the final text and exits.
// Problems come out as {"error": "..."} and exit 1.

import AVFoundation
import Foundation
import Speech

func emit(_ obj: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: obj), let line = String(data: data, encoding: .utf8) else { return }
    FileHandle.standardOutput.write((line + "\n").data(using: .utf8)!)
}

func fail(_ message: String) -> Never {
    emit(["error": message])
    exit(1)
}

let engine = AVAudioEngine()
let request = SFSpeechAudioBufferRecognitionRequest()
var task: SFSpeechRecognitionTask?

func stopListening() {
    if engine.isRunning {
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
    }
    request.endAudio()
    // The final result normally lands within a moment; don't hang if it doesn't.
    DispatchQueue.main.asyncAfter(deadline: .now() + 3) { exit(0) }
}

func start() {
    let recognizer = SFSpeechRecognizer(locale: Locale.current) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))
    guard let recognizer, recognizer.isAvailable else { fail("Speech recognition isn't available right now.") }
    request.shouldReportPartialResults = true
    request.addsPunctuation = true
    if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }

    let input = engine.inputNode
    let format = input.outputFormat(forBus: 0)
    guard format.sampleRate > 0 else { fail("No microphone found.") }
    input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in request.append(buffer) }
    engine.prepare()
    do { try engine.start() } catch { fail("Couldn't start the microphone: \(error.localizedDescription)") }
    emit(["listening": true])

    task = recognizer.recognitionTask(with: request) { result, error in
        if let result {
            emit(["text": result.bestTranscription.formattedString, "final": result.isFinal])
            if result.isFinal { exit(0) }
        } else if let error {
            // Stopping before anything was said ends the task with an error: that's just "nothing heard".
            if !engine.isRunning { exit(0) }
            fail(error.localizedDescription)
        }
    }

    // Cue closes stdin to say "stop".
    DispatchQueue.global().async {
        while let line = readLine(), line.trimmingCharacters(in: .whitespaces) != "stop" {}
        DispatchQueue.main.async { stopListening() }
    }
}

SFSpeechRecognizer.requestAuthorization { status in
    guard status == .authorized else { fail("Cue isn't allowed to use speech recognition. Turn it on in System Settings → Privacy & Security → Speech Recognition.") }
    AVCaptureDevice.requestAccess(for: .audio) { granted in
        guard granted else { fail("Cue isn't allowed to use the microphone. Turn it on in System Settings → Privacy & Security → Microphone.") }
        DispatchQueue.main.async { start() }
    }
}

dispatchMain()
