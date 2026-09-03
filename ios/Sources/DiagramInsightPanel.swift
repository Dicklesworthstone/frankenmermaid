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
