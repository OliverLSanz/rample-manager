import Foundation
import AVFoundation

/// One serial control queue owns the engine. A short lookahead schedules against
/// the audio host clock, independently of SwiftUI drawing and filesystem scans.
final class AudioEngine {
    private let queue = DispatchQueue(label: "net.oliver.rample.audio", qos: .userInteractive)
    private let engine = AVAudioEngine()
    private final class Stream {
        let sample: Sample
        let file: AVAudioFile
        let player = AVAudioPlayerNode()
        let preview = AVAudioPlayerNode()
        init(sample: Sample) throws { self.sample = sample; file = try AVAudioFile(forReading: sample.url) }
    }
    private var streams = [[Stream]](repeating: [], count: 4)
    private var mixers = (0..<4).map { _ in AVAudioMixerNode() }
    private var selectedPaths = [String](repeating: "", count: 4)
    private var selectors = (0..<4).map { _ in LayerSelector() }
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
    var onTrigger: ((Int, String) -> Void)?
    private var configurationObserver: NSObjectProtocol?
    private var lastMeterTime = 0.0

    init() {
        for i in 0..<4 {
            engine.attach(mixers[i])
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
        loadLayers(sample.map { [$0] } ?? [], selectedPath: sample?.id ?? "", channel: channel)
    }

    func loadLayers(_ samples: [Sample], selectedPath: String, channel: Int) {
        queue.async { [self] in
            let playable = samples.filter(\.readable)
            let old = streams[channel]
            let identifiers = playable.map(\.id)
            if old.map({ $0.sample.id }) != identifiers || selectedPaths[channel] != selectedPath { selectors[channel].reset() }
            selectedPaths[channel] = selectedPath
            // Reuse unchanged streams: editing an unrelated layer must not cut
            // off a currently sounding sample. Removed/edited layers stop now.
            for stream in old where !playable.contains(where: { $0.id == stream.sample.id && $0.revision == stream.sample.revision }) {
                stream.player.stop(); stream.preview.stop()
                engine.detach(stream.player); engine.detach(stream.preview)
            }
            streams[channel] = []
            do {
                for sample in playable {
                    if let reused = old.first(where: { $0.sample.id == sample.id && $0.sample.revision == sample.revision }) {
                        streams[channel].append(reused); continue
                    }
                    // Each layer has its own disk stream and format. No PCM
                    // conversion or loading entire samples into memory.
                    let stream = try Stream(sample: sample)
                    engine.attach(stream.player); engine.attach(stream.preview)
                    engine.connect(stream.player, to: mixers[channel], format: stream.file.processingFormat)
                    engine.connect(stream.preview, to: mixers[channel], format: stream.file.processingFormat)
                    streams[channel].append(stream)
                    if running { stream.player.play() }
                }
                let selected = playable.first { $0.id == selectedPath }
                let peaks = selected.map { Self.waveform($0.url) } ?? []
                DispatchQueue.main.async { self.onLoaded?(channel, selected?.id ?? "", peaks) }
            } catch { report("Could not load voice \(channel + 1): \(error.localizedDescription)") }
        }
    }

    func update(_ voices: [VoiceSettings], bpm: Double) {
        queue.async { [self] in
            for i in 0..<4 where voices[i].layerMode != settings[i].layerMode { selectors[i].reset() }
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
            for i in 0..<4 {
                selectors[i].reset()
                for stream in streams[i] { stream.player.stop(); stream.player.play() }
            }
            let source = DispatchSource.makeTimerSource(queue: queue)
            source.schedule(deadline: .now(), repeating: .milliseconds(10), leeway: .milliseconds(1))
            source.setEventHandler { [weak self] in self?.tick() }
            timer = source; source.resume()
        }
    }

    func stop() {
        queue.async { [self] in
            running = false; generation += 1; timer?.cancel(); timer = nil
            for stream in streams.flatMap({ $0 }) { stream.player.stop(); stream.preview.stop() }
            DispatchQueue.main.async { self.onStep?(-1) }
        }
    }

    func preview(_ channel: Int) {
        queue.async { [self] in
            guard let stream = chooseStream(channel, velocity: settings[channel].velocities[0]) else { return }
            do {
                try startEngine()
                streams[channel].forEach { $0.preview.stop() }
                stream.preview.scheduleFile(stream.file, at: nil); stream.preview.play()
                DispatchQueue.main.async { self.onTrigger?(channel, stream.sample.id) }
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
            var triggered: [(Int, String)] = []
            for i in 0..<4 where settings[i].steps[step] {
                guard let stream = chooseStream(i, velocity: settings[i].velocities[step]) else { continue }
                let file = stream.file
                // End each one-shot at its next trigger: monophonic per voice.
                let distance = (1...16).first { settings[i].steps[(step + $0) % 16] } ?? 16
                let frames = min(file.length, AVAudioFramePosition(Double(distance) * interval * file.processingFormat.sampleRate))
                guard frames > 0 else { continue }
                stream.player.scheduleSegment(file, startingFrame: 0, frameCount: AVAudioFrameCount(min(frames, AVAudioFramePosition(UInt32.max))), at: time)
                triggered.append((i, stream.sample.id))
            }
            queue.asyncAfter(deadline: .now() + max(0, at - now)) { [weak self] in
                guard let self, self.running, self.generation == token else { return }
                DispatchQueue.main.async {
                    self.onStep?(step)
                    for (channel, path) in triggered { self.onTrigger?(channel, path) }
                }
            }
            nextStep = (nextStep + 1) % 16; nextTime += interval
        }
    }

    private func chooseStream(_ channel: Int, velocity: Int) -> Stream? {
        guard let path = selectors[channel].choose(paths: streams[channel].map { $0.sample.id }, selected: selectedPaths[channel], mode: settings[channel].layerMode, velocity: velocity) else { return nil }
        return streams[channel].first { $0.sample.id == path }
    }

    private func recoverDevice() {
        let wasRunning = running
        running = false; generation += 1; timer?.cancel(); timer = nil
        for stream in streams.flatMap({ $0 }) { stream.player.stop(); stream.preview.stop() }
        do {
            try startEngine()
            for stream in streams.flatMap({ $0 }) { stream.player.play() }
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
