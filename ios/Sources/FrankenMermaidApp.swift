import SwiftUI

@main
struct FrankenMermaidApp: App {
    var body: some Scene {
        WindowGroup {
            StudioView()
                .preferredColorScheme(.dark)
#if targetEnvironment(macCatalyst)
                .frame(minWidth: 780, minHeight: 620)
#endif
        }
#if targetEnvironment(macCatalyst)
        .defaultSize(width: 1240, height: 840)
        .windowResizability(.automatic)
#endif
        .commands {
            CommandMenu("Diagram") {
                Button("Render Diagram") {
                    NotificationCenter.default.post(name: .renderMermaidNow, object: nil)
                }
                .keyboardShortcut("r", modifiers: .command)
            }
        }
    }
}

extension Notification.Name {
    static let renderMermaidNow = Notification.Name("FrankenMermaid.renderNow")
}

