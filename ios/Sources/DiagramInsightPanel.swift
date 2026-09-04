import SwiftUI

struct DiagramInsightPanel: View {
    @ObservedObject var renderer: MermaidRendererModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("SEMANTIC DESCRIPTION")
                Spacer()
                if !renderer.hasCurrentInsights { ProgressView().controlSize(.mini) }
            }
            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
            .foregroundStyle(Lab.secondary)

            Text(renderer.accessibilitySummary)
                .font(.system(size: Lab.size(11)))
                .foregroundStyle(Lab.text)
                .textSelection(.enabled)
                .accessibilityLabel("Diagram description")
                .accessibilityValue(renderer.accessibilitySummary)

            HStack {
                Text("SOURCE LENS")
                Spacer()
                Text("\(renderer.lensBindingCount) ELEMENTS")
                    .foregroundStyle(Lab.cyan)
            }
            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
            .foregroundStyle(Lab.secondary)

            if let binding = renderer.selectedLensBinding {
                LensSourceEditor(renderer: renderer, binding: binding)
                    .id("\(binding.id):\(binding.startByte ?? -1):\(binding.snippet ?? "")")
            } else {
                Label(
                    "Tap a rendered node, edge, or cluster to inspect its exact source binding",
                    systemImage: "cursorarrow.click"
                )
                    .font(.system(size: Lab.size(10)))
                    .foregroundStyle(Lab.secondary)
            }

            HStack {
                Text("RUST DIAGNOSTICS")
                Spacer()
                Text(renderer.diagnostics.isEmpty ? "CLEAR" : "\(renderer.diagnostics.count)")
                    .foregroundStyle(renderer.diagnostics.isEmpty ? Lab.emerald : Lab.amber)
            }
            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
            .foregroundStyle(Lab.secondary)

            if renderer.diagnostics.isEmpty {
                Label("No parser findings", systemImage: "checkmark.seal.fill")
                    .foregroundStyle(Lab.emerald)
            } else {
                ForEach(renderer.diagnostics) { diagnostic in
                    DiagnosticRow(diagnostic: diagnostic)
                }
            }
        }
    }
}

struct LensSourceEditor: View {
    @ObservedObject var renderer: MermaidRendererModel
    let binding: MermaidLensBinding
    @State private var replacement: String
    @State private var isApplying = false
    @State private var editError: String?

    init(renderer: MermaidRendererModel, binding: MermaidLensBinding) {
        self.renderer = renderer
        self.binding = binding
        _replacement = State(initialValue: binding.snippet ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label(binding.kind.uppercased(), systemImage: "scope")
                Spacer()
                if let line = binding.line {
                    Text("\(line):\(binding.column ?? 1)")
                }
            }
            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
            .foregroundStyle(Lab.amber)
            if let sourceID = binding.sourceID {
                Text(sourceID)
                    .font(.system(size: Lab.size(10), weight: .semibold, design: .monospaced))
                    .foregroundStyle(Lab.cyan)
            }
            if binding.exactSourceSnippet(in: renderer.source) != nil {
                TextEditor(text: $replacement)
                    .font(.system(size: Lab.size(10), design: .monospaced))
                    .foregroundStyle(Lab.text)
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: 72, maxHeight: 132)
                    .padding(6)
                    .background(Lab.panel, in: RoundedRectangle(cornerRadius: 8))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Lab.stroke))
                    .accessibilityLabel("Source replacement for \(binding.kind)")
                HStack {
                    Text("\(replacement.utf8.count) / 4096 bytes")
                        .font(.system(size: Lab.size(8), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                    Spacer()
                    Button {
                        applyEdit()
                    } label: {
                        if isApplying {
                            ProgressView().controlSize(.small)
                        } else {
                            Label("Apply exact edit", systemImage: "checkmark.seal")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .tint(Lab.cyan)
                    .disabled(
                        isApplying || replacement == binding.snippet ||
                        replacement.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                        replacement.utf8.count > 4_096
                    )
                    .accessibilityHint("Asks the bundled Rust source lens to replace only this bound source range")
                }
            } else if let snippet = binding.snippet {
                Text(snippet)
                    .font(.system(size: Lab.size(10), design: .monospaced))
                    .foregroundStyle(Lab.text)
                    .textSelection(.enabled)
                    .lineLimit(4)
                Label(
                    "This element has no current exact UTF-8 source range, so editing is disabled.",
                    systemImage: "lock.shield"
                )
                    .font(.system(size: Lab.size(9)))
                    .foregroundStyle(Lab.secondary)
            }
        }
        .padding(9)
        .background(Lab.statusBackground, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Lab.amber.opacity(0.34)))
        .alert("Source edit wasn’t applied", isPresented: Binding(
            get: { editError != nil },
            set: { if !$0 { editError = nil } }
        )) {
            Button("OK", role: .cancel) { editError = nil }
        } message: {
            Text(editError ?? "Unknown source-lens error")
        }
    }

    private func applyEdit() {
        guard !isApplying else { return }
        isApplying = true
        Task {
            do {
                try await renderer.applySelectedLensEdit(replacement: replacement)
            } catch {
                editError = error.localizedDescription
            }
            isApplying = false
        }
    }
}

private struct DiagnosticRow: View {
    let diagnostic: MermaidDiagnostic

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 7) {
                Image(systemName: diagnostic.severity == "error"
                      ? "xmark.octagon.fill" : "exclamationmark.triangle.fill")
                    .foregroundStyle(diagnostic.severity == "error" ? Lab.danger : Lab.amber)
                Text(diagnostic.message)
                    .fontWeight(.semibold)
                Spacer(minLength: 4)
                if let line = diagnostic.line {
                    Text("\(line):\(diagnostic.column ?? 1)")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
            }
            if let suggestion = diagnostic.suggestion {
                Text(suggestion)
                    .foregroundStyle(Lab.secondary)
                    .padding(.leading, 24)
            }
        }
        .font(.system(size: Lab.size(10)))
        .accessibilityElement(children: .combine)
    }
}
