import SwiftUI
import UIKit

private enum StudioLane: String, CaseIterable, Identifiable {
    case code = "Code"
    case diagram = "Diagram"
    case inspect = "Inspect"
    var id: Self { self }
}

struct StudioView: View {
    @StateObject private var renderer = MermaidRendererModel()
    @State private var lane: StudioLane = .code
    @State private var editorFocused = false
    @State private var showingSamples = false
    @State private var sharedArtifact: SharedArtifact?
    @State private var exporting = false
    @State private var exportError: String?

    init() {
        let requested = ProcessInfo.processInfo.environment["FM_INITIAL_LANE"]
        _lane = State(initialValue: StudioLane(rawValue: requested ?? "") ?? .code)
        _showingSamples = State(
            initialValue: ProcessInfo.processInfo.environment["FM_SHOW_SAMPLES"] == "1"
        )
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                LaboratoryBackground()
                VStack(spacing: 14) {
                    masthead
                    if geometry.size.width >= 760 { wideStudio } else { compactStudio }
                    footer
                }
                .padding(.horizontal, geometry.size.width >= 760 ? 22 : 14)
                .padding(.top, 12)
            }
        }
        .onChange(of: renderer.source) { _, _ in renderer.scheduleRender() }
        .onReceive(NotificationCenter.default.publisher(for: .renderMermaidNow)) { _ in renderer.renderNow() }
        .sheet(isPresented: $showingSamples) {
            DiagramSampleGallery { sample in
                editorFocused = false
                renderer.source = sample.source
                lane = .diagram
            }
        }
        .sheet(item: $sharedArtifact) { artifact in
            SystemShareSheet(activityItems: [artifact.url])
        }
        .alert("Couldn’t prepare that export", isPresented: Binding(
            get: { exportError != nil },
            set: { if !$0 { exportError = nil } }
        )) {
            Button("OK", role: .cancel) { exportError = nil }
        } message: {
            Text(exportError ?? "Unknown export error")
        }
    }

    private var masthead: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { brand; Spacer(); statusPill }
            VStack(alignment: .leading, spacing: 10) { brand; statusPill }
        }
    }

    private var brand: some View {
        HStack(spacing: 12) {
            Image("MonsterIcon")
                .resizable()
                .scaledToFill()
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                .shadow(color: Lab.cyan.opacity(0.44), radius: 13)
                .accessibilityLabel("Friendly FrankenMermaid graph monster")
            VStack(alignment: .leading, spacing: 1) {
                Text("FRANKENMERMAID")
                    .font(.system(size: Lab.size(20), weight: .black, design: .monospaced))
                    .minimumScaleFactor(0.66)
                    .lineLimit(1)
                    .foregroundStyle(Lab.text)
                Text("GRAPH_HEART // private · offline · Rust")
                    .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                    .foregroundStyle(Lab.secondary)
            }
        }
    }

    private var statusPill: some View {
        HStack(spacing: 8) {
            Image(systemName: statusSymbol)
            Text(statusText).lineLimit(1)
            if renderer.phase == .rendering { ProgressView().controlSize(.small) }
        }
        .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
        .foregroundStyle(statusColor)
        .padding(.horizontal, 13)
        .padding(.vertical, 9)
        .background(Color.black.opacity(0.38), in: Capsule())
        .overlay(Capsule().stroke(statusColor.opacity(0.3)))
    }

    private var compactStudio: some View {
        VStack(spacing: 12) {
            Picker("Studio", selection: $lane) {
                ForEach(StudioLane.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            switch lane {
            case .code: editorPanel
            case .diagram: diagramPanel
            case .inspect: inspectorPanel
            }
        }
    }

    private var wideStudio: some View {
        HStack(spacing: 14) {
            editorPanel.frame(minWidth: 320, maxWidth: .infinity)
            diagramPanel.frame(minWidth: 380, maxWidth: .infinity)
        }
    }

    private var editorPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    LabLabel(text: "01 · The Graph Source")
                    Spacer()
                    Button {
                        editorFocused = false
                        showingSamples = true
                    } label: {
                        Label("Samples", systemImage: "square.grid.2x2")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .tint(Lab.cyan)
                    Text("\(renderer.source.utf8.count) bytes")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
                MermaidCodeEditor(text: $renderer.source, isFocused: $editorFocused)
                    .background(Color.black.opacity(0.42), in: RoundedRectangle(cornerRadius: 12))
                    .frame(minHeight: 320)
                HStack {
                    Button {
                        editorFocused = false
                        renderer.renderNow()
                    } label: {
                        Label("Energize Graph", systemImage: "point.3.connected.trianglepath.dotted")
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    Spacer()
                    Text("⌘R")
                        .font(.system(size: Lab.size(10), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
            }
        }
    }

    private var diagramPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    LabLabel(text: "02 · The Living Diagram")
                    Spacer()
                    if let elapsed = renderer.elapsedMS {
                        Text(String(format: "%.1f ms · %d nodes · %d edges",
                                    elapsed, renderer.nodeCount, renderer.edgeCount))
                            .font(.system(size: Lab.size(9), design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                    }
                }
                MermaidWebView(webView: renderer.webView)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12).stroke(Lab.stroke))
                    .frame(minHeight: 320)
                HStack {
                    Text(renderer.diagramType)
                        .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.cyan)
                    Spacer()
                    if exporting {
                        ProgressView()
                            .controlSize(.small)
                            .tint(Lab.cyan)
                            .accessibilityLabel("Preparing export")
                    } else {
                        Menu {
                            ForEach(MermaidExportKind.allCases) { kind in
                                Button {
                                    prepareExport(kind)
                                } label: {
                                    Label(kind.title, systemImage: kind.symbol)
                                }
                            }
                        } label: {
                            Label("Share", systemImage: "square.and.arrow.up")
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .tint(Lab.cyan)
                        .accessibilityHint("Share source, vector art, or a self-contained animated web page")
                    }
                }
            }
        }
    }

    private var inspectorPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 14) {
                LabLabel(text: "03 · The Cartography Bench")
                Label("Real nodes and edges only", systemImage: "point.3.filled.connected.trianglepath.dotted")
                Label("External diagram links stay disabled", systemImage: "lock.shield")
                Label("The exact Rust/WASM renderer is bundled", systemImage: "shippingbox")
                Text("Lens editing, diagnostics, themes, SVG/PNG/PDF export, Graph Deck, documents, widgets, and Shortcuts remain tracked milestones—not fake controls.")
                    .foregroundStyle(Lab.secondary)
            }
            .font(.system(size: Lab.size(13)))
            .foregroundStyle(Lab.text)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var footer: some View {
        VStack(spacing: 4) {
            Text("Rendered entirely on this device · no source or diagram is uploaded")
            Text("If you like this free app, please show your appreciation by trying out my paid skills site at [JeffreysSkills.md](https://jeffreys-skills.md).")
                .tint(Lab.cyan)
                .frame(maxWidth: 560)
        }
        .font(.system(size: Lab.size(9), design: .monospaced))
        .foregroundStyle(Lab.secondary.opacity(0.78))
        .multilineTextAlignment(.center)
        .padding(.bottom, 8)
    }

    private var statusText: String {
        switch renderer.phase {
        case .loading: "flooding the graph chamber"
        case .ready: "graph heart ready"
        case .rendering: "parse · IR · layout · SVG"
        case .failed(let message): message
        }
    }

    private var statusSymbol: String {
        switch renderer.phase {
        case .loading: "drop.triangle"
        case .ready: "checkmark.seal"
        case .rendering: "point.3.connected.trianglepath.dotted"
        case .failed: "exclamationmark.triangle"
        }
    }

    private var statusColor: Color {
        switch renderer.phase {
        case .loading, .rendering: Lab.amber
        case .ready: Lab.cyan
        case .failed: Lab.danger
        }
    }

    private func prepareExport(_ kind: MermaidExportKind) {
        guard !exporting else { return }
        exporting = true
        Task {
            do {
                sharedArtifact = SharedArtifact(url: try await renderer.prepareExport(kind))
            } catch {
                exportError = error.localizedDescription
            }
            exporting = false
        }
    }
}

private struct SharedArtifact: Identifiable {
    let url: URL
    var id: URL { url }
}

private struct SystemShareSheet: UIViewControllerRepresentable {
    let activityItems: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: activityItems, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
