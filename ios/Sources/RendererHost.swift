import Foundation
import SwiftUI
import WebKit

enum GraphPhase: Equatable {
    case loading, ready, rendering
    case failed(String)
}

struct MermaidDiagnostic: Identifiable, Equatable, Sendable {
    let id: String
    let severity: String
    let category: String
    let message: String
    let suggestion: String?
    let line: Int?
    let column: Int?

    init?(payload: [String: Any], index: Int) {
        guard let message = payload["message"] as? String, !message.isEmpty else { return nil }
        let rawSeverity = (payload["severity"] as? String ?? "info").lowercased()
        severity = ["error", "warning", "info", "hint"].contains(rawSeverity) ? rawSeverity : "info"
        category = (payload["category"] as? String ?? "parser").lowercased()
        self.message = message
        let rawSuggestion = payload["suggestion"] as? String
        suggestion = rawSuggestion.flatMap { $0.isEmpty ? nil : $0 }
        let rawLine = payload["line"] as? Int ?? 0
        let rawColumn = payload["column"] as? Int ?? 0
        line = rawLine > 0 ? rawLine : nil
        column = rawColumn > 0 ? rawColumn : nil
        id = "\(index):\(severity):\(line ?? 0):\(column ?? 0):\(message)"
    }
}

struct MermaidRenderStyle: Equatable, Sendable {
    let theme: String
    let fontSize: Double
    let padding: Double
    let shadows: Bool
    let roundedCorners: Double
    let nodeGradients: Bool
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
    @Published private(set) var accessibilitySummary = "Render a diagram to create its semantic description."
    @Published private(set) var diagnostics: [MermaidDiagnostic] = []
    @Published private(set) var hasCurrentInsights = false
    @Published private(set) var lensBindingCount = 0
    @Published private(set) var selectedLensBinding: MermaidLensBinding?

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

    func updateStyle(_ style: MermaidRenderStyle, renderImmediately: Bool) {
        theme = style.theme
        fontSize = min(22, max(9, style.fontSize))
        padding = min(48, max(8, style.padding))
        shadows = style.shadows
        roundedCorners = min(24, max(0, style.roundedCorners))
        nodeGradients = style.nodeGradients
        hasCurrentRenderedArtifact = false
        hasCurrentInsights = false
        lensBindingCount = 0
        selectedLensBinding = nil
        if renderImmediately { renderNow() }
    }

    func scheduleRender() {
        scheduledRender?.cancel()
        // An export made during the debounce window would otherwise silently
        // contain the previous source. Mark the rendered artifact stale as soon
        // as editing begins, rather than only when WebKit starts rendering.
        hasCurrentRenderedArtifact = false
        lensBindingCount = 0
        selectedLensBinding = nil
        hasCurrentInsights = false
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
        lensBindingCount = 0
        selectedLensBinding = nil
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
        let bytes: Data
        if kind == .source {
            bytes = Data(source.utf8)
        } else {
            guard hasCurrentRenderedArtifact, phase == .ready else {
                throw CocoaError(.fileWriteUnknown, userInfo: [
                    NSLocalizedDescriptionKey:
                        "Wait for the current diagram to finish rendering before exporting it."
                ])
            }
            if kind == .pdf {
                bytes = try await webView.pdf()
            } else {
                let command: [String: Any] = [
                    "kind": kind.rawValue,
                    "title": "FrankenMermaid \(diagramType) diagram"
                ]
                let result = try await webView.callAsyncJavaScript(
                    "return await window.frankenExport(command)",
                    arguments: ["command": command],
                    in: nil,
                    contentWorld: .page
                )
                guard let exported = result as? String, !exported.isEmpty else {
                    throw MermaidExportError.missingArtifact
                }
                bytes = kind == .png
                    ? try MermaidExportCodec.decodePNGDataURL(exported)
                    : Data(exported.utf8)
            }
        }
        try MermaidExportCodec.validateSize(bytes)

        let safeType = diagramType
            .lowercased()
            .replacingOccurrences(of: #"[^a-z0-9]+"#, with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        let filename = "FrankenMermaid-\(safeType.isEmpty ? "diagram" : safeType)-" +
            "\(UUID().uuidString.prefix(8)).\(kind.fileExtension)"
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(filename)
        try bytes.write(to: url, options: .atomic)
        return url
    }

    func applySelectedLensEdit(replacement: String) async throws {
        guard phase == .ready, hasCurrentRenderedArtifact,
              let binding = selectedLensBinding,
              binding.exactSourceSnippet(in: source) != nil else {
            throw MermaidLensEditError.unavailable
        }
        let trimmed = replacement.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw MermaidLensEditError.emptyReplacement }
        guard replacement.utf8.count <= 4_096 else {
            throw MermaidLensEditError.replacementTooLarge
        }

        let expectedSource = source
        let expectedRequestID = requestID
        let command: [String: Any] = [
            "requestID": expectedRequestID,
            "source": expectedSource,
            "elementID": binding.id,
            "replacement": replacement
        ]
        let result = try await webView.callAsyncJavaScript(
            "return window.frankenApplyLensEdit(command)",
            arguments: ["command": command],
            in: nil,
            contentWorld: .page
        )
        guard requestID == expectedRequestID, source == expectedSource else {
            throw MermaidLensEditError.staleSelection
        }
        guard let receipt = result as? [String: Any],
              receipt["requestID"] as? Int == expectedRequestID,
              receipt["elementID"] as? String == binding.id,
              receipt["previousSnippet"] as? String == binding.snippet,
              receipt["replacement"] as? String == replacement,
              let updatedSource = receipt["updatedSource"] as? String,
              !updatedSource.isEmpty,
              updatedSource.utf8.count <= MermaidSourceLoader.maximumBytes else {
            throw MermaidLensEditError.invalidReceipt
        }
        selectedLensBinding = nil
        source = updatedSource
    }

    private func receiveLensSelection(_ payload: [String: Any]) {
        guard (payload["requestID"] as? Int) == requestID else { return }
        selectedLensBinding = (payload["binding"] as? [String: Any]).flatMap {
            MermaidLensBinding(payload: $0)
        }
    }

    private func receiveRenderResult(_ payload: [String: Any]) {
        guard (payload["requestID"] as? Int) == requestID else { return }
        elapsedMS = payload["elapsedMS"] as? Double
        diagramType = payload["diagramType"] as? String ?? "diagram"
        nodeCount = payload["nodeCount"] as? Int ?? 0
        edgeCount = payload["edgeCount"] as? Int ?? 0
        lensBindingCount = payload["lensBindingCount"] as? Int ?? 0
        accessibilitySummary = payload["accessibilitySummary"] as? String
            ?? "The renderer did not return a semantic diagram description."
        let rawDiagnostics = payload["diagnostics"] as? [[String: Any]] ?? []
        diagnostics = rawDiagnostics.enumerated().compactMap { index, diagnostic in
            MermaidDiagnostic(payload: diagnostic, index: index)
        }
        hasCurrentRenderedArtifact = true
        hasCurrentInsights = true
        phase = .ready
#if DEBUG
        runDebugExportProbeIfNeeded()
#endif
    }

#if DEBUG
    private func runDebugExportProbeIfNeeded() {
        guard debugExportProbePending else { return }
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
            receiveRenderResult(payload)
        case "lens.selection":
            receiveLensSelection(payload)
        case "failure":
            guard (payload["requestID"] as? Int) == requestID else { return }
            hasCurrentRenderedArtifact = false
            hasCurrentInsights = false
            lensBindingCount = 0
            selectedLensBinding = nil
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
