import Foundation
import SwiftUI
import WebKit

enum GraphPhase: Equatable {
    case loading
    case ready
    case rendering
    case failed(String)
}

enum MermaidExportKind: String, CaseIterable, Identifiable {
    case source
    case svg
    case animatedHTML

    var id: Self { self }

    var title: String {
        switch self {
        case .source: "Mermaid Source"
        case .svg: "Vector SVG"
        case .animatedHTML: "Animated Web Page"
        }
    }

    var symbol: String {
        switch self {
        case .source: "chevron.left.forwardslash.chevron.right"
        case .svg: "scribble.variable"
        case .animatedHTML: "sparkles.rectangle.stack"
        }
    }

    var fileExtension: String {
        switch self {
        case .source: "mmd"
        case .svg: "svg"
        case .animatedHTML: "html"
        }
    }
}

@MainActor
final class MermaidRendererModel: NSObject, ObservableObject {
    @Published var source = MermaidRendererModel.sample
    @Published private(set) var phase: GraphPhase = .loading
    @Published private(set) var elapsedMS: Double?
    @Published private(set) var diagramType = "detecting"
    @Published private(set) var nodeCount = 0
    @Published private(set) var edgeCount = 0
    @Published private(set) var hasCurrentRenderedArtifact = false

    let webView: WKWebView
    private var requestID = 0
    private var scheduledRender: Task<Void, Never>?
    private var debugExportProbePending = false
    private var theme = "dark"
    private var fontSize = 14.0
    private var padding = 18.0
    private var shadows = true
    private var roundedCorners = 10.0
    private var nodeGradients = true

    override init() {
        debugExportProbePending = ProcessInfo.processInfo.environment["FM_EXPORT_PROBE"] == "1"
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(MermaidResourceSchemeHandler(), forURLScheme: "frankenmermaid-resource")
        configuration.defaultWebpagePreferences.allowsContentJavaScript = true
        configuration.websiteDataStore = .nonPersistent()
        webView = WKWebView(frame: .zero, configuration: configuration)
        super.init()
        configuration.userContentController.add(self, name: "frankenBridge")
        webView.navigationDelegate = self
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.load(URLRequest(url: URL(string: "frankenmermaid-resource://bundle/bridge.html")!))
    }

    deinit { scheduledRender?.cancel() }

    func updateStyle(
        theme: String,
        fontSize: Double,
        padding: Double,
        shadows: Bool,
        roundedCorners: Double,
        nodeGradients: Bool,
        renderImmediately: Bool
    ) {
        self.theme = theme
        self.fontSize = min(22, max(9, fontSize))
        self.padding = min(48, max(8, padding))
        self.shadows = shadows
        self.roundedCorners = min(24, max(0, roundedCorners))
        self.nodeGradients = nodeGradients
        hasCurrentRenderedArtifact = false
        if renderImmediately { renderNow() }
    }

    func scheduleRender() {
        scheduledRender?.cancel()
        // An export made during the debounce window would otherwise silently
        // contain the previous source. Mark the rendered artifact stale as soon
        // as editing begins, rather than only when WebKit starts rendering.
        hasCurrentRenderedArtifact = false
        let expectedSource = source
        scheduledRender = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(180))
            guard !Task.isCancelled, let self, self.source == expectedSource else { return }
            self.renderNow()
        }
    }

    func renderNow() {
        guard phase != .loading, !source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        requestID += 1
        let req = requestID
        let command: [String: Any] = [
            "requestID": req,
            "source": source,
            "options": [
                "theme": theme,
                "responsive": true,
                "accessible": true,
                "fontSize": fontSize,
                "padding": padding,
                "shadows": shadows,
                "roundedCorners": roundedCorners,
                "nodeGradients": nodeGradients,
                "enableLinks": false
            ]
        ]
        hasCurrentRenderedArtifact = false
        phase = .rendering
        Task { [weak self, weak webView] in
            guard let self else { return }
            guard let webView else {
                if self.requestID == req {
                    self.phase = .failed("The private diagram renderer is no longer available.")
                }
                return
            }
            do {
                _ = try await webView.callAsyncJavaScript(
                    "return await window.frankenRender(command)",
                    arguments: ["command": command],
                    in: nil,
                    contentWorld: .page
                )
            } catch {
                // A slower, superseded WebKit request must not replace the
                // state of a newer successful render.
                guard self.requestID == req else { return }
                self.phase = .failed(error.localizedDescription)
            }
        }
    }

    func prepareExport(_ kind: MermaidExportKind) async throws -> URL {
        let contents: String
        if kind == .source {
            contents = source
        } else {
            guard hasCurrentRenderedArtifact, phase == .ready else {
                throw CocoaError(.fileWriteUnknown, userInfo: [
                    NSLocalizedDescriptionKey:
                        "Wait for the current diagram to finish rendering before exporting it."
                ])
            }
            let command: [String: Any] = [
                "kind": kind.rawValue,
                "title": "FrankenMermaid \(diagramType) diagram"
            ]
            let result = try await webView.callAsyncJavaScript(
                "return window.frankenExport(command)",
                arguments: ["command": command],
                in: nil,
                contentWorld: .page
            )
            guard let exported = result as? String, !exported.isEmpty else {
                throw CocoaError(.fileWriteUnknown, userInfo: [
                    NSLocalizedDescriptionKey: "The rendered diagram was not available to export."
                ])
            }
            contents = exported
        }

        let safeType = diagramType
            .lowercased()
            .replacingOccurrences(of: #"[^a-z0-9]+"#, with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        let filename = "FrankenMermaid-\(safeType.isEmpty ? "diagram" : safeType)-\(UUID().uuidString.prefix(8)).\(kind.fileExtension)"
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(filename)
        try Data(contents.utf8).write(to: url, options: .atomic)
        return url
    }

    static let sample = """
    flowchart TD
        Source[Mermaid source] --> Parse{Rust parser}
        Parse -->|valid| Layout[Deterministic layout]
        Parse -->|diagnostic| Lens[Source-aware lens]
        Layout --> SVG[Private SVG stage]
        Lens --> Source
        SVG --> Share[SVG · PNG · PDF]
    """
}

extension MermaidRendererModel: WKScriptMessageHandler {
    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        guard let payload = message.body as? [String: Any],
              let type = payload["type"] as? String else { return }
        switch type {
        case "ready":
            phase = .ready
            renderNow()
        case "result":
            guard (payload["requestID"] as? Int) == requestID else { return }
            elapsedMS = payload["elapsedMS"] as? Double
            diagramType = payload["diagramType"] as? String ?? "diagram"
            nodeCount = payload["nodeCount"] as? Int ?? 0
            edgeCount = payload["edgeCount"] as? Int ?? 0
            hasCurrentRenderedArtifact = true
            phase = .ready
#if DEBUG
            if debugExportProbePending {
                debugExportProbePending = false
                Task { [weak self] in
                    guard let self else { return }
                    do {
                        let url = try await self.prepareExport(.animatedHTML)
                        UserDefaults.standard.set(url.path, forKey: "FM_LAST_EXPORT_PROBE_PATH")
                    } catch {
                        UserDefaults.standard.set(error.localizedDescription, forKey: "FM_LAST_EXPORT_PROBE_ERROR")
                    }
                }
            }
#endif
        case "failure":
            guard (payload["requestID"] as? Int) == requestID else { return }
            hasCurrentRenderedArtifact = false
            phase = .failed(payload["message"] as? String ?? "Renderer failed")
        default:
            break
        }
    }
}

extension MermaidRendererModel: WKNavigationDelegate {
    func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let scheme = navigationAction.request.url?.scheme?.lowercased() else {
            decisionHandler(.cancel)
            return
        }
        decisionHandler(scheme == "frankenmermaid-resource" || scheme == "about" ? .allow : .cancel)
    }
}

final class MermaidResourceSchemeHandler: NSObject, WKURLSchemeHandler {
    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url,
              url.host == "bundle",
              let decodedPath = url.path.removingPercentEncoding else {
            fail(task, 400); return
        }
        let relativePath = String(decodedPath.drop(while: { $0 == "/" }))
        guard !relativePath.isEmpty,
              !relativePath.split(separator: "/").contains(".."),
              let root = Bundle.main.resourceURL?.appendingPathComponent("Renderer", isDirectory: true) else {
            fail(task, 403); return
        }
        let candidate = root.appendingPathComponent(relativePath).standardizedFileURL
        guard candidate.path.hasPrefix(root.standardizedFileURL.path + "/"),
              let bytes = try? Data(contentsOf: candidate) else {
            fail(task, 404); return
        }
        task.didReceive(URLResponse(
            url: url,
            mimeType: Self.mime(candidate.pathExtension),
            expectedContentLength: bytes.count,
            textEncodingName: ["html", "js"].contains(candidate.pathExtension) ? "utf-8" : nil
        ))
        task.didReceive(bytes)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}

    private func fail(_ task: WKURLSchemeTask, _ code: Int) {
        task.didFailWithError(NSError(domain: NSURLErrorDomain, code: code))
    }

    private static func mime(_ extensionName: String) -> String {
        switch extensionName.lowercased() {
        case "html": "text/html"
        case "js": "text/javascript"
        case "wasm": "application/wasm"
        case "json": "application/json"
        default: "application/octet-stream"
        }
    }
}

struct MermaidWebView: UIViewRepresentable {
    let webView: WKWebView
    func makeUIView(context: Context) -> WKWebView { webView }
    func updateUIView(_ uiView: WKWebView, context: Context) {}
}
