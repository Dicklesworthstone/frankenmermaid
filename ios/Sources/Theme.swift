import SwiftUI
import UIKit

enum Lab {
    static let background = Color(red: 0.002, green: 0.025, blue: 0.033)
    static let panel = Color.black.opacity(0.52)
    static let stroke = Color.white.opacity(0.08)
    static let emerald = Color(red: 0.20, green: 0.83, blue: 0.60)
    static let cyan = Color(red: 0.24, green: 0.84, blue: 0.96)
    static let amber = Color(red: 0.98, green: 0.75, blue: 0.14)
    static let danger = Color(red: 0.97, green: 0.44, blue: 0.44)
    static let text = Color(red: 0.89, green: 0.92, blue: 0.95)
    static let secondary = Color(red: 0.57, green: 0.65, blue: 0.73)

    static func size(_ base: CGFloat) -> CGFloat {
#if targetEnvironment(macCatalyst)
        base * 1.38
#else
        UIFontMetrics(forTextStyle: .body).scaledValue(for: base)
#endif
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

