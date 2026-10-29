import SwiftUI
import AppKit

enum Palette {
    static let background = Color(red: 0.065, green: 0.075, blue: 0.085)
    static let sidebar = Color(red: 0.085, green: 0.095, blue: 0.105)
    static let panel = Color(red: 0.105, green: 0.12, blue: 0.135)
    static let raised = Color(red: 0.145, green: 0.16, blue: 0.18)
    static let muted = Color(red: 0.52, green: 0.56, blue: 0.60)
    static let line = Color.white.opacity(0.075)
    static let accent = Color(red: 0.96, green: 0.49, blue: 0.36)
    static let voices = [accent, Color(red: 0.51, green: 0.79, blue: 0.68), Color(red: 0.67, green: 0.61, blue: 0.87), Color(red: 0.87, green: 0.76, blue: 0.46)]
}

struct ContentView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            sidebar.frame(width: 224)
            Rectangle().fill(Palette.line).frame(width: 1)
            VStack(alignment: .leading, spacing: 0) {
                topBar
                Rectangle().fill(Palette.line).frame(height: 1)
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        kitHeading
                        if let error = model.folderError { notice(error, icon: "externaldrive.badge.exclamationmark", color: .orange) }
                        if model.kits.isEmpty && !model.scanning { emptyLibrary }
                        HStack(alignment: .top, spacing: 12) {
                            ForEach(0..<4) { channel in SampleCard(model: model, channel: channel) }
                        }
                        sequencer
                        if !model.selectionWarnings.isEmpty {
                            DisclosureGroup {
                                VStack(alignment: .leading, spacing: 7) {
                                    ForEach(model.selectionWarnings, id: \.self) { text in
                                        Text("• " + text).font(.system(size: 11)).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
                                    }
                                }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, 9)
                            } label: {
                                Label("\(model.selectionWarnings.count) Rample compatibility warnings", systemImage: "exclamationmark.triangle")
                                    .font(.system(size: 11, weight: .medium)).foregroundStyle(Color.orange.opacity(0.9))
                            }
                            .tint(Palette.muted).padding(13).background(Palette.panel, in: RoundedRectangle(cornerRadius: 9))
                        }
                        footer
                    }.padding(24)
                }
            }
            .background(Palette.background)
        }
        .frame(minWidth: 1180, minHeight: 760)
        .preferredColorScheme(.dark)
        .background(Palette.background)
        .alert("Rample Preview", isPresented: Binding(get: { model.alert != nil }, set: { if !$0 { model.alert = nil } })) {
            Button("OK") { model.alert = nil }
        } message: { Text(model.alert ?? "") }
        .sheet(isPresented: $model.exportPresented) { exportSheet }
        .background(KeyboardShortcuts(model: model).frame(width: 0, height: 0))
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "waveform.path").font(.system(size: 24, weight: .bold)).foregroundStyle(Palette.accent)
                VStack(alignment: .leading, spacing: 2) {
                    Text("RAMPLE").font(.system(size: 17, weight: .black, design: .rounded)).tracking(2)
                    Text("P R E V I E W").font(.system(size: 8, weight: .semibold)).foregroundStyle(Palette.muted)
                }
            }.padding(.horizontal, 22).padding(.top, 42).padding(.bottom, 29)

            HStack {
                Text("LIBRARY").font(.system(size: 9, weight: .bold)).tracking(1.5).foregroundStyle(Palette.muted)
                Spacer()
                Text("\(model.kits.count)").font(.system(size: 10, weight: .medium, design: .monospaced)).foregroundStyle(Palette.muted)
            }.padding(.horizontal, 20)

            HStack(spacing: 7) {
                Image(systemName: "magnifyingglass").foregroundStyle(Palette.muted)
                TextField("Search kits or samples", text: $model.search).textFieldStyle(.plain).font(.system(size: 11))
            }.padding(10).background(Palette.background.opacity(0.75), in: RoundedRectangle(cornerRadius: 7)).padding(.horizontal, 14).padding(.top, 13)

            Menu {
                Button("All banks") { model.bankFilter = "All" }
                ForEach(model.banks, id: \.self) { bank in Button("Bank \(bank)") { model.bankFilter = bank } }
            } label: {
                HStack {
                    Image(systemName: "square.stack.3d.up").font(.system(size: 11))
                    Text(model.bankFilter == "All" ? "All banks" : "Bank \(model.bankFilter)").font(.system(size: 11))
                    Spacer()
                    Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
                }.foregroundStyle(Palette.muted).padding(.horizontal, 8).padding(.vertical, 12)
            }.menuStyle(.borderlessButton).menuIndicator(.hidden).padding(.horizontal, 14)

            ScrollView {
                LazyVStack(spacing: 5) {
                    ForEach(model.visibleKits) { kit in
                        Button { model.selectKit(kit.id) } label: {
                            HStack(spacing: 11) {
                                Text(kit.id).font(.system(size: 13, weight: .bold, design: .monospaced))
                                    .foregroundStyle(model.selectedKit == kit.id ? Palette.accent : .white.opacity(0.8))
                                    .frame(width: 39, alignment: .leading)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text("Kit \(kit.id)").font(.system(size: 11, weight: .medium))
                                    Text("\(kit.samples.count) samples").font(.system(size: 10)).foregroundStyle(Palette.muted)
                                }
                                Spacer(minLength: 0)
                                if model.selectedKit == kit.id {
                                    Circle().fill(Palette.accent).frame(width: 5, height: 5)
                                } else if !kit.warnings.isEmpty {
                                    Image(systemName: "exclamationmark.circle").font(.system(size: 10)).foregroundStyle(Palette.muted)
                                }
                            }.padding(.horizontal, 12).padding(.vertical, 12)
                                .background(model.selectedKit == kit.id ? Palette.accent.opacity(0.09) : Color.clear, in: RoundedRectangle(cornerRadius: 7))
                                .contentShape(Rectangle())
                        }.buttonStyle(.plain).accessibilityLabel("Load kit \(kit.id)")
                    }
                    if model.visibleKits.isEmpty {
                        Text(model.search.isEmpty ? "No kits in this bank" : "No matches").font(.system(size: 11)).foregroundStyle(Palette.muted).padding(.top, 20)
                    }
                }.padding(.horizontal, 12)
            }
            Spacer(minLength: 8)
            VStack(alignment: .leading, spacing: 11) {
                if model.isDemo {
                    Label("Demo library", systemImage: "sparkles").font(.system(size: 10, weight: .medium)).foregroundStyle(Palette.voices[1])
                    Text("Four sounds to get started. Open your folder to hear your own samples.").font(.system(size: 11)).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
                }
                Button(action: model.chooseFolder) {
                    Label("Open folder…", systemImage: "folder.badge.plus").font(.system(size: 11, weight: .semibold)).frame(maxWidth: .infinity).padding(.vertical, 10)
                }.buttonStyle(.plain).background(Palette.raised, in: RoundedRectangle(cornerRadius: 7)).help("Select your SD root or a folder containing kits")
                Text("LOCAL · OFFLINE").font(.system(size: 8, weight: .medium, design: .monospaced)).tracking(1).foregroundStyle(Palette.muted.opacity(0.7)).frame(maxWidth: .infinity)
            }.padding(17)
        }.background(Palette.sidebar)
    }

    private var topBar: some View {
        HStack(spacing: 12) {
            Image(systemName: "externaldrive").font(.system(size: 13)).foregroundStyle(Palette.muted)
            Button(action: model.revealRoot) {
                Text(model.root?.lastPathComponent ?? "No folder").font(.system(size: 12, weight: .medium)).lineLimit(1)
            }.buttonStyle(.plain).help(model.root?.path ?? "")
            if model.isDemo { Text("DEMO").font(.system(size: 8, weight: .bold)).foregroundStyle(Palette.muted).padding(.horizontal, 6).padding(.vertical, 3).background(Palette.raised, in: Capsule()) }
            Spacer()
            HStack(spacing: 6) {
                Circle().fill(model.folderError != nil ? .orange : Palette.voices[1]).frame(width: 5, height: 5)
                Text(model.scanning ? "Scanning…" : (model.watching ? "Live updates" : "Auto-refresh · 4 s")).font(.system(size: 10)).foregroundStyle(Palette.muted)
            }
            Button { model.refresh(force: true) } label: { Image(systemName: "arrow.clockwise").font(.system(size: 12)).padding(7) }
                .buttonStyle(.plain).help("Refresh files (⌘R)").accessibilityLabel("Refresh files")
            Rectangle().fill(Palette.line).frame(width: 1, height: 18)
            Button { model.exportPresented = true } label: {
                Label("Export kit", systemImage: "square.and.arrow.up").font(.system(size: 11, weight: .medium)).padding(.horizontal, 12).padding(.vertical, 8)
            }.buttonStyle(.plain).background(Palette.raised, in: RoundedRectangle(cornerRadius: 6))
        }.padding(.horizontal, 24).frame(height: 60)
    }

    private var kitHeading: some View {
        HStack(alignment: .center, spacing: 17) {
            Text(model.selectedKit.isEmpty ? "—" : model.selectedKit).font(.system(size: 30, weight: .medium, design: .monospaced)).foregroundStyle(Palette.accent)
                .frame(width: 70, height: 61).background(Palette.accent.opacity(0.08), in: RoundedRectangle(cornerRadius: 12)).overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.accent.opacity(0.17)))
            VStack(alignment: .leading, spacing: 7) {
                Text("Your kit. In context.").font(.system(size: 25, weight: .semibold)).tracking(-0.6)
                Text("Choose a layer for each voice and hear how your samples fit together.").font(.system(size: 12)).foregroundStyle(Palette.muted)
            }
            Spacer(minLength: 0)
        }
    }

    private var emptyLibrary: some View {
        notice("No kits found. Select the root folder containing A0, A1… Z99. WAV filenames inside each kit must start with 1, 2, 3 or 4.", icon: "folder", color: Palette.voices[1])
    }

    private var sequencer: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "square.grid.3x3.fill").font(.system(size: 12)).foregroundStyle(Palette.accent)
                Text("Sequencer").font(.system(size: 14, weight: .semibold))
                Text("16 STEPS · 4/4 · 1/16").font(.system(size: 9, weight: .medium, design: .monospaced)).foregroundStyle(Palette.muted).padding(.leading, 6)
                Spacer()
                Menu {
                    Button("Load example beat", action: model.basicPattern)
                    Button("Clear all 64 steps", action: model.clearPattern)
                } label: { Label("Pattern", systemImage: "slider.horizontal.3").font(.system(size: 10)).foregroundStyle(Palette.muted) }
                    .menuStyle(.borderlessButton).fixedSize()
            }.padding(16)
            Rectangle().fill(Palette.line).frame(height: 1)
            VStack(spacing: 10) {
                HStack(spacing: 0) {
                    Text("VOICE / LEVEL").font(.system(size: 8, weight: .semibold)).tracking(1).foregroundStyle(Palette.muted).frame(width: 143, alignment: .leading)
                    HStack(spacing: 5) {
                        ForEach(0..<16) { step in
                            Text(String(format: "%02d", step + 1)).font(.system(size: 9, weight: step % 4 == 0 ? .bold : .regular, design: .monospaced))
                                .foregroundStyle(model.playhead == step ? .white : Palette.muted.opacity(step % 4 == 0 ? 1 : 0.55)).frame(maxWidth: .infinity)
                                .padding(.leading, step % 4 == 0 && step != 0 ? 5 : 0)
                        }
                    }
                }.padding(.bottom, 1)
                ForEach(0..<4) { channel in SequencerRow(model: model, channel: channel) }
                HStack(spacing: 0) {
                    Color.clear.frame(width: 143, height: 1)
                    HStack(spacing: 5) {
                        ForEach(0..<16) { step in
                            Capsule().fill(model.playhead == step ? Palette.accent : Palette.line)
                                .frame(maxWidth: .infinity).frame(height: 3).padding(.leading, step % 4 == 0 && step != 0 ? 5 : 0)
                        }
                    }
                }.padding(.top, 2)
            }.padding(16)
            Rectangle().fill(Palette.line).frame(height: 1)
            transport.padding(18)
        }.background(Palette.panel, in: RoundedRectangle(cornerRadius: 12)).overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.line))
    }

    private var transport: some View {
        HStack(spacing: 15) {
            Button(action: model.togglePlay) {
                HStack(spacing: 8) {
                    Image(systemName: model.isPlaying ? "stop.fill" : "play.fill").font(.system(size: 11))
                    Text(model.isPlaying ? "Stop" : "Play").font(.system(size: 12, weight: .semibold))
                }.frame(width: 132, height: 37).foregroundStyle(Palette.background)
                    .background(Palette.accent, in: RoundedRectangle(cornerRadius: 7))
            }.buttonStyle(.plain).accessibilityLabel(model.isPlaying ? "Stop sequencer" : "Play sequencer")
            HStack(spacing: 8) {
                TextField("BPM", value: $model.bpm, format: .number.precision(.fractionLength(0)))
                    .font(.system(size: 20, weight: .medium, design: .monospaced)).textFieldStyle(.plain).multilineTextAlignment(.trailing).frame(width: 46)
                    .onSubmit { model.bpm = min(300, max(30, model.bpm)) }
                    .accessibilityLabel("Tempo in BPM")
                VStack(alignment: .leading, spacing: 1) {
                    Text("BPM").font(.system(size: 8, weight: .bold)).foregroundStyle(Palette.muted)
                    HStack(spacing: 9) {
                        Button { model.bpm = max(30, model.bpm - 1) } label: { Image(systemName: "minus").font(.system(size: 9)) }.buttonStyle(.plain)
                        Button { model.bpm = min(300, model.bpm + 1) } label: { Image(systemName: "plus").font(.system(size: 9)) }.buttonStyle(.plain)
                    }
                }
            }
            Slider(value: $model.bpm, in: 30...300, step: 1).tint(Palette.accent).frame(maxWidth: 145).controlSize(.small).accessibilityLabel("Tempo")
            Spacer(minLength: 4)
            LevelMeter(meter: model.meter).padding(.trailing, 7)
            Text(model.playhead >= 0 ? "\(model.playhead / 4 + 1).\(model.playhead % 4 + 1)" : "1.1")
                .font(.system(size: 18, weight: .medium, design: .monospaced)).foregroundStyle(model.isPlaying ? .white.opacity(0.85) : Palette.muted)
            Text("SPACE").font(.system(size: 8, weight: .medium, design: .monospaced)).foregroundStyle(Palette.muted)
                .padding(.horizontal, 7).padding(.vertical, 5).overlay(RoundedRectangle(cornerRadius: 4).stroke(Palette.line))
        }
    }

    private var footer: some View {
        HStack(spacing: 6) {
            Image(systemName: "keyboard").font(.system(size: 11))
            Text("Space: play / stop").font(.system(size: 10))
            Text("·").padding(.horizontal, 3)
            Text("1–4: audition voice").font(.system(size: 10))
            Text("·").padding(.horizontal, 3)
            Text("⌘R: refresh").font(.system(size: 10))
            Spacer()
            Text(model.statusMessage.isEmpty ? "Original files stay untouched" : model.statusMessage).font(.system(size: 10)).lineLimit(1)
        }.foregroundStyle(Palette.muted.opacity(0.75))
    }

    private func notice(_ text: String, icon: String, color: Color) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: icon).foregroundStyle(color)
            Text(text).font(.system(size: 12)).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
        }.padding(15).frame(maxWidth: .infinity, alignment: .leading).background(color.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
    }

    private var exportSheet: some View {
        VStack(alignment: .leading, spacing: 19) {
            Label("Export selection", systemImage: "square.and.arrow.up").font(.system(size: 21, weight: .semibold))
            Text("Create a kit with a copy of your selected samples. Choose your SD root as the destination to use it directly on Rample.").font(.system(size: 13)).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
            HStack {
                Text("Kit name").font(.system(size: 12))
                TextField("B0", text: $model.exportName).textFieldStyle(.roundedBorder).frame(width: 90)
                Text("A0–Z99").font(.system(size: 11, design: .monospaced)).foregroundStyle(Palette.muted)
            }
            Text("Compatible mono WAV · voice 1 required · existing folders stay untouched").font(.system(size: 10)).foregroundStyle(Palette.muted)
            HStack {
                Spacer()
                Button("Cancel") { model.exportPresented = false }.keyboardShortcut(.cancelAction)
                Button("Choose destination…", action: model.exportKit).buttonStyle(.borderedProminent).tint(Palette.accent)
            }
        }.padding(28).frame(width: 480).background(Palette.background)
    }
}

struct SampleCard: View {
    @ObservedObject var model: AppModel
    let channel: Int
    private var color: Color { Palette.voices[channel] }
    private var sample: Sample? { model.sample(channel) }
    private var missing: Bool { sample == nil && !model.voices[channel].samplePath.isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                HStack(spacing: 5) {
                    Circle().fill(color).frame(width: 5, height: 5)
                    Text("VOICE 0\(channel + 1)").font(.system(size: 9, weight: .bold)).tracking(1)
                }.foregroundStyle(color)
                Spacer()
                Menu {
                    ForEach(model.kits) { kit in Button("\(kit.id) · \(kit.samples(for: channel + 1).count) layers") { model.selectSource(kit.id, channel: channel) } }
                } label: {
                    HStack(spacing: 4) {
                        Text(model.voices[channel].kitID.isEmpty ? "Kit" : model.voices[channel].kitID).font(.system(size: 10, weight: .medium, design: .monospaced))
                        Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                    }.foregroundStyle(Palette.muted)
                }.menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize().help("Source kit for voice \(channel + 1)")
            }.padding(.bottom, 14)
            Menu {
                Button("No sample") { model.selectSample("", channel: channel) }
                ForEach(model.choices(channel)) { item in
                    Button {
                        model.selectSample(item.id, channel: channel)
                    } label: {
                        if item.id == model.voices[channel].samplePath { Label(item.name, systemImage: "checkmark") }
                        else { Text(item.name + (item.warnings.isEmpty ? "" : " ⚠︎")) }
                    }.disabled(!item.readable)
                }
            } label: {
                HStack(spacing: 6) {
                    Text(sample?.name ?? (missing ? "Sample deleted" : "No sample"))
                        .font(.system(size: 13, weight: .semibold)).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.up.chevron.down").font(.system(size: 9, weight: .medium)).foregroundStyle(Palette.muted)
                }.foregroundStyle(missing ? Color.orange : Color.white.opacity(0.9)).frame(maxWidth: .infinity)
            }.menuStyle(.borderlessButton).menuIndicator(.hidden).disabled(model.choices(channel).isEmpty)
                .help(sample?.url.lastPathComponent ?? "Select a sample")
                .accessibilityLabel("Sample for voice \(channel + 1)")

            WaveformView(peaks: model.waveforms[channel], color: color, active: model.isPlaying && model.playhead >= 0 && model.voices[channel].steps[model.playhead] && !model.voices[channel].muted)
                .frame(height: 46).padding(.vertical, 12)
            HStack {
                if let sample {
                    Text(String(format: "%.2f s", sample.duration)).font(.system(size: 10, design: .monospaced))
                    Spacer(minLength: 2)
                    Text(sample.channels == 1 ? "MONO" : "STEREO").font(.system(size: 8, weight: .medium, design: .monospaced))
                    if !sample.warnings.isEmpty { Image(systemName: "exclamationmark.triangle").font(.system(size: 9)).foregroundStyle(.orange).help(sample.warnings.joined(separator: "\n")) }
                } else {
                    Text(missing ? "Choose another file or restore it" : "No layers in this voice").font(.system(size: 10)).lineLimit(1)
                    Spacer(minLength: 0)
                }
            }.foregroundStyle(Palette.muted).help(sample?.detail ?? "").padding(.bottom, 15)
            HStack {
                Button { model.preview(channel) } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "play.fill").font(.system(size: 8))
                        Text("Audition").font(.system(size: 10, weight: .medium))
                        Text("\(channel + 1)").font(.system(size: 8, design: .monospaced)).foregroundStyle(Palette.muted).padding(.horizontal, 4).padding(.vertical, 2).background(Palette.raised, in: RoundedRectangle(cornerRadius: 3))
                    }.frame(maxWidth: .infinity).frame(height: 29)
                }.buttonStyle(.plain).background(Palette.raised.opacity(0.5), in: RoundedRectangle(cornerRadius: 5)).disabled(sample?.readable != true || model.loadedPaths[channel] != sample?.id)
                    .accessibilityLabel("Audition voice \(channel + 1)")
                Button { model.revealSample(channel) } label: { Image(systemName: "folder").font(.system(size: 11)).foregroundStyle(Palette.muted).padding(7) }.buttonStyle(.plain).disabled(sample == nil).help("Show WAV in Finder")
            }
        }.padding(16).frame(maxWidth: .infinity).background(Palette.panel, in: RoundedRectangle(cornerRadius: 10))
            .overlay(alignment: .top) { Rectangle().fill(color.opacity(0.75)).frame(height: 2).padding(.horizontal, 16) }
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(Palette.line))
    }
}

struct SequencerRow: View {
    @ObservedObject var model: AppModel
    let channel: Int
    private var color: Color { Palette.voices[channel] }
    var body: some View {
        HStack(spacing: 0) {
            HStack(spacing: 7) {
                Text("0\(channel + 1)").font(.system(size: 10, weight: .semibold, design: .monospaced)).foregroundStyle(color)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Button { model.changeVoice(channel) { $0.muted.toggle() } } label: {
                            Text("M").font(.system(size: 8, weight: .bold)).foregroundStyle(model.voices[channel].muted ? Palette.background : Palette.muted)
                                .frame(width: 19, height: 16).background(model.voices[channel].muted ? color : Palette.raised, in: RoundedRectangle(cornerRadius: 3))
                        }.buttonStyle(.plain).help("Mute voice \(channel + 1)").accessibilityLabel("Mute voice \(channel + 1)")
                        Button { model.changeVoice(channel) { $0.solo.toggle() } } label: {
                            Text("S").font(.system(size: 8, weight: .bold)).foregroundStyle(model.voices[channel].solo ? Palette.background : Palette.muted)
                                .frame(width: 19, height: 16).background(model.voices[channel].solo ? color : Palette.raised, in: RoundedRectangle(cornerRadius: 3))
                        }.buttonStyle(.plain).help("Solo voice \(channel + 1)").accessibilityLabel("Solo voice \(channel + 1)")
                        Spacer(minLength: 0)
                    }
                    Slider(value: Binding(get: { Double(model.voices[channel].volume) }, set: { value in model.changeVoice(channel) { $0.volume = Float(value) } }), in: 0...1)
                        .controlSize(.mini).tint(color).frame(width: 81).accessibilityLabel("Voice level \(channel + 1)")
                }
            }.frame(width: 143, alignment: .leading)
            HStack(spacing: 5) {
                ForEach(0..<16) { step in
                    let on = model.voices[channel].steps[step]
                    let current = model.playhead == step
                    Button { model.toggleStep(channel, step) } label: {
                        RoundedRectangle(cornerRadius: 5)
                            .fill(on ? color.opacity(model.voices[channel].muted ? 0.25 : (current ? 1 : 0.76)) : (current ? Color.white.opacity(0.12) : Palette.raised.opacity(step % 4 == 0 ? 0.95 : 0.58)))
                            .overlay(RoundedRectangle(cornerRadius: 5).stroke(current ? Color.white.opacity(0.8) : (on ? color.opacity(0.3) : Color.white.opacity(0.025)), lineWidth: current ? 1.5 : 1))
                            .overlay { if on { Circle().fill(Palette.background.opacity(0.48)).frame(width: 4, height: 4) } }
                            .frame(maxWidth: .infinity).frame(height: 35)
                    }.buttonStyle(.plain).padding(.leading, step % 4 == 0 && step != 0 ? 5 : 0)
                        .help("Voice \(channel + 1), step \(step + 1)")
                        .accessibilityLabel("Voice \(channel + 1), step \(step + 1), \(on ? "on" : "off")")
                }
            }
        }
    }
}

struct WaveformView: View {
    let peaks: [Float]
    let color: Color
    let active: Bool
    var body: some View {
        Canvas { context, size in
            var center = Path(); center.move(to: CGPoint(x: 0, y: size.height / 2)); center.addLine(to: CGPoint(x: size.width, y: size.height / 2))
            context.stroke(center, with: .color(color.opacity(0.12)), lineWidth: 1)
            guard !peaks.isEmpty else { return }
            let width = size.width / CGFloat(peaks.count)
            for (i, peak) in peaks.enumerated() {
                let height = max(1.5, CGFloat(peak) * size.height * 0.92)
                let rect = CGRect(x: CGFloat(i) * width, y: (size.height - height) / 2, width: max(1, width * 0.65), height: height)
                context.fill(Path(roundedRect: rect, cornerRadius: 0.6), with: .color(color.opacity(active ? 1 : 0.68)))
            }
        }.accessibilityLabel("Waveform")
    }
}

struct LevelMeter: View {
    @ObservedObject var meter: OutputMeter
    var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(0..<9) { bar in
                RoundedRectangle(cornerRadius: 1).fill(Double(meter.level) > pow(Double(bar) / 12, 2) + 0.001 ? Palette.voices[1] : Palette.raised)
                    .frame(width: 3, height: CGFloat(7 + bar * 2))
            }
        }.frame(height: 25).help("Output level")
    }
}

struct KeyboardShortcuts: NSViewRepresentable {
    let model: AppModel
    func makeNSView(context: Context) -> NSView {
        context.coordinator.start(model: model)
        return NSView()
    }
    func updateNSView(_ nsView: NSView, context: Context) {}
    func makeCoordinator() -> Coordinator { Coordinator() }
    final class Coordinator {
        private var monitor: Any?
        func start(model: AppModel) {
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak model] event in
                guard let model, NSApp.keyWindow != nil, !model.exportPresented,
                      !(NSApp.keyWindow?.firstResponder is NSTextView),
                      event.modifierFlags.intersection([.command, .control, .option]).isEmpty,
                      !event.isARepeat else { return event }
                if event.keyCode == 49 { model.togglePlay(); return nil }
                if let key = event.characters, let voice = Int(key), (1...4).contains(voice) { model.preview(voice - 1); return nil }
                return event
            }
        }
        deinit { if let monitor { NSEvent.removeMonitor(monitor) } }
    }
}
