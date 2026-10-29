import XCTest
@testable import RamplePreview

final class LayerTests: XCTestCase {
    private let paths = ["1_A.wav", "1_B.wav", "1_C.wav"]

    func testSingleSampleIsDefaultAndNeverChangesLayer() {
        XCTAssertEqual(VoiceSettings().layerMode, .single)
        var selector = LayerSelector()
        for _ in 0..<20 {
            XCTAssertEqual(selector.choose(paths: paths, selected: paths[1], mode: .single, velocity: 127), paths[1])
        }
        XCTAssertNil(selector.choose(paths: paths, selected: "removed.wav", mode: .single, velocity: 127))
        XCTAssertNil(selector.choose(paths: [], selected: paths[1], mode: .random, velocity: 127))
        XCTAssertNil(selector.choose(paths: paths, selected: "", mode: .random, velocity: 127))
    }

    func testSequentialStartsAtSelectionAndWrapsOnTriggers() {
        var selector = LayerSelector()
        let result = (0..<7).map { _ in selector.choose(paths: paths, selected: paths[1], mode: .sequential, velocity: 100) }
        XCTAssertEqual(result, [paths[1], paths[2], paths[0], paths[1], paths[2], paths[0], paths[1]])
        selector.reset()
        XCTAssertEqual(selector.choose(paths: paths, selected: paths[1], mode: .sequential, velocity: 100), paths[1])
    }

    func testReverseWrapsInReverseFilenameOrder() {
        var selector = LayerSelector()
        let result = (0..<5).map { _ in selector.choose(paths: paths, selected: paths[1], mode: .reverse, velocity: 100) }
        XCTAssertEqual(result, [paths[1], paths[0], paths[2], paths[1], paths[0]])
    }

    func testRandomCanChooseAnyLayerAndAllowsRepeats() {
        var selector = LayerSelector()
        let indexes = [0, 0, 2, 1]
        let result = indexes.map { index in selector.choose(paths: paths, selected: paths[1], mode: .random, velocity: 100, random: { _ in index }) }
        XCTAssertEqual(result, [paths[0], paths[0], paths[2], paths[1]])
    }

    func testVelocityExtremesMiddleAndAllTwelveZones() {
        var selector = LayerSelector()
        XCTAssertEqual(selector.choose(paths: paths, selected: paths[0], mode: .velocity, velocity: 1), paths[0])
        XCTAssertEqual(selector.choose(paths: paths, selected: paths[0], mode: .velocity, velocity: 64), paths[1])
        XCTAssertEqual(selector.choose(paths: paths, selected: paths[0], mode: .velocity, velocity: 127), paths[2])
        let twelve = (0..<12).map { "layer\($0)" }
        let selected = Set((1...127).compactMap { selector.choose(paths: twelve, selected: twelve[0], mode: .velocity, velocity: $0) })
        XCTAssertEqual(selected, Set(twelve))
    }

    func testAddingAndDeletingLayersKeepsCursorByIdentity() {
        var selector = LayerSelector()
        _ = selector.choose(paths: paths, selected: paths[1], mode: .sequential, velocity: 100)
        XCTAssertEqual(selector.choose(paths: ["new.wav"] + paths, selected: paths[1], mode: .sequential, velocity: 100), paths[2])
        XCTAssertEqual(selector.choose(paths: Array(paths.prefix(2)), selected: paths[1], mode: .sequential, velocity: 100), paths[1])
    }

    func testOldSessionMigratesWithoutLosingTheKitOrPattern() throws {
        let json = Data(#"{"rootPath":"/tmp/samples","selectedKit":"B12","bpm":137,"voices":[{"kitID":"B12","samplePath":"/tmp/samples/B12/1_kick.wav","steps":[true,false,true],"volume":0.6,"muted":true,"solo":false}]}"#.utf8)
        let old = try JSONDecoder().decode(SavedSession.self, from: json)
        let voice = try XCTUnwrap(old.voices.first)
        XCTAssertEqual(old.bpm, 137)
        XCTAssertEqual(voice.kitID, "B12")
        XCTAssertEqual(voice.samplePath, "/tmp/samples/B12/1_kick.wav")
        XCTAssertEqual(voice.steps.count, 16)
        XCTAssertEqual(Array(voice.steps.prefix(3)), [true, false, true])
        XCTAssertEqual(voice.layerMode, .single)
        XCTAssertEqual(voice.velocities, Array(repeating: 100, count: 16))
        XCTAssertTrue(voice.muted)
        XCTAssertNil(old.kitConfigurations)
    }

    func testNewVoiceSettingsRoundTripAndSanitizeUnknownValues() throws {
        var voice = VoiceSettings()
        voice.layerMode = .velocity; voice.velocities[2] = 32
        XCTAssertEqual(try JSONDecoder().decode(VoiceSettings.self, from: JSONEncoder().encode(voice)), voice)
        let unknown = try JSONDecoder().decode(VoiceSettings.self, from: Data(#"{"layerMode":"future-mode","velocities":[0,128,-1]}"#.utf8))
        XCTAssertEqual(unknown.layerMode, .single)
        XCTAssertEqual(Array(unknown.velocities.prefix(4)), [1, 127, 1, 100])
    }

    func testPerKitModesAndSelectionsAreIndependentAndPersistent() throws {
        let library = URL(fileURLWithPath: "/tmp/library")
        let other = URL(fileURLWithPath: "/tmp/other-library")
        var store = KitConfigurations()
        var voice = VoiceSettings()
        voice.samplePath = "/tmp/library/A0/1_B.wav"; voice.layerMode = .reverse
        store.remember(root: library, kit: "A0", channel: 0, settings: voice)
        voice.samplePath = "/tmp/library/B12/1_X.wav"; voice.layerMode = .random
        store.remember(root: library, kit: "B12", channel: 0, settings: voice)
        let restored = try JSONDecoder().decode(KitConfigurations.self, from: JSONEncoder().encode(store))
        XCTAssertEqual(restored.configuration(root: library, kit: "A0", channel: 0), KitVoiceConfiguration(sampleName: "1_B.wav", layerMode: .reverse))
        XCTAssertEqual(restored.configuration(root: library, kit: "B12", channel: 0)?.layerMode, .random)
        XCTAssertNil(restored.configuration(root: library, kit: "A0", channel: 1))
        XCTAssertNil(restored.configuration(root: other, kit: "A0", channel: 0))
    }

    func testAudioEngineActuallyTriggersSelectedCyclicReverseAndVelocityLayers() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let kitURL = root.appendingPathComponent("A0")
        try FileManager.default.createDirectory(at: kitURL, withIntermediateDirectories: true)
        for i in 1...3 { try DemoSamples.wav(Array(repeating: Int16(800 * i), count: 4410)).write(to: kitURL.appendingPathComponent("1_layer\(i).wav")) }
        let samples = CatalogScanner().scan(root).kits[0].samples
        let expected: [(LayerMode, [Int])] = [(.single, [1, 1, 1, 1]), (.sequential, [1, 2, 0, 1]), (.reverse, [1, 0, 2, 1]), (.velocity, [0, 1, 2, 0])]
        for (mode, indexes) in expected {
            let engine = AudioEngine()
            engine.onError = { XCTFail($0) }
            let loaded = expectation(description: "Loaded \(mode)")
            engine.onLoaded = { _, _, _ in loaded.fulfill() }
            engine.loadLayers(samples, selectedPath: samples[1].id, channel: 0)
            wait(for: [loaded], timeout: 4)
            var voices = (0..<4).map { _ in VoiceSettings() }
            voices[0].layerMode = mode
            voices[0].steps = Array(repeating: true, count: 16)
            voices[0].velocities = [1, 64, 127, 1] + Array(repeating: 100, count: 12)
            engine.update(voices, bpm: 300)
            let triggered = expectation(description: "Audible layer selection \(mode)")
            var paths: [String] = []
            engine.onTrigger = { channel, path in
                guard channel == 0, paths.count < 4 else { return }
                paths.append(path)
                if paths.count == 4 { triggered.fulfill() }
            }
            engine.play(); wait(for: [triggered], timeout: 4); engine.stop()
            XCTAssertEqual(paths, indexes.map { samples[$0].id }, "\(mode)")
        }
    }
}
