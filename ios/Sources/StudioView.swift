import SwiftUI
import UIKit
import UniformTypeIdentifiers

private extension UTType {
    static let mermaidSource = UTType(
        importedAs: "com.frankenmermaid.source",
        conformingTo: .plainText
    )
}

private enum StudioLane: String, CaseIterable, Identifiable {
    case code = "Code"
    case diagram = "Diagram"
    case inspect = "Inspect"
    var id: Self { self }
}

struct StudioView: View {
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @AppStorage(LabAppearance.storageKey) private var appearance = LabAppearance.dark.rawValue
    @AppStorage("diagramTheme") private var diagramTheme = "dark"
    @AppStorage("renderFontScale") private var renderFontScale = 1.0
    @AppStorage("diagramShadows") private var diagramShadows = true
    @AppStorage("diagramGradients") private var diagramGradients = true
    @AppStorage("diagramCornerRadius") private var diagramCornerRadius = 10.0
    @AppStorage("diagramPadding") private var diagramPadding = 18.0
    @StateObject private var renderer = MermaidRendererModel()
    @State private var lane: StudioLane = .code
    @State private var editorFocused = false
    @State private var showingSamples = false
    @State private var showingSourceImporter = false
    @State private var sharedArtifact: SharedArtifact?
    @State private var exporting = false
    @State private var exportError: String?
    @State private var sourceImportError: String?

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
#if targetEnvironment(macCatalyst)
                    if geometry.size.width >= 1_080 {
                        desktopStudio
                    } else if geometry.size.width >= 760 {
                        wideStudio
                    } else {
                        compactStudio
                    }
#else
                    if geometry.size.width >= 760 { wideStudio } else { compactStudio }
#endif
                    footer
                }
                .padding(.horizontal, geometry.size.width >= 760 ? 22 : 14)
                .padding(.top, 12)
            }
        }
        .onChange(of: renderer.source) { _, _ in renderer.scheduleRender() }
        .onChange(of: diagramTheme) { _, _ in applyRenderStyle() }
        .onChange(of: renderFontScale) { _, value in
            let clamped = clampedRenderFontScale(value)
            if clamped != value { renderFontScale = clamped }
            applyRenderStyle()
        }
        .onChange(of: diagramShadows) { _, _ in applyRenderStyle() }
        .onChange(of: diagramGradients) { _, _ in applyRenderStyle() }
        .onChange(of: diagramCornerRadius) { _, _ in applyRenderStyle() }
        .onChange(of: diagramPadding) { _, _ in applyRenderStyle() }
        .onAppear {
            renderFontScale = clampedRenderFontScale(renderFontScale)
            applyRenderStyle(renderImmediately: false)
        }
        .onReceive(NotificationCenter.default.publisher(for: .renderMermaidNow)) { _ in renderer.renderNow() }
        .sheet(isPresented: $showingSamples) {
            DiagramSampleGallery { sample in
                editorFocused = false
                renderer.source = sample.source
                lane = .diagram
            }
        }
        .sheet(item: $sharedArtifact) { artifact in
            SystemShareSheet(fileURL: artifact.url)
        }
        .fileImporter(
            isPresented: $showingSourceImporter,
            allowedContentTypes: [.mermaidSource, .plainText],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case .success(let urls):
                guard let url = urls.first else { return }
                openSource(url)
            case .failure(let error):
                sourceImportError = error.localizedDescription
            }
        }
        .alert("Couldn’t prepare that export", isPresented: Binding(
            get: { exportError != nil },
            set: { if !$0 { exportError = nil } }
        )) {
            Button("OK", role: .cancel) { exportError = nil }
        } message: {
            Text(exportError ?? "Unknown export error")
        }
        .alert("Couldn’t open that source", isPresented: Binding(
            get: { sourceImportError != nil },
            set: { if !$0 { sourceImportError = nil } }
        )) {
            Button("OK", role: .cancel) { sourceImportError = nil }
        } message: {
            Text(sourceImportError ?? "Unknown source import error")
        }
        .preferredColorScheme((LabAppearance(rawValue: appearance) ?? .dark).colorScheme)
    }

    private var masthead: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { brand; Spacer(); statusControls }
            VStack(alignment: .leading, spacing: 10) { brand; statusControls }
        }
    }

    private var statusControls: some View {
        HStack(spacing: 8) {
            LabAppearanceButton(selection: $appearance)
            statusPill
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
                FrankenWordmark(
                    productInitial: "M",
                    productRemainder: "ERMAID",
                    fullName: "FrankenMermaid"
                )
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
        .background(Lab.statusBackground, in: Capsule())
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

    private var desktopStudio: some View {
        HStack(spacing: 14) {
            editorPanel.frame(minWidth: 320, maxWidth: .infinity)
            diagramPanel.frame(minWidth: 380, maxWidth: .infinity)
            inspectorPanel.frame(width: 286)
        }
    }

    private var editorPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    LabLabel(text: "01 · The Graph Source")
                    Spacer()
                    Menu {
                        Button {
                            editorFocused = false
                            showingSourceImporter = true
                        } label: {
                            Label("Open Mermaid file…", systemImage: "folder")
                        }
                        Button {
                            editorFocused = false
                            showingSamples = true
                        } label: {
                            Label("Sample gallery", systemImage: "square.grid.2x2")
                        }
                    } label: {
                        Label("Source", systemImage: "doc.badge.gearshape")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .tint(Lab.cyan)
                    Text("\(renderer.source.utf8.count) bytes")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
                MermaidCodeEditor(text: $renderer.source, isFocused: $editorFocused)
                    .background(Lab.statusBackground.opacity(0.58), in: RoundedRectangle(cornerRadius: 12))
                    .frame(minHeight: 320)
#if !targetEnvironment(macCatalyst)
                if horizontalSizeClass == .compact {
                    HStack {
                        Button {
                            editorFocused = false
                            renderer.renderNow()
                            withAnimation(.snappy) { lane = .diagram }
                        } label: {
                            Label("View Diagram", systemImage: "point.3.connected.trianglepath.dotted")
                        }
                        .buttonStyle(PrimaryButtonStyle())
                        Spacer()
                        Text("⌘R")
                            .font(.system(size: Lab.size(10), design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                    }
                }
#endif
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
                                .disabled(kind != .source && !renderer.hasCurrentRenderedArtifact)
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
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    LabLabel(text: "03 · Style Laboratory")

                    VStack(alignment: .leading, spacing: 7) {
                        Text("THEME")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                        Picker("Diagram theme", selection: $diagramTheme) {
                            ForEach(Self.themes, id: \.id) { theme in
                                Text(theme.name).tag(theme.id)
                            }
                        }
                        .pickerStyle(.menu)
                        .tint(Lab.cyan)
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("RENDERED TEXT SIZE")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                        renderFontSizeControl
                    }

                    Divider().background(Lab.stroke)

                    Toggle("Cast node shadows", isOn: $diagramShadows)
                    Toggle("Illuminate gradients", isOn: $diagramGradients)

                    VStack(alignment: .leading, spacing: 7) {
                        HStack {
                            Text("CORNER ENERGY")
                            Spacer()
                            Text("\(Int(diagramCornerRadius.rounded()))")
                        }
                        .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                        Slider(value: $diagramCornerRadius, in: 0...24, step: 2)
                            .tint(Lab.cyan)
                    }

                    VStack(alignment: .leading, spacing: 7) {
                        HStack {
                            Text("CANVAS BREATHING ROOM")
                            Spacer()
                            Text("\(Int(diagramPadding.rounded()))")
                        }
                        .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                        Slider(value: $diagramPadding, in: 8...48, step: 2)
                            .tint(Lab.cyan)
                    }

                    Divider().background(Lab.stroke)

                    DiagramInsightPanel(renderer: renderer)

                    Divider().background(Lab.stroke)

                    Label("Real nodes and edges only", systemImage: "point.3.filled.connected.trianglepath.dotted")
                    Label("External links stay disabled", systemImage: "lock.shield")
                    Label("Exact bundled Rust/WASM renderer", systemImage: "shippingbox")
                }
                .font(.system(size: Lab.size(12)))
                .foregroundStyle(Lab.text)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var renderFontSizeControl: some View {
        HStack(spacing: 8) {
            Button {
                renderFontScale = clampedRenderFontScale(renderFontScale - 0.1)
            } label: {
                Image(systemName: "textformat.size.smaller")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.bordered)
            .disabled(renderFontScale <= 0.7)
            .accessibilityLabel("Decrease rendered diagram text size")

            Button {
                renderFontScale = 1.0
            } label: {
                Text("\(Int((renderFontScale * 100).rounded()))%")
                    .font(.system(size: Lab.size(11), weight: .black, design: .monospaced))
                    .frame(minWidth: 48)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel(
                "Rendered diagram text size \(Int((renderFontScale * 100).rounded())) percent. Reset to 100 percent"
            )

            Button {
                renderFontScale = clampedRenderFontScale(renderFontScale + 0.1)
            } label: {
                Image(systemName: "textformat.size.larger")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.bordered)
            .disabled(renderFontScale >= 1.6)
            .accessibilityLabel("Increase rendered diagram text size")
        }
        .tint(Lab.cyan)
    }

    private var footer: some View {
        VStack(spacing: 4) {
            Text("Rendered entirely on this device · no source or diagram is uploaded")
            Text(
                "If you like this free app, please show your appreciation by trying out my paid skills site at " +
                    "[JeffreysSkills.md](https://jeffreys-skills.md)."
            )
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

    private func applyRenderStyle(renderImmediately: Bool = true) {
        renderer.updateStyle(
            theme: diagramTheme,
            fontSize: 14.0 * clampedRenderFontScale(renderFontScale),
            padding: diagramPadding,
            shadows: diagramShadows,
            roundedCorners: diagramCornerRadius,
            nodeGradients: diagramGradients,
            renderImmediately: renderImmediately
        )
    }

    private func clampedRenderFontScale(_ value: Double) -> Double {
        min(1.6, max(0.7, (value * 10).rounded() / 10))
    }

    private static let themes: [(id: String, name: String)] = [
        ("dark", "Dark Laboratory"),
        ("neon", "Neon Current"),
        ("blueprint", "Blueprint"),
        ("forest", "Forest"),
        ("pastel", "Pastel"),
        ("corporate", "Corporate"),
        ("neutral", "Neutral"),
        ("monochrome", "Monochrome"),
        ("high-contrast", "High Contrast"),
        ("default", "Default")
    ]

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

    private func openSource(_ url: URL) {
        Task {
            do {
                let source = try await MermaidSourceLoader.load(from: url)
                editorFocused = false
                renderer.source = source
                lane = .code
            } catch {
                sourceImportError = error.localizedDescription
            }
        }
    }
}

private struct SharedArtifact: Identifiable {
    let url: URL
    var id: URL { url }
}

private struct SystemShareSheet: UIViewControllerRepresentable {
    let fileURL: URL

    func makeUIViewController(context: Context) -> UIActivityViewController {
        let contentType = UTType(filenameExtension: fileURL.pathExtension) ?? .data
        let provider = NSItemProvider()
        provider.suggestedName = fileURL.lastPathComponent
        provider.registerFileRepresentation(
            forTypeIdentifier: contentType.identifier,
            fileOptions: [],
            visibility: .all
        ) { completion in
            // Register only a copied file representation. A bare HTML URL can
            // otherwise be interpreted as a web link or text by destinations.
            completion(fileURL, false, nil)
            return nil
        }
        let configuration = UIActivityItemsConfiguration(itemProviders: [provider])
        return UIActivityViewController(activityItemsConfiguration: configuration)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
