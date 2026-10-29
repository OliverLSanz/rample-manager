import Foundation

enum DemoSamples {
    static func create(in support: URL) throws -> URL {
        let root = support.appendingPathComponent("Demo SD", isDirectory: true)
        let kit = root.appendingPathComponent("A0", isDirectory: true)
        try FileManager.default.createDirectory(at: kit, withIntermediateDirectories: true)
        let names = ["1_Sub kick.wav", "2_Dust snare.wav", "3_Closed hat.wav", "4_Wood click.wav"]
        for i in 0..<4 {
            let url = kit.appendingPathComponent(names[i])
            if FileManager.default.fileExists(atPath: url.path) { continue }
            let duration = [0.55, 0.3, 0.12, 0.19][i]
            var seed: UInt64 = 401 + UInt64(i)
            let pcm: [Int16] = (0..<Int(duration * 44100)).map { n in
                let t = Double(n) / 44100
                seed = 2862933555777941757 &* seed &+ 3037000493
                let noise = Double(seed >> 33) / Double(UInt32.max >> 1) * 2 - 1
                let value: Double
                switch i {
                case 0: value = sin(2 * .pi * (48 * t + 7 * (1 - exp(-t * 28)))) * exp(-t * 9) * min(1, t * 1200)
                case 1: value = (noise * 0.65 + sin(2 * .pi * 185 * t) * 0.25) * exp(-t * 18) * min(1, t * 2000)
                case 2: value = (noise * 0.7 + sin(2 * .pi * 7800 * t) * 0.1) * exp(-t * 48)
                default: value = (sin(2 * .pi * 760 * t) * 0.6 + noise * 0.15) * exp(-t * 38)
                }
                return Int16(max(-1, min(1, value * 0.8)) * 32767)
            }
            try wav(pcm).write(to: url, options: .atomic)
        }
        return root
    }

    static func wav(_ pcm: [Int16], rate: UInt32 = 44100) -> Data {
        var data = Data()
        func string(_ s: String) { data.append(contentsOf: s.utf8) }
        func u16(_ value: UInt16) { var v = value.littleEndian; withUnsafeBytes(of: &v) { data.append(contentsOf: $0) } }
        func u32(_ value: UInt32) { var v = value.littleEndian; withUnsafeBytes(of: &v) { data.append(contentsOf: $0) } }
        string("RIFF"); u32(UInt32(pcm.count * 2 + 36)); string("WAVEfmt "); u32(16)
        u16(1); u16(1); u32(rate); u32(rate * 2); u16(2); u16(16)
        string("data"); u32(UInt32(pcm.count * 2))
        for frame in pcm { u16(UInt16(bitPattern: frame)) }
        return data
    }
}
