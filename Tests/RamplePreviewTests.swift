import XCTest
import AVFoundation
@testable import RamplePreview

final class RamplePreviewTests: XCTestCase {
    var root: URL!
    let fm = FileManager.default

    override func setUpWithError() throws {
        root = fm.temporaryDirectory.appendingPathComponent("rample-test-\(UUID().uuidString)")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
    }
    override func tearDownWithError() throws { try fm.removeItem(at: root) }

    @discardableResult func write(_ filename: String, kit: String = "A0", frames: Int = 4410, rate: UInt32 = 44100, value: Int16 = 1200) throws -> URL {
        let directory = root.appendingPathComponent(kit)
        try fm.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent(filename)
        try DemoSamples.wav(Array(repeating: value, count: frames), rate: rate).write(to: url)
        return url
    }

    func testKitNames() {
        for name in ["A0", "B12", "Z99", "A00"] { XCTAssertTrue(CatalogScanner.isKitName(name), name) }
        for name in ["a0", "A100", "ABC", "A-1", "A1/", "A1\n", "1A", "B"] { XCTAssertFalse(CatalogScanner.isKitName(name), name) }
    }

    func testRootLayoutVoicePrefixesAndNaturalLayerOrder() throws {
        try write("1_Kick10.wav"); try write("1_Kick2.WAV"); try write("2_Snare.wav")
        try write("4_Click.wav", kit: "A10"); try write("1_Kick.wav", kit: "A2")
        try write("1_ignored.wav", kit: "not-a-kit")
        try write("5_ignored.wav"); try write("no-prefix.wav")
        try write("1_nested.wav", kit: "A0/nested")
        let result = CatalogScanner().scan(root)
        XCTAssertNil(result.error)
        XCTAssertEqual(result.kits.map(\.id), ["A0", "A2", "A10"])
        XCTAssertEqual(result.kits[0].samples.map(\.name), ["1_Kick2", "1_Kick10", "2_Snare"])
        XCTAssertEqual(result.kits[0].samples(for: 2).count, 1)
        XCTAssertTrue(result.kits[0].warnings.isEmpty)
        XCTAssertFalse(result.kits[2].warnings.isEmpty)
    }

    func testFormatValidationAndCorruptWav() throws {
        try write("1_valid.wav")
        try write("2_rate.wav", rate: 48000)
        try write("3_short.wav", frames: 100)
        try Data("not a wav".utf8).write(to: root.appendingPathComponent("A0/4_broken.wav"))
        let samples = CatalogScanner().scan(root).kits[0].samples
        XCTAssertEqual(samples.count, 4)
        XCTAssertTrue(samples[0].warnings.isEmpty)
        XCTAssertEqual(samples[0].bits, 16)
        XCTAssertEqual(samples[0].duration, 0.1, accuracy: 0.0001)
        XCTAssertTrue(samples[1].warnings.joined().contains("44.1"))
        XCTAssertTrue(samples[2].warnings.joined().contains("50 ms"))
        XCTAssertFalse(samples[3].readable)
    }

    func testTwelveLayerLimit() throws {
        for n in 1...13 { try write("1_layer\(n).wav") }
        let kit = CatalogScanner().scan(root).kits[0]
        XCTAssertEqual(kit.samples.count, 13)
        XCTAssertTrue(kit.warnings.joined().contains("supports 12"))
    }

    func testSameSizeEditAdditionDeletionAndAtomicReplacement() throws {
        let url = try write("1_kick.wav")
        let scanner = CatalogScanner()
        let original = scanner.scan(root).kits[0].samples[0]
        try DemoSamples.wav(Array(repeating: Int16(2400), count: 4410)).write(to: url)
        let modified = scanner.scan(root).kits[0].samples[0]
        XCTAssertNotEqual(original.revision, modified.revision)
        try DemoSamples.wav(Array(repeating: Int16(3600), count: 4410)).write(to: url, options: .atomic)
        XCTAssertNotEqual(modified.revision, scanner.scan(root).kits[0].samples[0].revision)
        try write("2_snare.wav")
        XCTAssertEqual(scanner.scan(root).kits[0].samples.count, 2)
        try fm.removeItem(at: url)
        let remaining = scanner.scan(root).kits[0]
        XCTAssertEqual(remaining.samples.map(\.voice), [2])
        XCTAssertFalse(remaining.warnings.isEmpty)
    }

    func testSymlinkFilesAndFoldersAreNotFollowed() throws {
        let url = try write("1_kick.wav")
        try fm.createSymbolicLink(at: root.appendingPathComponent("B0"), withDestinationURL: root.appendingPathComponent("A0"))
        try fm.createSymbolicLink(at: root.appendingPathComponent("A0/2_link.wav"), withDestinationURL: url)
        let result = CatalogScanner().scan(root)
        XCTAssertEqual(result.kits.count, 1)
        XCTAssertEqual(result.kits[0].samples.count, 1)
    }

    func testMissingRootIsAnError() {
        XCTAssertNotNil(CatalogScanner().scan(root.appendingPathComponent("missing")).error)
    }

    func testExportCopiesAndNeverOverwrites() throws {
        let source = try write("1_kick.wav")
        try write("2_snare.wav")
        let before = try Data(contentsOf: source)
        let samples = CatalogScanner().scan(root).kits[0].samples
        let target = try KitExporter.export(samples: [samples[0], samples[1], nil, nil], parent: root, name: "B12")
        XCTAssertEqual(try Data(contentsOf: target.appendingPathComponent("1_kick.wav")), before)
        XCTAssertEqual(try Data(contentsOf: source), before)
        XCTAssertEqual(try fm.contentsOfDirectory(atPath: target.path).sorted(), ["1_kick.wav", "2_snare.wav"])
        XCTAssertThrowsError(try KitExporter.export(samples: [samples[0], samples[1], nil, nil], parent: root, name: "B12"))
        XCTAssertEqual(try Data(contentsOf: target.appendingPathComponent("1_kick.wav")), before)
        XCTAssertFalse(try fm.contentsOfDirectory(atPath: root.path).contains { $0.hasPrefix(".rample-export") })
    }

    func testExportRejectsInvalidKitAndIncompatibleSample() throws {
        try write("1_wrong.wav", rate: 48000)
        let sample = CatalogScanner().scan(root).kits[0].samples[0]
        XCTAssertThrowsError(try KitExporter.export(samples: [nil, nil, nil, nil], parent: root, name: "B0"))
        XCTAssertThrowsError(try KitExporter.export(samples: [sample, nil, nil, nil], parent: root, name: "../oops"))
        XCTAssertThrowsError(try KitExporter.export(samples: [sample, nil, nil, nil], parent: root, name: "B0"))
        XCTAssertFalse(fm.fileExists(atPath: root.appendingPathComponent("B0").path))
    }

    func testRecursiveWatcherSeesAnAtomicSaveInAKit() throws {
        let url = try write("1_kick.wav")
        let watcher = FolderWatcher()
        let changed = expectation(description: "FSEvents change inside kit")
        changed.assertForOverFulfill = false
        let replacement = DemoSamples.wav(Array(repeating: Int16(2500), count: 4410))
        watcher.onChange = {
            if (try? Data(contentsOf: url)) == replacement { changed.fulfill() }
        }
        XCTAssertTrue(watcher.start(root))
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            try? replacement.write(to: url, options: .atomic)
        }
        wait(for: [changed], timeout: 4)
        watcher.stop()
    }

    func testDemoWaveformsAndRealAudioTransport() throws {
        let demo = try DemoSamples.create(in: root)
        let kit = try XCTUnwrap(CatalogScanner().scan(demo).kits.first)
        XCTAssertEqual(kit.samples.count, 4)
        XCTAssertTrue(kit.samples.allSatisfy { $0.warnings.isEmpty })
        XCTAssertEqual(AudioEngine.waveform(kit.samples[0].url).count, 100)

        let engine = AudioEngine()
        let loaded = expectation(description: "Sample streamed and waveform loaded")
        let audible = expectation(description: "Actual engine mixer receives non-silent PCM")
        let steps = expectation(description: "Host-clock sequencer advances")
        var heardAudio = false
        var observedSteps: [Int] = []
        engine.onLoaded = { _, _, _ in loaded.fulfill() }
        engine.onError = { XCTFail($0) }
        engine.onLevel = { level in
            if level > 0.005 && !heardAudio { heardAudio = true; audible.fulfill() }
        }
        engine.onStep = { step in
            if step >= 0 {
                observedSteps.append(step)
                if observedSteps.count == 5 { steps.fulfill() }
            }
        }
        var voices = (0..<4).map { _ in VoiceSettings() }
        voices[0].steps[0] = true; voices[0].steps[4] = true
        engine.update(voices, bpm: 120)
        engine.load(kit.samples[0], channel: 0)
        wait(for: [loaded], timeout: 4)
        engine.play()
        wait(for: [audible, steps], timeout: 4)
        engine.stop()
        XCTAssertEqual(Array(observedSteps.prefix(5)), [0, 1, 2, 3, 4])
    }

    func testFourVoicesReloadAndMuteDuringPlayback() throws {
        for voice in 1...4 { try write("\(voice)_tone.wav", frames: 22050, value: 1200) }
        let scanner = CatalogScanner()
        let samples = scanner.scan(root).kits[0].samples
        let engine = AudioEngine()
        engine.onError = { XCTFail($0) }
        let loaded = expectation(description: "Four independent file streams")
        loaded.expectedFulfillmentCount = 4
        engine.onLoaded = { _, _, _ in loaded.fulfill() }
        var settings = (0..<4).map { _ in VoiceSettings() }
        for i in 0..<4 { settings[i].steps[0] = true; settings[i].steps[4] = true }
        engine.update(settings, bpm: 120)
        for i in 0..<4 { engine.load(samples[i], channel: i) }
        wait(for: [loaded], timeout: 4)
        let playing = expectation(description: "Playing all voices")
        var receivedSound = false
        engine.onLevel = { level in
            if level > 0.01 && !receivedSound { receivedSound = true; playing.fulfill() }
        }
        engine.play(); wait(for: [playing], timeout: 4)

        // Exercise graph reconfiguration, not merely another same-format file.
        try write("1_tone.wav", frames: 24000, rate: 48000, value: 2200)
        let changed = scanner.scan(root).kits[0].samples[0]
        let reloaded = expectation(description: "Edited WAV at a new sample rate")
        engine.onLoaded = { channel, path, peaks in
            if channel == 0 { XCTAssertEqual(path, changed.id); XCTAssertEqual(peaks.count, 100); reloaded.fulfill() }
        }
        engine.load(changed, channel: 0)
        wait(for: [reloaded], timeout: 4)
        for i in 0..<4 { settings[i].muted = true }
        let silent = expectation(description: "Mute reaches the actual audio mixer")
        var receivedSilence = false
        engine.onLevel = { level in
            if level == 0 && !receivedSilence { receivedSilence = true; silent.fulfill() }
        }
        engine.update(settings, bpm: 137)
        wait(for: [silent], timeout: 4)
        engine.stop()
    }
}
