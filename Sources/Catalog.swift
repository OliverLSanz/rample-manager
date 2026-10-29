import Foundation
import AVFoundation
import CoreServices

struct Sample: Identifiable, Equatable {
    let url: URL
    let voice: Int
    let revision: String
    let duration: Double
    let rate: Double
    let channels: Int
    let bits: Int
    let warnings: [String]
    let readable: Bool
    var id: String { url.path }
    var name: String { url.deletingPathExtension().lastPathComponent }
    var detail: String {
        "\(String(format: "%.2f", duration)) s · \(channels == 1 ? "mono" : "stereo") · \(String(format: "%.1f", rate / 1000)) kHz · \(bits) bit"
    }
}

struct Kit: Identifiable, Equatable {
    let url: URL
    let samples: [Sample]
    let warnings: [String]
    var id: String { url.lastPathComponent }
    var bank: String { String(id.prefix(1)).uppercased() }
    func samples(for voice: Int) -> [Sample] { samples.filter { $0.voice == voice } }
}

struct CatalogResult {
    var kits: [Kit]
    var error: String?
}

/// Kept off the UI thread. Cache keys include inode, nanosecond timestamps and size
/// so atomic saves and same-size edits both invalidate a sample.
final class CatalogScanner {
    private var cache: [String: Sample] = [:]

    static func isKitName(_ name: String) -> Bool {
        name.range(of: "^[A-Z][0-9]{1,2}$", options: .regularExpression) != nil
    }

    func scan(_ root: URL, force: Bool = false) -> CatalogResult {
        let fm = FileManager.default
        do {
            let folders = try fm.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey], options: .skipsHiddenFiles)
            var nextCache: [String: Sample] = [:]
            let kits = folders.filter { url in
                let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                return values?.isDirectory == true && values?.isSymbolicLink != true && Self.isKitName(url.lastPathComponent)
            }.sorted { $0.lastPathComponent.localizedStandardCompare($1.lastPathComponent) == .orderedAscending }.map { folder -> Kit in
                var kitWarnings: [String] = []
                let files: [URL]
                do {
                    files = try fm.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey], options: .skipsHiddenFiles)
                } catch {
                    return Kit(url: folder, samples: [], warnings: ["Could not read this folder: \(error.localizedDescription)"])
                }
                let samples = files.sorted { $0.lastPathComponent.localizedStandardCompare($1.lastPathComponent) == .orderedAscending }.compactMap { url -> Sample? in
                    let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                    guard values?.isRegularFile == true, values?.isSymbolicLink != true,
                          url.pathExtension.lowercased() == "wav",
                          let first = url.lastPathComponent.first, let voice = Int(String(first)), (1...4).contains(voice) else { return nil }
                    var info = stat()
                    guard lstat(url.path, &info) == 0 else { return nil }
                    let revision = "\(info.st_ino)-\(info.st_size)-\(info.st_mtimespec.tv_sec)-\(info.st_mtimespec.tv_nsec)-\(info.st_ctimespec.tv_sec)-\(info.st_ctimespec.tv_nsec)"
                    let sample: Sample
                    if !force, let old = cache[url.path], old.revision == revision { sample = old }
                    else { sample = inspect(url, voice: voice, revision: revision) }
                    nextCache[url.path] = sample
                    return sample
                }
                if !samples.contains(where: { $0.voice == 1 && $0.readable && $0.warnings.isEmpty }) {
                    kitWarnings.append("Voice 1 needs a compatible WAV; Rample will not be able to open this kit.")
                }
                for voice in 1...4 {
                    let layers = samples.filter { $0.voice == voice }
                    if layers.count > 12 { kitWarnings.append("Voice \(voice): \(layers.count) layers; Rample supports 12.") }
                    if Set(layers.map(\.channels)).count > 1 { kitWarnings.append("Voice \(voice): mixed mono and stereo layers.") }
                }
                if samples.contains(where: { $0.channels == 2 }) { kitWarnings.append("A stereo sample uses two hardware voices on Rample. It plays in stereo here.") }
                return Kit(url: folder, samples: samples, warnings: kitWarnings)
            }
            cache = nextCache
            return CatalogResult(kits: kits, error: nil)
        } catch {
            return CatalogResult(kits: [], error: "Could not read the folder. If this is an SD card, check that it is still connected. \(error.localizedDescription)")
        }
    }

    private func inspect(_ url: URL, voice: Int, revision: String) -> Sample {
        do {
            let file = try AVAudioFile(forReading: url)
            let format = file.processingFormat
            let duration = Double(file.length) / format.sampleRate
            let header = try wavFormat(url)
            var warnings: [String] = []
            if header.code != 1 { warnings.append("Rample requires uncompressed integer PCM WAV.") }
            if header.bits != 8 && header.bits != 16 { warnings.append("Rample requires 8 or 16 bits.") }
            if format.sampleRate != 44100 { warnings.append("Rample requires 44.1 kHz.") }
            if format.channelCount > 2 { warnings.append("Rample supports mono or stereo.") }
            if duration < 0.05 { warnings.append("Rample requires at least 50 ms.") }
            return Sample(url: url, voice: voice, revision: revision, duration: duration, rate: format.sampleRate, channels: Int(format.channelCount), bits: header.bits, warnings: warnings, readable: file.length > 0 && format.channelCount <= 2)
        } catch {
            return Sample(url: url, voice: voice, revision: revision, duration: 0, rate: 0, channels: 0, bits: 0, warnings: ["Unreadable WAV: \(error.localizedDescription)"], readable: false)
        }
    }

    private func wavFormat(_ url: URL) throws -> (code: Int, bits: Int) {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let head = try handle.read(upToCount: 12) ?? Data()
        guard head.count == 12, String(data: head.prefix(4), encoding: .ascii) == "RIFF", String(data: head.suffix(4), encoding: .ascii) == "WAVE" else { throw CocoaError(.fileReadCorruptFile) }
        for _ in 0..<256 {
            let chunk = try handle.read(upToCount: 8) ?? Data()
            guard chunk.count == 8 else { break }
            let size = chunk[4..<8].enumerated().reduce(UInt32(0)) { $0 | UInt32($1.element) << ($1.offset * 8) }
            if String(data: chunk.prefix(4), encoding: .ascii) == "fmt " {
                let fmt = try handle.read(upToCount: Int(min(size, 64))) ?? Data()
                guard fmt.count >= 16 else { break }
                var code = Int(fmt[0]) | Int(fmt[1]) << 8
                if code == 0xfffe && fmt.count >= 40 { code = Int(fmt[24]) | Int(fmt[25]) << 8 }
                return (code, Int(fmt[14]) | Int(fmt[15]) << 8)
            }
            try handle.seek(toOffset: handle.offset() + UInt64(size) + UInt64(size % 2))
        }
        throw CocoaError(.fileReadCorruptFile)
    }
}

final class FolderWatcher {
    private var stream: FSEventStreamRef?
    var onChange: (() -> Void)?

    func start(_ root: URL) -> Bool {
        stop()
        var context = FSEventStreamContext(version: 0, info: Unmanaged.passUnretained(self).toOpaque(), retain: nil, release: nil, copyDescription: nil)
        let callback: FSEventStreamCallback = { _, info, _, _, _, _ in
            guard let info else { return }
            Unmanaged<FolderWatcher>.fromOpaque(info).takeUnretainedValue().onChange?()
        }
        let flags = kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagWatchRoot | kFSEventStreamCreateFlagNoDefer
        stream = FSEventStreamCreate(nil, callback, &context, [root.path] as CFArray, FSEventStreamEventId(kFSEventStreamEventIdSinceNow), 0.15, FSEventStreamCreateFlags(flags))
        guard let stream else { return false }
        FSEventStreamSetDispatchQueue(stream, .main)
        return FSEventStreamStart(stream)
    }

    func stop() {
        if let stream { FSEventStreamStop(stream); FSEventStreamInvalidate(stream); FSEventStreamRelease(stream) }
        stream = nil
    }
    deinit { stop() }
}
