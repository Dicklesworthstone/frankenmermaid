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
                VStack(alignment: .leading, spacing: 5) {
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
                    if let snippet = binding.snippet {
                        Text(snippet)
                            .font(.system(size: Lab.size(10), design: .monospaced))
                            .foregroundStyle(Lab.text)
                            .textSelection(.enabled)
                            .lineLimit(4)
                    }
                }
                .padding(9)
                .background(Lab.statusBackground, in: RoundedRectangle(cornerRadius: 10))
                .overlay(RoundedRectangle(cornerRadius: 10).stroke(Lab.amber.opacity(0.34)))
                .accessibilityElement(children: .combine)
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
