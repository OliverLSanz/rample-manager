import AppKit
import Combine
import Foundation

struct SavedSession: Codable {
    var rootPath: String
    var selectedKit: String
    var bpm: Double
    var voices: [VoiceSettings]
}

enum KitExporter {
    static func export(samples: [Sample?], parent: URL, name: String) throws -> URL {
        guard CatalogScanner.isKitName(name) else { throw problem("Use a letter from A to Z and a number from 0 to 99, for example B12.") }
        guard samples.count == 4, let first = samples[0], first.readable else { throw problem("The kit needs a sample in voice 1.") }
        let chosen = samples.compactMap { $0 }
        guard chosen.allSatisfy({ $0.readable && $0.warnings.isEmpty }) else { throw problem("Some WAV files are not Rample-compatible. Check the warnings for each voice.") }
        guard chosen.allSatisfy({ $0.channels == 1 }) else { throw problem("Use mono samples to export this four-voice kit. Stereo samples use two hardware voices; you can audition them here, but this export does not rearrange them.") }
        let fm = FileManager.default
        let target = parent.appendingPathComponent(name, isDirectory: true)
        guard !fm.fileExists(atPath: target.path) else { throw problem("\(name) already exists. Choose another number; existing kits are never overwritten.") }
        let container = parent.appendingPathComponent(".rample-export-\(UUID().uuidString)", isDirectory: true)
        let staging = container.appendingPathComponent(name, isDirectory: true)
        try fm.createDirectory(at: staging, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: container) }
        for (i, sample) in samples.enumerated() {
            guard let sample else { continue }
            let filename = "\(i + 1)" + sample.url.lastPathComponent.dropFirst()
            try fm.copyItem(at: sample.url, to: staging.appendingPathComponent(filename))
        }
        // Validate the copies too, in case an editor changed a source during export.
        let copied = CatalogScanner().scan(container)
        guard let kit = copied.kits.first, kit.samples.count == chosen.count,
              kit.samples.allSatisfy({ $0.readable && $0.warnings.isEmpty && $0.channels == 1 }) else {
            throw problem("A file changed during export or a copy is invalid. Please try again.")
        }
        try fm.moveItem(at: staging, to: target)
        return target
    }

    static func problem(_ message: String) -> NSError {
        NSError(domain: "RamplePreview", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}

final class OutputMeter: ObservableObject {
    @Published var level: Float = 0
}

final class AppModel: ObservableObject {
    @Published var root: URL?
    @Published var kits: [Kit] = []
    @Published var selectedKit = ""
    @Published var bankFilter = "All"
    @Published var search = ""
    @Published var bpm = 120.0 { didSet { syncAudio(); saveSoon() } }
    @Published var voices = (0..<4).map { _ in VoiceSettings() }
    @Published var waveforms = [[Float]](repeating: [], count: 4)
    @Published var loadedPaths = [String](repeating: "", count: 4)
    @Published var isPlaying = false
    @Published var playhead = -1
    let meter = OutputMeter()
    @Published var scanning = false
    @Published var watching = false
    @Published var lastUpdated: Date?
    @Published var alert: String?
    @Published var folderError: String?
    @Published var exportPresented = false
    @Published var exportName = "B0"
    @Published var isDemo = false
    @Published var statusMessage = ""

    let audio = AudioEngine()
    private let scanner = CatalogScanner()
    private let scanQueue = DispatchQueue(label: "net.oliver.rample.scan", qos: .userInitiated)
    private let watcher = FolderWatcher()
    private var refreshWork: DispatchWorkItem?
    private var saveWork: DispatchWorkItem?
    private var fallback: Timer?
    private var revisionByChannel = [String](repeating: "", count: 4)
    private var rootGeneration = 0
    private var scanAgain = false
    private var forceNextScan = false
    private var loadedSession: SavedSession?
    private var notificationObserver: NSObjectProtocol?

    static var support: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Rample Preview", isDirectory: true)
    }
    private var sessionURL: URL { Self.support.appendingPathComponent("session.json") }

    init() {
        try? FileManager.default.createDirectory(at: Self.support, withIntermediateDirectories: true)
        if let data = try? Data(contentsOf: sessionURL), let session = try? JSONDecoder().decode(SavedSession.self, from: data), session.voices.count == 4 {
            loadedSession = session
            bpm = min(300, max(30, session.bpm.isFinite ? session.bpm : 120))
            voices = session.voices.map { voice in
                var safe = voice
                safe.steps = Array((voice.steps + Array(repeating: false, count: 16)).prefix(16))
                safe.volume = min(1, max(0, voice.volume.isFinite ? voice.volume : 0.8))
                return safe
            }
            selectedKit = session.selectedKit
        }
        audio.onStep = { [weak self] step in self?.playhead = step }
        audio.onLevel = { [weak self] level in
            guard let self else { return }
            if abs(self.meter.level - level) > 0.003 || (level == 0 && self.meter.level != 0) { self.meter.level = level }
        }
        audio.onError = { [weak self] message in self?.alert = message; self?.isPlaying = false; self?.audio.stop() }
        audio.onLoaded = { [weak self] channel, path, peaks in
            guard let self, self.voices[channel].samplePath == path || path.isEmpty else { return }
            self.loadedPaths[channel] = path; self.waveforms[channel] = peaks
        }
        watcher.onChange = { [weak self] in self?.scheduleRefresh() }
        // A light metadata fallback catches disconnected volumes and filesystems
        // that do not deliver FSEvents. Header reads are cached between scans.
        fallback = Timer.scheduledTimer(withTimeInterval: 4, repeats: true) { [weak self] _ in self?.refresh() }
        notificationObserver = NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in self?.refresh(force: true) }
        syncAudio()
        if let path = loadedSession?.rootPath, !path.isEmpty {
            setRoot(URL(fileURLWithPath: path), restoring: true)
        } else { showDemo() }
    }

    var banks: [String] { Array(Set(kits.map(\.bank))).sorted() }
    var visibleKits: [Kit] {
        kits.filter { (bankFilter == "All" || $0.bank == bankFilter) && (search.isEmpty || $0.id.localizedCaseInsensitiveContains(search) || $0.samples.contains { $0.name.localizedCaseInsensitiveContains(search) }) }
    }
    var sampleCount: Int { kits.reduce(0) { $0 + $1.samples.count } }
    var activeKit: Kit? { kits.first { $0.id == selectedKit } }
    var selectionWarnings: [String] {
        Array(Set((activeKit?.warnings ?? []) + (0..<4).flatMap { sample($0)?.warnings ?? [] })).sorted()
    }
    func sourceKit(_ channel: Int) -> Kit? { kits.first { $0.id == voices[channel].kitID } }
    func choices(_ channel: Int) -> [Sample] { sourceKit(channel)?.samples(for: channel + 1) ?? [] }
    func sample(_ channel: Int) -> Sample? { choices(channel).first { $0.id == voices[channel].samplePath } }

    func chooseFolder() {
        let panel = NSOpenPanel()
        panel.title = "Select your SD root or sample library folder"
        panel.message = "It should directly contain folders such as A0, A1, B12…"
        panel.prompt = "Open folder"
        panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.allowsMultipleSelection = false
        if let root { panel.directoryURL = root }
        if panel.runModal() == .OK, let url = panel.url { setRoot(url) }
    }

    func setRoot(_ url: URL, restoring: Bool = false) {
        audio.stop(); isPlaying = false; playhead = -1
        rootGeneration += 1; root = url.standardizedFileURL
        isDemo = url.standardizedFileURL.path == Self.support.appendingPathComponent("Demo SD").path
        folderError = nil; kits = []; revisionByChannel = Array(repeating: "", count: 4)
        loadedPaths = Array(repeating: "", count: 4); waveforms = Array(repeating: [], count: 4)
        for channel in 0..<4 { audio.load(nil, channel: channel) }
        if !restoring {
            selectedKit = ""; bankFilter = "All"; search = ""
            for i in 0..<4 { voices[i].kitID = ""; voices[i].samplePath = "" }
        }
        watching = watcher.start(url)
        refresh(force: true)
        saveSoon()
    }

    func refresh(force: Bool = false) {
        guard let root else { return }
        if scanning { scanAgain = true; forceNextScan = forceNextScan || force; return }
        scanning = true
        let token = rootGeneration
        scanQueue.async { [weak self] in
            guard let self else { return }
            let result = self.scanner.scan(root, force: force)
            DispatchQueue.main.async {
                self.scanning = false
                if token == self.rootGeneration {
                    self.folderError = result.error
                    if self.kits != result.kits { self.kits = result.kits; self.lastUpdated = Date() }
                    if self.lastUpdated == nil { self.lastUpdated = Date() }
                    if self.selectedKit.isEmpty, let first = result.kits.first { self.selectKit(first.id) }
                    self.reloadChangedSamples(force: force)
                } else { self.scanAgain = true }
                if self.scanAgain { self.scanAgain = false; let force = self.forceNextScan; self.forceNextScan = false; self.refresh(force: force) }
            }
        }
    }

    private func scheduleRefresh() {
        refreshWork?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.refresh() }
        refreshWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: work)
    }

    func selectKit(_ id: String) {
        selectedKit = id
        for i in 0..<4 {
            voices[i].kitID = id
            voices[i].samplePath = choices(i).first(where: \.readable)?.id ?? ""
        }
        reloadChangedSamples(); syncAudio(); saveSoon()
    }

    func selectSource(_ id: String, channel: Int) {
        voices[channel].kitID = id
        voices[channel].samplePath = choices(channel).first(where: \.readable)?.id ?? ""
        reloadChangedSamples(); saveSoon()
    }

    func selectSample(_ path: String, channel: Int) {
        voices[channel].samplePath = path
        reloadChangedSamples(); saveSoon()
    }

    private func reloadChangedSamples(force: Bool = false) {
        for i in 0..<4 {
            let selected = sample(i)
            let key = selected.map { $0.id + $0.revision } ?? ""
            if force || key != revisionByChannel[i] {
                revisionByChannel[i] = key; waveforms[i] = []; loadedPaths[i] = ""
                audio.load(selected, channel: i)
            }
        }
    }

    func togglePlay() {
        if isPlaying { audio.stop(); isPlaying = false; playhead = -1 }
        else if (0..<4).contains(where: { sample($0)?.readable == true }) { syncAudio(); audio.play(); isPlaying = true }
        else { alert = "Select at least one readable WAV to play." }
    }

    func changeVoice(_ channel: Int, _ update: (inout VoiceSettings) -> Void) {
        update(&voices[channel]); syncAudio(); saveSoon()
    }
    func toggleStep(_ channel: Int, _ step: Int) { changeVoice(channel) { $0.steps[step].toggle() } }
    func clearPattern() {
        for i in 0..<4 { voices[i].steps = Array(repeating: false, count: 16) }
        syncAudio(); saveSoon()
    }
    func basicPattern() {
        let hits = [[0, 4, 8, 12], [4, 12], [0, 2, 4, 6, 8, 10, 12, 14], [6, 14]]
        for i in 0..<4 { voices[i].steps = (0..<16).map { hits[i].contains($0) } }
        syncAudio(); saveSoon()
    }
    func preview(_ channel: Int) { audio.preview(channel) }
    func revealRoot() { if let root { NSWorkspace.shared.open(root) } }
    func revealSample(_ channel: Int) { if let sample = sample(channel) { NSWorkspace.shared.activateFileViewerSelecting([sample.url]) } }

    func exportKit() {
        guard CatalogScanner.isKitName(exportName) else { alert = "Invalid kit name. Use A0–Z99."; return }
        let panel = NSOpenPanel()
        panel.title = "Export \(exportName): select the destination root folder"
        panel.message = "A new \(exportName) folder will contain a copy of each selected sample."
        panel.canChooseFiles = false; panel.canChooseDirectories = true; panel.canCreateDirectories = true
        panel.prompt = "Export kit"; panel.directoryURL = isDemo ? nil : root
        guard panel.runModal() == .OK, let parent = panel.url else { return }
        let samples = (0..<4).map { sample($0) }
        do {
            let result = try KitExporter.export(samples: samples, parent: parent, name: exportName)
            exportPresented = false; statusMessage = "Kit \(exportName) exported"
            NSWorkspace.shared.activateFileViewerSelecting([result]); refresh(force: true)
        } catch { alert = error.localizedDescription }
    }

    func showDemo() {
        do {
            let demo = try DemoSamples.create(in: Self.support)
            setRoot(demo)
            basicPattern()
        } catch { alert = "Could not create the demo kit: \(error.localizedDescription)" }
    }

    private func syncAudio() { audio.update(voices, bpm: bpm) }
    private func saveSoon() {
        saveWork?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.save() }
        saveWork = work; DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: work)
    }
    func save() {
        let session = SavedSession(rootPath: root?.path ?? "", selectedKit: selectedKit, bpm: bpm, voices: voices)
        do { let data = try JSONEncoder().encode(session); try data.write(to: sessionURL, options: .atomic) }
        catch { statusMessage = "Could not save the session: \(error.localizedDescription)" }
    }
}
