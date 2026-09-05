import SwiftUI
import UIKit

enum LabAppearance: String {
    static let storageKey = "frankenmermaid.appearance"
    case dark
    case light
    var colorScheme: ColorScheme { self == .dark ? .dark : .light }
}

enum Lab {
    static let textScaleStorageKey = "frankenmermaid.uiTextScale"
    static let defaultTextScale = 1.0
    static let minimumTextScale = 0.8
    static let maximumTextScale = 1.6
    static let textScaleStep = 0.1

    static let background = adaptive(dark: UIColor(red: 0.002, green: 0.025, blue: 0.033, alpha: 1), light: UIColor(red: 0.925, green: 0.965, blue: 0.975, alpha: 1))
    static let panel = adaptive(dark: UIColor(white: 0, alpha: 0.52), light: UIColor(red: 0.985, green: 0.997, blue: 1, alpha: 0.97))
    static let stroke = adaptive(dark: UIColor(white: 1, alpha: 0.08), light: UIColor(red: 0.025, green: 0.22, blue: 0.29, alpha: 0.18))
    static let emerald = adaptive(dark: UIColor(red: 0.20, green: 0.83, blue: 0.60, alpha: 1), light: UIColor(red: 0.015, green: 0.41, blue: 0.255, alpha: 1))
    static let cyan = adaptive(dark: UIColor(red: 0.24, green: 0.84, blue: 0.96, alpha: 1), light: UIColor(red: 0.015, green: 0.405, blue: 0.545, alpha: 1))
    static let amber = adaptive(dark: UIColor(red: 0.98, green: 0.75, blue: 0.14, alpha: 1), light: UIColor(red: 0.65, green: 0.38, blue: 0.005, alpha: 1))
    static let danger = adaptive(dark: UIColor(red: 0.97, green: 0.44, blue: 0.44, alpha: 1), light: UIColor(red: 0.70, green: 0.12, blue: 0.16, alpha: 1))
    static let text = adaptive(dark: UIColor(red: 0.89, green: 0.92, blue: 0.95, alpha: 1), light: UIColor(red: 0.035, green: 0.105, blue: 0.135, alpha: 1))
    static let secondary = adaptive(dark: UIColor(red: 0.57, green: 0.65, blue: 0.73, alpha: 1), light: UIColor(red: 0.265, green: 0.35, blue: 0.39, alpha: 1))
    static let statusBackground = adaptive(dark: UIColor(white: 0, alpha: 0.38), light: UIColor(red: 0.82, green: 0.92, blue: 0.945, alpha: 0.95))

    private static func adaptive(dark: UIColor, light: UIColor) -> Color {
        Color(uiColor: UIColor { traits in traits.userInterfaceStyle == .dark ? dark : light })
    }

    static func clampedTextScale(_ value: Double) -> Double {
        guard value.isFinite else { return defaultTextScale }
        return min(maximumTextScale, max(minimumTextScale, value))
    }

    static func adjustedTextScale(_ value: Double, steps: Int) -> Double {
        let adjusted = clampedTextScale(value) + Double(steps) * textScaleStep
        return (clampedTextScale(adjusted) * 10).rounded() / 10
    }

    private static var currentTextScale: CGFloat {
        let stored = (UserDefaults.standard.object(forKey: textScaleStorageKey) as? NSNumber)?.doubleValue
        return CGFloat(clampedTextScale(stored ?? defaultTextScale))
    }

    static func size(_ base: CGFloat) -> CGFloat {
        let scaledBase = base * currentTextScale
#if targetEnvironment(macCatalyst)
        return scaledBase * 1.38
#else
        return UIFontMetrics(forTextStyle: .body).scaledValue(for: scaledBase)
#endif
    }
}

struct LabAppearanceButton: View {
    @Binding var selection: String
    private var appearance: LabAppearance { LabAppearance(rawValue: selection) ?? .dark }

    var body: some View {
        Button {
            selection = appearance == .dark ? LabAppearance.light.rawValue : LabAppearance.dark.rawValue
        } label: {
            Image(systemName: appearance == .dark ? "sun.max.fill" : "moon.stars.fill")
                .font(.system(size: Lab.size(14), weight: .bold))
                .frame(width: 44, height: 44)
                .background(Lab.statusBackground, in: Circle())
                .overlay(Circle().stroke(Lab.stroke))
        }
        .buttonStyle(.plain)
        .foregroundStyle(appearance == .dark ? Lab.amber : Lab.cyan)
        .accessibilityIdentifier("appearance-toggle")
        .accessibilityLabel(appearance == .dark ? "Switch to light mode" : "Switch to dark mode")
        .accessibilityValue(appearance == .dark ? "Dark mode" : "Light mode")
        .accessibilityHint("Remembers this choice for future launches")
    }
}

struct MermaidDocumentControls: View {
    @ObservedObject var session: MermaidDocumentSession
    let source: String
    let canUndo: Bool
    let canRedo: Bool
    let save: () -> Void
    let saveCopy: () -> Void
    let reopen: () -> Void
    let open: () -> Void
    let showSamples: () -> Void
    let openRecent: (MermaidRecentDocument) -> Void
    let undo: () -> Void
    let redo: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            documentStatus
            ViewThatFits(in: .horizontal) {
                controlRow(compact: false)
                controlRow(compact: true)
            }
        }
    }

    private var documentStatus: some View {
        let dirty = session.isDirty(source: source)
        let status = if session.isSaving {
            "SAVING"
        } else if session.attention == .changedOnDisk {
            "CHANGED ON DISK"
        } else if session.attention == .unavailable {
            "FILE UNAVAILABLE"
        } else if !session.hasCurrentDocument {
            dirty ? "UNSAVED" : "READY"
        } else {
            dirty ? "EDITED" : "SAVED"
        }
        let statusColor = if session.isSaving {
            Lab.cyan
        } else if session.attention != nil {
            Lab.danger
        } else if dirty {
            Lab.amber
        } else if session.hasCurrentDocument {
            Lab.emerald
        } else {
            Lab.cyan
        }
        return HStack(spacing: 8) {
            Image(systemName: session.hasCurrentDocument ? "doc.text.fill" : "doc.text")
                .foregroundStyle(dirty ? Lab.amber : Lab.cyan)
            Text(session.displayName)
                .font(.system(size: Lab.size(11), weight: .bold, design: .rounded))
                .foregroundStyle(Lab.text)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            HStack(spacing: 5) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 7, height: 7)
                Text(status)
            }
            .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
            .foregroundStyle(statusColor)
        }
        .padding(.horizontal, 11)
        .frame(minHeight: 36)
        .background(Lab.statusBackground.opacity(0.58), in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Lab.stroke.opacity(0.75)))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(session.displayName), \(status.lowercased())")
        .accessibilityIdentifier("source-document-status")
    }

    private func controlRow(compact: Bool) -> some View {
        HStack(spacing: 8) {
            sourceMenu

            Button(action: save) {
                if compact {
                    Image(systemName: "square.and.arrow.down")
                        .frame(minWidth: 24, minHeight: 28)
                } else {
                    Label("Save", systemImage: "square.and.arrow.down")
                }
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .tint(Lab.cyan)
            .disabled(saveDisabled)
            .accessibilityLabel(session.hasCurrentDocument ? "Save" : "Save new Mermaid file")
            .accessibilityHint(
                session.hasCurrentDocument
                    ? "Save changes back to \(session.displayName)"
                    : "Choose a Files location for this Mermaid source"
            )
            .accessibilityIdentifier("save-source-document")

            Spacer(minLength: compact ? 0 : 4)

            historyButton(compact: compact, isUndo: true)
            historyButton(compact: compact, isUndo: false)
        }
    }

    private func historyButton(compact: Bool, isUndo: Bool) -> some View {
        let title = isUndo ? "Undo" : "Redo"
        let symbol = isUndo ? "arrow.uturn.backward" : "arrow.uturn.forward"
        return Button(action: isUndo ? undo : redo) {
            if compact {
                Image(systemName: symbol)
                    .frame(minWidth: 24, minHeight: 28)
            } else {
                Label(title, systemImage: symbol)
            }
        }
        .buttonStyle(.bordered)
        .controlSize(.large)
        .tint(Lab.amber)
        .disabled(isUndo ? !canUndo : !canRedo)
        .frame(minHeight: 44)
        .keyboardShortcut("z", modifiers: isUndo ? .command : [.command, .shift])
        .accessibilityLabel("\(title) source change")
        .accessibilityIdentifier(isUndo ? "undo-source-change" : "redo-source-change")
        .accessibilityHint(
            isUndo
                ? "Restore the source before the most recent edit, import, sample, or source-lens change"
                : "Reapply the last undone Mermaid source change"
        )
    }

    private var sourceMenu: some View {
        Menu {
            Button(action: save) {
                Label("Save", systemImage: "square.and.arrow.down")
            }
            .disabled(saveDisabled)

            Button(action: saveCopy) {
                Label("Save a Copy…", systemImage: "doc.on.doc")
            }

            if session.hasCurrentDocument {
                Button(action: reopen) {
                    Label("Reopen from Disk", systemImage: "arrow.clockwise")
                }
            }

            Divider()

            Button(action: open) {
                Label("Open Mermaid File…", systemImage: "folder")
            }
            Button(action: showSamples) {
                Label("Sample Gallery", systemImage: "square.grid.2x2")
            }

            if !session.recentDocuments.isEmpty {
                Divider()
                Section("Recent Files") {
                    ForEach(session.recentDocuments) { recent in
                        Button {
                            openRecent(recent)
                        } label: {
                            Label(recent.displayName, systemImage: "clock.arrow.circlepath")
                        }
                    }
                }
            }
        } label: {
            Label("Source", systemImage: "doc.badge.gearshape")
        }
        .buttonStyle(.bordered)
        .controlSize(.large)
        .tint(Lab.cyan)
        .accessibilityHint("Open, save, copy, reopen, or choose a recent Mermaid source file")
    }

    private var saveDisabled: Bool {
        session.isSaving || (session.hasCurrentDocument && !session.isDirty(source: source))
    }
}

/// The FrankenSuite wordmark uses full-size initials and uppercase small caps
/// so FrankenMermaid reads immediately as two words instead of one text block.
struct FrankenWordmark: View {
    let productInitial: String
    let productRemainder: String
    let fullName: String
    var size: CGFloat = 21
    var accent: Color = Lab.cyan

    var body: some View {
        (
            Text("F")
                .font(.system(size: Lab.size(size), weight: .black, design: .monospaced))
                .foregroundColor(Lab.text.opacity(0.88))
            + Text("RANKEN")
                .font(.system(size: Lab.size(size * 0.66), weight: .black, design: .monospaced))
                .foregroundColor(Lab.text.opacity(0.88))
            + Text(productInitial)
                .font(.system(size: Lab.size(size), weight: .black, design: .monospaced))
                .foregroundColor(accent)
            + Text(productRemainder)
                .font(.system(size: Lab.size(size * 0.66), weight: .black, design: .monospaced))
                .foregroundColor(accent)
        )
        .kerning(0.8)
        .lineLimit(1)
        .minimumScaleFactor(0.72)
        .allowsTightening(true)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(fullName)
    }
}

struct LaboratoryBackground: View {
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        ZStack {
            Lab.background
            RadialGradient(
                colors: [Lab.cyan.opacity(reduceTransparency ? 0.05 : 0.16), .clear],
                center: .topLeading,
                startRadius: 0,
                endRadius: 760
            )
            Canvas { context, size in
                var filaments = Path()
                let step: CGFloat = 52
                stride(from: CGFloat.zero, through: size.width, by: step).forEach { x in
                    filaments.move(to: CGPoint(x: x, y: 0))
                    filaments.addLine(to: CGPoint(x: x, y: size.height))
                }
                stride(from: CGFloat.zero, through: size.height, by: step).forEach { y in
                    filaments.move(to: CGPoint(x: 0, y: y))
                    filaments.addLine(to: CGPoint(x: size.width, y: y))
                }
                context.stroke(filaments, with: .color(Lab.cyan.opacity(0.035)), lineWidth: 0.6)
            }
            .accessibilityHidden(true)
        }
        .ignoresSafeArea()
    }
}

struct LabPanel<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(16)
            .background(Lab.panel, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 18).stroke(Lab.stroke))
    }
}

struct LabLabel: View {
    let text: String
    var body: some View {
        Text(text.uppercased())
            .font(.system(size: Lab.size(10), weight: .black, design: .monospaced))
            .kerning(2.1)
            .foregroundStyle(Lab.cyan)
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: Lab.size(12), weight: .black, design: .monospaced))
            .textCase(.uppercase)
            .foregroundStyle(Color(red: 0.01, green: 0.07, blue: 0.08))
            .padding(.horizontal, 18)
            .padding(.vertical, 11)
            .background(
                LinearGradient(colors: [Lab.cyan, Lab.emerald],
                               startPoint: .topLeading, endPoint: .bottomTrailing),
                in: Capsule()
            )
            .opacity(isEnabled ? (configuration.isPressed ? 0.72 : 1) : 0.34)
    }
}
