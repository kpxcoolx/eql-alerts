import AVFoundation
import AppKit
import Foundation

// Required so NSSound/AVSpeech produce audible output when launched as a
// headless child of the Tauri process (no window / dock icon).
let app = NSApplication.shared
app.setActivationPolicy(.accessory)

func clampVolume(_ v: Float) -> Float {
    if v.isNaN { return 1.0 }
    return min(1.0, max(0.0, v))
}

var args = Array(CommandLine.arguments.dropFirst())
var volume: Float = 1.0
if args.first == "--volume", args.count >= 2 {
    volume = clampVolume(Float(args[1]) ?? 1.0)
    args.removeFirst(2)
}

guard let path = args.first else {
    fputs("usage: eql-speak [--volume 0-1] <audio-file> | eql-speak [--volume 0-1] --speak <text> [female|male]\n", stderr)
    exit(2)
}

func pickVoice(gender: String) -> AVSpeechSynthesisVoice? {
    let voices = AVSpeechSynthesisVoice.speechVoices().filter { $0.language.hasPrefix("en") }

    // Prefer Apple Eloquence (clearer short callouts) over classic compact voices.
    let preferIds: [String]
    let preferNames: [String]
    if gender == "male" {
        preferIds = [
            "com.apple.eloquence.en-US.Rocko",
            "com.apple.eloquence.en-US.Reed",
            "com.apple.eloquence.en-US.Eddy",
            "com.apple.voice.compact.en-GB.Daniel",
        ]
        preferNames = ["Rocko", "Reed", "Eddy", "Daniel", "Fred", "Albert", "Aaron"]
    } else {
        preferIds = [
            "com.apple.eloquence.en-US.Sandy",
            "com.apple.eloquence.en-US.Shelley",
            "com.apple.eloquence.en-US.Flo",
            "com.apple.voice.compact.en-US.Samantha",
        ]
        preferNames = ["Sandy", "Shelley", "Flo", "Samantha", "Kathy", "Karen", "Zoe"]
    }

    for id in preferIds {
        if let v = voices.first(where: { $0.identifier == id }) {
            return v
        }
    }
    for name in preferNames {
        if let v = voices.first(where: {
            $0.name.localizedCaseInsensitiveCompare(name) == .orderedSame
                && $0.language.hasPrefix("en-US")
        }) {
            return v
        }
        if let v = voices.first(where: { $0.name.localizedCaseInsensitiveContains(name) && $0.language.hasPrefix("en") }) {
            return v
        }
    }
    return AVSpeechSynthesisVoice(language: "en-US")
}

if path == "--speak" {
    let text = args.count > 1 ? args[1] : "Alert test"
    let gender = (args.count > 2 ? args[2] : "female").lowercased()
    let synth = AVSpeechSynthesizer()
    let u = AVSpeechUtterance(string: text)
    u.volume = volume
    // Slightly brisk for DBM-style alerts without slurring.
    u.rate = AVSpeechUtteranceDefaultSpeechRate * 1.05
    u.voice = pickVoice(gender: gender)
    let voiceName = u.voice?.name ?? "default"
    let voiceId = u.voice?.identifier ?? ""
    fputs("voice=\(voiceName) id=\(voiceId) gender=\(gender)\n", stderr)

    final class Waiter: NSObject, AVSpeechSynthesizerDelegate {
        var done = false
        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
            done = true
        }
        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
            done = true
        }
    }

    let waiter = Waiter()
    synth.delegate = waiter
    synth.speak(u)
    let deadline = Date().addingTimeInterval(45)
    while !waiter.done && Date() < deadline {
        RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05))
    }
    fputs(waiter.done ? "speak-ok vol=\(volume)\n" : "speak-timeout\n", stderr)
    exit(waiter.done ? 0 : 1)
}

let url = URL(fileURLWithPath: path)
guard FileManager.default.fileExists(atPath: path) else {
    fputs("missing file: \(path)\n", stderr)
    exit(1)
}

// Prefer AVAudioPlayer for WAV/AIFF — routes like system sounds on surround outputs.
do {
    let player = try AVAudioPlayer(contentsOf: url)
    player.volume = volume
    player.prepareToPlay()
    guard player.play() else {
        fputs("AVAudioPlayer.play returned false\n", stderr)
        // fall through to NSSound
        throw NSError(domain: "eql-speak", code: 1)
    }
    while player.isPlaying {
        RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05))
    }
    // Let CoreAudio flush to surround devices before exiting.
    Thread.sleep(forTimeInterval: 0.15)
    fputs("avaudio-ok vol=\(volume)\n", stderr)
    exit(0)
} catch {
    fputs("avaudio fallback: \(error.localizedDescription)\n", stderr)
}

if let sound = NSSound(contentsOf: url, byReference: true) {
    sound.volume = volume
    guard sound.play() else {
        fputs("NSSound.play returned false\n", stderr)
        exit(1)
    }
    while sound.isPlaying {
        RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05))
    }
    fputs("nssound-ok vol=\(volume)\n", stderr)
    exit(0)
}

fputs("no player could open: \(path)\n", stderr)
exit(1)
