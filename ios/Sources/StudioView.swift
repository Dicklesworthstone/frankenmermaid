import SwiftUI
import UIKit
import UniformTypeIdentifiers

private extension UTType {
    static let mermaidSource = UTType(
        exportedAs: "com.frankenmermaid.source",
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
    @AppStorage(Lab.textScaleStorageKey) private var uiTextScale = Lab.defaultTextScale
    @AppStorage("diagramTheme") private var diagramTheme = "dark"
    @AppStorage("renderFontScale") private var renderFontScale = 1.0
    @AppStorage("diagramShadows") private var diagramShadows = true
    @AppStorage("diagramGradients") private var diagramGradients = true
    @AppStorage("diagramCornerRadius") private var diagramCornerRadius = 10.0
    @AppStorage("diagramPadding") private var diagramPadding = 18.0
    @StateObject private var renderer = MermaidRendererModel()
    @StateObject private var sourceHistory = MermaidSourceHistory()
    @State private var lane: StudioLane = .code
    @State private var editorFocused = false
    @State private var showingSamples = false
    @State private var showingSourceImporter = false
    @State private var sharedArtifact: SharedArtifact?
    @State private var exporting = false
    @State private var exportError: String?
    @State private var deckError: String?
    @State private var sourceImportError: String?
    @State private var compactLensBinding: MermaidLensBinding?

    init() {
        let requested = ProcessInfo.processInfo.environment["FM_INITIAL_LANE"]
        _lane = State(initialValue: StudioLane(rawValue: requested ?? "") ?? .code)
        _showingSamples = State(
            initialValue: ProcessInfo.processInfo.environment["FM_SHOW_SAMPLES"] == "1"
        )
    }

    var body: some View {
        Group {
            if renderer.isPresentingDeck {
                deckTheater
            } else {
                alertedStudio
            }
        }
        .preferredColorScheme((LabAppearance(rawValue: appearance) ?? .dark).colorScheme)
        .alert("Graph Deck couldn’t continue", isPresented: Binding(
            get: { deckError != nil },
            set: { if !$0 { deckError = nil } }
        )) {
            Button("OK", role: .cancel) { deckError = nil }
        } message: {
            Text(deckError ?? "Unknown Graph Deck error")
        }
    }

    private var deckTheater: some View {
        ZStack {
            Color(red: 0.012, green: 0.047, blue: 0.055)
                .ignoresSafeArea()
            MermaidWebView(webView: renderer.webView)
                .ignoresSafeArea()
                .accessibilityIdentifier("graph-deck-theater")

            VStack(spacing: 0) {
                HStack(alignment: .top, spacing: 12) {
                    Button {
                        Task { await renderer.stopDeckPresentation() }
                    } label: {
                        Label("Done", systemImage: "xmark")
                            .font(.system(size: Lab.size(12), weight: .bold))
                            .padding(.horizontal, 13)
                            .frame(minHeight: 44)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.black.opacity(0.72))
                    .accessibilityIdentifier("close-graph-deck")

                    VStack(alignment: .leading, spacing: 3) {
                        Text(renderer.deckScene?.title ?? renderer.deckSummary?.title ?? "Graph Deck")
                            .font(.system(size: Lab.size(17), weight: .black, design: .rounded))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if let caption = renderer.deckScene?.caption, !caption.isEmpty {
                            Text(caption)
                                .font(.system(size: Lab.size(11), weight: .medium))
                                .foregroundStyle(.white.opacity(0.72))
                                .lineLimit(2)
                        }
                    }
                    Spacer(minLength: 8)
                    Text(renderer.deckScene?.position ?? "")
                        .font(.system(size: Lab.size(11), weight: .bold, design: .monospaced))
                        .foregroundStyle(.white.opacity(0.72))
                        .padding(.top, 12)
                        .accessibilityIdentifier("graph-deck-position")
                }
                .padding(.horizontal, 14)
                .padding(.top, 8)
                .padding(.bottom, 20)
                .background(
                    LinearGradient(
                        colors: [.black.opacity(0.74), .black.opacity(0)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )

                Spacer()

                HStack(spacing: 12) {
                    Button {
                        performDeckAction(.previous)
                    } label: {
                        Label("Previous", systemImage: "chevron.left")
                            .frame(minWidth: 74, minHeight: 48)
                    }
                    .accessibilityIdentifier("graph-deck-previous")

                    if renderer.deckSummary?.overviewEnabled == true {
                        Button {
                            performDeckAction(.overview)
                        } label: {
                            Label("Whole graph", systemImage: "square.dashed.inset.filled")
                                .frame(minHeight: 48)
                        }
                        .accessibilityIdentifier("graph-deck-overview")
                    }

                    Button {
                        performDeckAction(.next)
                    } label: {
                        Label("Next", systemImage: "chevron.right")
                            .labelStyle(.titleAndIcon)
                            .frame(minWidth: 74, minHeight: 48)
                    }
                    .accessibilityIdentifier("graph-deck-next")
                }
                .font(.system(size: Lab.size(12), weight: .bold))
                .buttonStyle(.borderedProminent)
                .tint(.black.opacity(0.76))
                .foregroundStyle(.white)
                .padding(.horizontal, 14)
                .padding(.top, 24)
                .padding(.bottom, 10)
                .background(
                    LinearGradient(
                        colors: [.black.opacity(0), .black.opacity(0.78)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
            }
        }
        .statusBarHidden(true)
        .persistentSystemOverlays(.hidden)
    }

    private var studioLayout: some View {
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
    }

    private var renderObservedStudio: some View {
        studioLayout
        .onChange(of: renderer.source) { previousSource, source in
            sourceHistory.recordChange(
                from: previousSource,
                to: source,
                continuous: editorFocused
            )
            renderer.scheduleRender()
        }
        .onChange(of: editorFocused) { _, isFocused in
            if !isFocused { sourceHistory.endContinuousEditing() }
        }
        .onChange(of: renderer.selectedLensBinding) { _, binding in
            guard horizontalSizeClass == .compact else { return }
            compactLensBinding = binding
        }
        .onChange(of: uiTextScale) { _, value in
            let clamped = Lab.clampedTextScale(value)
            if clamped != value { uiTextScale = clamped }
        }
        .onAppear {
            uiTextScale = Lab.clampedTextScale(uiTextScale)
            renderFontScale = clampedRenderFontScale(renderFontScale)
            applyRenderStyle(renderImmediately: false)
        }
        .onReceive(NotificationCenter.default.publisher(for: .renderMermaidNow)) { _ in
            renderAndRevealDiagram()
        }
    }

    private var styleObservedStudio: some View {
        renderObservedStudio
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
    }

    private var presentedStudio: some View {
        styleObservedStudio
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
        .sheet(item: $compactLensBinding) { binding in
            compactLensSheet(for: binding)
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
        .onOpenURL { url in
            guard url.isFileURL else { return }
            openSource(url)
        }
    }

    private var alertedStudio: some View {
        presentedStudio
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
    }

    private func compactLensSheet(for binding: MermaidLensBinding) -> some View {
        NavigationStack {
            ZStack {
                LaboratoryBackground()
                ScrollView {
                    LensSourceEditor(renderer: renderer, binding: binding)
                        .padding(18)
                }
            }
            .navigationTitle("Source Lens")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { compactLensBinding = nil }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
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
        .accessibilityIdentifier("frankenmermaid-brand")
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
#if targetEnvironment(macCatalyst)
        HStack(spacing: 14) {
            editorPanel.frame(minWidth: 320, maxWidth: .infinity)
            diagramPanel.frame(minWidth: 380, maxWidth: .infinity)
        }
#else
        VStack(spacing: 12) {
            Picker("Studio", selection: $lane) {
                ForEach(StudioLane.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)

            HStack(spacing: 14) {
                switch lane {
                case .code:
                    editorPanel.frame(minWidth: 320, maxWidth: .infinity)
                    diagramPanel.frame(minWidth: 380, maxWidth: .infinity)
                case .diagram:
                    diagramPanel.frame(minWidth: 380, maxWidth: .infinity)
                    inspectorPanel.frame(minWidth: 300, maxWidth: .infinity)
                case .inspect:
                    inspectorPanel.frame(minWidth: 300, maxWidth: .infinity)
                    diagramPanel.frame(minWidth: 380, maxWidth: .infinity)
                }
            }
        }
#endif
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
                        .accessibilityIdentifier("source-editor-panel")
                    Spacer()
                    Text("\(renderer.source.utf8.count) bytes")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
                HStack(spacing: 8) {
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
                    .controlSize(.large)
                    .tint(Lab.cyan)
                    Spacer(minLength: 4)
                    Button {
                        undoSourceChange()
                    } label: {
                        Label("Undo", systemImage: "arrow.uturn.backward")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)
                    .tint(Lab.amber)
                    .disabled(!sourceHistory.canUndo)
                    .frame(minHeight: 44)
                    .keyboardShortcut("z", modifiers: .command)
                    .accessibilityIdentifier("undo-source-change")
                    .accessibilityHint("Restore the source before the most recent edit, import, sample, or source-lens change")

                    Button {
                        redoSourceChange()
                    } label: {
                        Label("Redo", systemImage: "arrow.uturn.forward")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)
                    .tint(Lab.amber)
                    .disabled(!sourceHistory.canRedo)
                    .frame(minHeight: 44)
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                    .accessibilityIdentifier("redo-source-change")
                    .accessibilityHint("Reapply the last undone Mermaid source change")
                }
                MermaidCodeEditor(text: $renderer.source, isFocused: $editorFocused)
                    .background(Lab.statusBackground.opacity(0.58), in: RoundedRectangle(cornerRadius: 12))
                    .frame(minHeight: 320)
#if !targetEnvironment(macCatalyst)
                if horizontalSizeClass == .compact {
                    HStack {
                        Button {
                            renderAndRevealDiagram()
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
                    .accessibilityIdentifier("live-diagram-stage")
                    .accessibilityLabel("Live Mermaid diagram preview")
                HStack {
                    Text(renderer.diagramType)
                        .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.cyan)
                    Spacer()
                    if let deck = renderer.deckSummary {
                        Button {
                            startDeckPresentation()
                        } label: {
                            Label("Present", systemImage: "play.rectangle.fill")
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .tint(Lab.amber)
                        .accessibilityIdentifier("present-graph-deck")
                        .accessibilityHint(
                            "Open \(deck.title), \(deck.sceneCount) presentation scenes, in the full-screen theater"
                        )
                    }
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
                                .disabled(
                                    kind != .source && (
                                        !renderer.hasCurrentRenderedArtifact ||
                                        (kind == .deckHTML && renderer.deckSummary == nil)
                                    )
                                )
                            }
                        } label: {
                            Label("Share", systemImage: "square.and.arrow.up")
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .tint(Lab.cyan)
                        .accessibilityHint(
                            "Share source, SVG, PNG, PDF, an animated page, or a self-contained Graph Deck presentation"
                        )
                    }
                }
            }
        }
    }

    private func renderAndRevealDiagram() {
        editorFocused = false
        renderer.renderNow()
        withAnimation(.snappy) { lane = .diagram }
    }

    private func startDeckPresentation() {
        editorFocused = false
        Task {
            do {
                try await renderer.startDeckPresentation()
            } catch {
                deckError = error.localizedDescription
            }
        }
    }

    private func performDeckAction(_ action: MermaidDeckAction) {
        Task {
            do {
                try await renderer.performDeckAction(action)
            } catch {
                deckError = error.localizedDescription
            }
        }
    }

    private func undoSourceChange() {
        guard let source = sourceHistory.undo(currentSource: renderer.source) else { return }
        renderer.source = source
    }

    private func redoSourceChange() {
        guard let source = sourceHistory.redo(currentSource: renderer.source) else { return }
        renderer.source = source
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
        .accessibilityIdentifier("diagram-inspector-panel")
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
        Text("Rendered entirely on this device · no source or diagram is uploaded")
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
            MermaidRenderStyle(
                theme: diagramTheme,
                fontSize: 14.0 * clampedRenderFontScale(renderFontScale),
                padding: diagramPadding,
                shadows: diagramShadows,
                roundedCorners: diagramCornerRadius,
                nodeGradients: diagramGradients
            ),
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
