import SwiftUI
import UIKit

enum LabAppearance: String {
    static let storageKey = "frankenmermaid.appearance"
    case dark
    case light
    var colorScheme: ColorScheme { self == .dark ? .dark : .light }
}

enum Lab {
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

    static func size(_ base: CGFloat) -> CGFloat {
#if targetEnvironment(macCatalyst)
        base * 1.38
#else
        UIFontMetrics(forTextStyle: .body).scaledValue(for: base)
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
