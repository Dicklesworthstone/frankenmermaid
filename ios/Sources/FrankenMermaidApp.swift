import SwiftUI
import UIKit

@main
struct FrankenMermaidApp: App {
    var body: some Scene {
        WindowGroup {
            StudioView()
                .background(CatalystWindowFreedom())
#if targetEnvironment(macCatalyst)
                .frame(minWidth: 480, minHeight: 420)
#endif
        }
#if targetEnvironment(macCatalyst)
        .defaultSize(width: 1240, height: 840)
        .windowResizability(.contentMinSize)
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

private struct CatalystWindowFreedom: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> Controller { Controller() }
    func updateUIViewController(_ controller: Controller, context: Context) { controller.configure() }

    final class Controller: UIViewController {
        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            configure()
        }

        override func viewDidLayoutSubviews() {
            super.viewDidLayoutSubviews()
            configure()
        }

        func configure() {
#if targetEnvironment(macCatalyst)
            guard let restrictions = view.window?.windowScene?.sizeRestrictions else { return }
            restrictions.minimumSize = CGSize(width: 480, height: 420)
            restrictions.maximumSize = CGSize(width: 10_000, height: 10_000)
#endif
        }
    }
}

extension Notification.Name {
    static let renderMermaidNow = Notification.Name("FrankenMermaid.renderNow")
}
