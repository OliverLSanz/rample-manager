import SwiftUI

@main
struct RamplePreviewApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        Window("Rample Preview", id: "main") {
            ContentView(model: model)
                .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in model.save() }
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentMinSize)
        .defaultSize(width: 1320, height: 850)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Open sample folder…", action: model.chooseFolder).keyboardShortcut("o")
                Button("Refresh files") { model.refresh(force: true) }.keyboardShortcut("r")
                Divider()
                Button("Export kit…") { model.exportPresented = true }.keyboardShortcut("e", modifiers: [.command, .shift])
            }
            CommandMenu("Playback") {
                Button(model.isPlaying ? "Stop" : "Play", action: model.togglePlay)
                Button("Load example beat", action: model.basicPattern)
                Button("Clear pattern", action: model.clearPattern)
            }
            CommandGroup(replacing: .help) {
                Button("Rample kit format") { NSWorkspace.shared.open(URL(string: "https://squarp.net/rample/manual/")!) }
                Button("Open demo library", action: model.showDemo)
            }
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationDidFinishLaunching(_ notification: Notification) { NSApp.activate(ignoringOtherApps: true) }
}
