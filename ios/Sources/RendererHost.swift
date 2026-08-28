import Foundation
import SwiftUI
import WebKit

enum GraphPhase: Equatable {
    case loading
    case ready
    case rendering
    case failed(String)
}

@MainActor
final class MermaidRendererModel: NSObject, ObservableObject {
    @Published var source = MermaidRendererModel.sample
    @Published private(set) var phase: GraphPhase = .loading
    @Published private(set) var elapsedMS: Double?
    @Published private(set) var diagramType = "detecting"
    @Published private(set) var nodeCount = 0
    @Published private(set) var edgeCount = 0

    let webView: WKWebView
    private var requestID = 0
    private var scheduledRender: Task<Void, Never>?

    override init() {
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

    func scheduleRender() {
        scheduledRender?.cancel()
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
        let command: [String: Any] = ["requestID": requestID, "source": source]
        phase = .rendering
        Task { [weak self, weak webView] in
            do {
                _ = try await webView?.callAsyncJavaScript(
                    "return await window.frankenRender(command)",
                    arguments: ["command": command],
                    in: nil,
                    contentWorld: .page
                )
            } catch {
                self?.phase = .failed(error.localizedDescription)
            }
        }
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
            phase = .ready
        case "failure":
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
