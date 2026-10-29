import Foundation
import AVFoundation

struct VoiceSettings: Codable, Equatable {
    var kitID: String = ""
    var samplePath: String = ""
    var steps = Array(repeating: false, count: 16)
    var volume: Float = 0.8
    var muted = false
    var solo = false
}

/// One serial control queue owns the engine. A short lookahead schedules against
/// the audio host clock, independently of SwiftUI drawing and filesystem scans.
final class AudioEngine {
    private let queue = DispatchQueue(label: "net.oliver.rample.audio", qos: .userInteractive)
    private let engine = AVAudioEngine()
    private var players = (0..<4).map { _ in AVAudioPlayerNode() }
    private var previews = (0..<4).map { _ in AVAudioPlayerNode() }
    private var mixers = (0..<4).map { _ in AVAudioMixerNode() }
    private var files = [AVAudioFile?](repeating: nil, count: 4)
    private var settings = (0..<4).map { _ in VoiceSettings() }
    private var timer: DispatchSourceTimer?
    private var running = false
    private var generation = 0
    private var nextTime = 0.0
    private var nextStep = 0
    private var bpm = 120.0
    var onStep: ((Int) -> Void)?
    var onLevel: ((Float) -> Void)?
    var onError: ((String) -> Void)?
    var onLoaded: ((Int, String, [Float]) -> Void)?
    private var configurationObserver: NSObjectProtocol?
    private var lastMeterTime = 0.0

    init() {
        for i in 0..<4 {
            engine.attach(players[i]); engine.attach(previews[i]); engine.attach(mixers[i])
            engine.connect(players[i], to: mixers[i], format: nil)
            engine.connect(previews[i], to: mixers[i], format: nil)
            engine.connect(mixers[i], to: engine.mainMixerNode, format: nil)
        }
        // Leave ample headroom for four simultaneous full-scale samples.
        engine.mainMixerNode.outputVolume = 0.4
        engine.mainMixerNode.installTap(onBus: 0, bufferSize: 512, format: nil) { [weak self] buffer, _ in
            guard let self, let data = buffer.floatChannelData else { return }
            let now = Self.now
            guard now - self.lastMeterTime > 0.04 else { return }
            self.lastMeterTime = now
            var peak: Float = 0
            for ch in 0..<Int(buffer.format.channelCount) {
                for frame in 0..<Int(buffer.frameLength) { peak = max(peak, abs(data[ch][frame])) }
            }
            let level = peak
            DispatchQueue.main.async { [weak self] in self?.onLevel?(level) }
        }
        configurationObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self] _ in
            self?.queue.async { [weak self] in self?.recoverDevice() }
        }
    }

    private func startEngine() throws {
        if !engine.isRunning { engine.prepare(); try engine.start() }
    }

    func load(_ sample: Sample?, channel: Int) {
        queue.async { [self] in
            players[channel].stop(); previews[channel].stop(); files[channel] = nil
            guard let sample, sample.readable else {
                DispatchQueue.main.async { self.onLoaded?(channel, "", []) }
                return
            }
            do {
                let file = try AVAudioFile(forReading: sample.url)
                // AVAudioFile streams from disk; long files never get loaded wholesale.
                engine.disconnectNodeOutput(players[channel]); engine.disconnectNodeOutput(previews[channel])
                engine.connect(players[channel], to: mixers[channel], format: file.processingFormat)
                engine.connect(previews[channel], to: mixers[channel], format: file.processingFormat)
                files[channel] = file
                try startEngine()
                players[channel].play()
                let peaks = Self.waveform(sample.url)
                DispatchQueue.main.async { self.onLoaded?(channel, sample.id, peaks) }
            } catch { report("Could not play \(sample.url.lastPathComponent): \(error.localizedDescription)") }
        }
    }

    func update(_ voices: [VoiceSettings], bpm: Double) {
        queue.async { [self] in
            settings = voices; self.bpm = min(300, max(30, bpm))
            let anySolo = voices.contains { $0.solo }
            for i in 0..<4 {
                mixers[i].outputVolume = voices[i].muted || (anySolo && !voices[i].solo) ? 0 : voices[i].volume
            }
        }
    }

    func play() {
        queue.async { [self] in
            guard !running else { return }
            do { try startEngine() } catch { report(error.localizedDescription); return }
            running = true; generation += 1; nextStep = 0
            nextTime = Self.now + 0.09
            for player in players { player.stop(); player.play() }
            let source = DispatchSource.makeTimerSource(queue: queue)
            source.schedule(deadline: .now(), repeating: .milliseconds(10), leeway: .milliseconds(1))
            source.setEventHandler { [weak self] in self?.tick() }
            timer = source; source.resume()
        }
    }

    func stop() {
        queue.async { [self] in
            running = false; generation += 1; timer?.cancel(); timer = nil
            players.forEach { $0.stop() }; previews.forEach { $0.stop() }
            DispatchQueue.main.async { self.onStep?(-1) }
        }
    }

    func preview(_ channel: Int) {
        queue.async { [self] in
            guard let file = files[channel] else { return }
            do {
                try startEngine()
                let node = previews[channel]
                node.stop(); node.scheduleFile(file, at: nil); node.play()
            } catch { report(error.localizedDescription) }
        }
    }

    private func tick() {
        guard running else { return }
        let now = Self.now
        // Never emit a burst of late triggers after sleep or a lengthy device change.
        if nextTime < now - 0.15 { nextTime = now + 0.03 }
        while nextTime < now + 0.07 {
            let step = nextStep, at = nextTime, token = generation
            let interval = 60.0 / bpm / 4.0
            let time = AVAudioTime(hostTime: AVAudioTime.hostTime(forSeconds: at))
            for i in 0..<4 where settings[i].steps[step] {
                guard let file = files[i] else { continue }
                // End each one-shot at its next trigger: monophonic per voice.
                let distance = (1...16).first { settings[i].steps[(step + $0) % 16] } ?? 16
                let frames = min(file.length, AVAudioFramePosition(Double(distance) * interval * file.processingFormat.sampleRate))
                guard frames > 0 else { continue }
                players[i].scheduleSegment(file, startingFrame: 0, frameCount: AVAudioFrameCount(min(frames, AVAudioFramePosition(UInt32.max))), at: time)
            }
            queue.asyncAfter(deadline: .now() + max(0, at - now)) { [weak self] in
                guard let self, self.running, self.generation == token else { return }
                DispatchQueue.main.async { self.onStep?(step) }
            }
            nextStep = (nextStep + 1) % 16; nextTime += interval
        }
    }

    private func recoverDevice() {
        let wasRunning = running
        running = false; generation += 1; timer?.cancel(); timer = nil
        players.forEach { $0.stop() }; previews.forEach { $0.stop() }
        do {
            try startEngine()
            players.forEach { $0.play() }
            if wasRunning { play() }
        } catch { report("The audio output changed: \(error.localizedDescription)") }
    }

    private func report(_ message: String) {
        DispatchQueue.main.async { self.onError?(message) }
    }

    deinit {
        timer?.cancel()
        engine.stop()
        engine.mainMixerNode.removeTap(onBus: 0)
        if let configurationObserver { NotificationCenter.default.removeObserver(configurationObserver) }
    }

    private static var now: Double { AVAudioTime.seconds(forHostTime: mach_absolute_time()) }

    static func waveform(_ url: URL) -> [Float] {
        guard let file = try? AVAudioFile(forReading: url), file.length > 0,
              let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: 512) else { return [] }
        var peaks: [Float] = []
        for i in 0..<100 {
            file.framePosition = min(file.length - 1, AVAudioFramePosition(Double(i) / 100 * Double(file.length)))
            do { try file.read(into: buffer, frameCount: AVAudioFrameCount(min(512, file.length - file.framePosition))) } catch { return [] }
            var peak: Float = 0
            if let data = buffer.floatChannelData {
                for ch in 0..<Int(buffer.format.channelCount) {
                    for frame in 0..<Int(buffer.frameLength) { peak = max(peak, abs(data[ch][frame])) }
                }
            }
            peaks.append(peak)
        }
        let highest = max(peaks.max() ?? 1, 0.001)
        return peaks.map { $0 / highest }
    }
}
