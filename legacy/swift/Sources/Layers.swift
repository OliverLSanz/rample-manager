import Foundation

enum LayerMode: String, Codable, CaseIterable, Identifiable {
    case single = "manual"
    case random
    case sequential = "cyclic"
    case reverse = "reverseCyclic"
    case velocity

    var id: String { rawValue }
    var title: String {
        switch self {
        case .single: return "Single sample"
        case .random: return "Random"
        case .sequential: return "Sequential"
        case .reverse: return "Reverse sequential"
        case .velocity: return "Velocity"
        }
    }
    var detail: String {
        switch self {
        case .single: return "MANUAL: play only the selected sample on every trigger."
        case .random: return "RANDOM: choose a layer on each trigger. Repeats are possible."
        case .sequential: return "CYCLIC: advance through layers in filename order, then wrap."
        case .reverse: return "REVERSE CYCLIC: step backwards through layers, then wrap."
        case .velocity: return "VELOCITY: choose a layer from each step's velocity. Option-click a step to edit it."
        }
    }
}

struct VoiceSettings: Codable, Equatable {
    var kitID = ""
    var samplePath = ""
    var steps = Array(repeating: false, count: 16)
    var volume: Float = 0.8
    var muted = false
    var solo = false
    var layerMode = LayerMode.single
    var velocities = Array(repeating: 100, count: 16)

    init() {}

    private enum CodingKeys: String, CodingKey {
        case kitID, samplePath, steps, volume, muted, solo, layerMode, velocities
    }

    // Old sessions lack layerMode and velocities. Migrate them without losing
    // the selected files, pattern, BPM or mixer; single sample is always default.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kitID = try c.decodeIfPresent(String.self, forKey: .kitID) ?? ""
        samplePath = try c.decodeIfPresent(String.self, forKey: .samplePath) ?? ""
        let savedSteps = try c.decodeIfPresent([Bool].self, forKey: .steps) ?? []
        steps = Array((savedSteps + Array(repeating: false, count: 16)).prefix(16))
        let savedVolume = try c.decodeIfPresent(Float.self, forKey: .volume) ?? 0.8
        volume = savedVolume.isFinite ? min(1, max(0, savedVolume)) : 0.8
        muted = try c.decodeIfPresent(Bool.self, forKey: .muted) ?? false
        solo = try c.decodeIfPresent(Bool.self, forKey: .solo) ?? false
        let mode = try c.decodeIfPresent(String.self, forKey: .layerMode)
        layerMode = mode.flatMap(LayerMode.init(rawValue:)) ?? .single
        let savedVelocities = try c.decodeIfPresent([Int].self, forKey: .velocities) ?? []
        velocities = Array((savedVelocities + Array(repeating: 100, count: 16)).prefix(16)).map { min(127, max(1, $0)) }
    }
}

/// State is independent per voice. Use paths rather than indexes so adding or
/// removing another layer doesn't silently move the cursor to another sample.
struct LayerSelector {
    private(set) var lastPath: String?
    mutating func reset() { lastPath = nil }

    mutating func choose(paths: [String], selected: String, mode: LayerMode, velocity: Int,
                         random: (Int) -> Int = { Int.random(in: 0..<$0) }) -> String? {
        guard !paths.isEmpty, !selected.isEmpty else { return nil }
        let anchor = paths.firstIndex(of: selected) ?? 0
        let previous = lastPath.flatMap { paths.firstIndex(of: $0) }
        let index: Int
        switch mode {
        case .single:
            guard let selectedIndex = paths.firstIndex(of: selected) else { return nil }
            index = selectedIndex
        case .random: index = min(paths.count - 1, max(0, random(paths.count)))
        case .sequential: index = previous.map { ($0 + 1) % paths.count } ?? anchor
        case .reverse: index = previous.map { ($0 + paths.count - 1) % paths.count } ?? anchor
        case .velocity:
            // Preview uses even zones across the MIDI velocity range. Exact
            // hardware zone boundaries require a device comparison.
            index = min(paths.count - 1, min(127, max(1, velocity)) * paths.count / 128)
        }
        lastPath = paths[index]
        return paths[index]
    }
}

struct KitVoiceConfiguration: Codable, Equatable {
    var sampleName: String
    var layerMode: LayerMode
}

/// App-only memory. These JSON values are never presented as native .rpl saves.
struct KitConfigurations: Codable, Equatable {
    private var libraries: [String: [String: [String: KitVoiceConfiguration]]] = [:]

    func configuration(root: URL, kit: String, channel: Int) -> KitVoiceConfiguration? {
        libraries[root.standardizedFileURL.path]?[kit]?[String(channel)]
    }
    mutating func remember(root: URL, kit: String, channel: Int, settings: VoiceSettings) {
        guard !kit.isEmpty else { return }
        let name = settings.samplePath.isEmpty ? "" : URL(fileURLWithPath: settings.samplePath).lastPathComponent
        libraries[root.standardizedFileURL.path, default: [:]][kit, default: [:]][String(channel)] = KitVoiceConfiguration(sampleName: name, layerMode: settings.layerMode)
    }
}
