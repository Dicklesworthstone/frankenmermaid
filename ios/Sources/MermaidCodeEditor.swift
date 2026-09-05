import SwiftUI
import UIKit

/// Product-level Mermaid source history shared by direct typing, sample/file
/// replacement, and engine-owned source-lens edits. UIKit's responder-local
/// undo stack cannot see the latter operations, so the studio owns one bounded
/// history for every path that changes the canonical source string.
final class MermaidSourceHistory: ObservableObject {
    private struct ReplayTransition {
        let from: String
        let to: String
    }

    @Published private var undoSources: [String] = []
    @Published private var redoSources: [String] = []
    private var pendingReplay: ReplayTransition?
    private var lastContinuousEditAt: Date?

    private let maximumEntries = 100
    private let continuousEditInterval: TimeInterval = 0.8

    var canUndo: Bool { !undoSources.isEmpty }
    var canRedo: Bool { !redoSources.isEmpty }

    func recordChange(
        from previousSource: String,
        to source: String,
        continuous: Bool,
        now: Date = Date()
    ) {
        guard previousSource != source else { return }

        if let replay = pendingReplay {
            pendingReplay = nil
            if replay.from == previousSource, replay.to == source { return }
        }

        let continuesCurrentEdit = continuous
            && lastContinuousEditAt.map { now.timeIntervalSince($0) <= continuousEditInterval } == true
            && !undoSources.isEmpty
        if !continuesCurrentEdit {
            appendBounded(previousSource, to: &undoSources)
        }
        redoSources.removeAll(keepingCapacity: true)
        lastContinuousEditAt = continuous ? now : nil
    }

    func endContinuousEditing() {
        lastContinuousEditAt = nil
    }

    func undo(currentSource: String) -> String? {
        guard let previousSource = undoSources.popLast() else { return nil }
        appendBounded(currentSource, to: &redoSources)
        pendingReplay = ReplayTransition(from: currentSource, to: previousSource)
        lastContinuousEditAt = nil
        return previousSource
    }

    func redo(currentSource: String) -> String? {
        guard let nextSource = redoSources.popLast() else { return nil }
        appendBounded(currentSource, to: &undoSources)
        pendingReplay = ReplayTransition(from: currentSource, to: nextSource)
        lastContinuousEditAt = nil
        return nextSource
    }

    private func appendBounded(_ source: String, to stack: inout [String]) {
        if stack.last == source { return }
        if stack.count == maximumEntries { stack.removeFirst() }
        stack.append(source)
    }
}

/// A native editing surface with lexical Mermaid presentation. This does not
/// parse, validate, or mutate Mermaid structure—the Rust engine remains the
/// only parser. The highlighter merely colors stable lexical cues so typing is
/// immediate while engine diagnostics remain authoritative.
struct MermaidCodeEditor: UIViewRepresentable {
    @Binding var text: String
    @Binding var isFocused: Bool
    @Environment(\.colorScheme) private var colorScheme
    @AppStorage(Lab.textScaleStorageKey) private var uiTextScale = Lab.defaultTextScale

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> MermaidTextView {
        let view = MermaidTextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        view.keyboardDismissMode = .interactive
        view.autocapitalizationType = .none
        view.autocorrectionType = .no
        view.smartDashesType = .no
        view.smartQuotesType = .no
        view.spellCheckingType = .no
        view.alwaysBounceVertical = true
        view.alwaysBounceHorizontal = true
        view.showsVerticalScrollIndicator = true
        view.showsHorizontalScrollIndicator = true
        view.textContainer.widthTracksTextView = false
        view.textContainer.lineBreakMode = .byClipping
        view.textContainer.size = CGSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        view.tintColor = UIColor(Lab.cyan)
        view.selectedTextRange = view.textRange(from: view.beginningOfDocument, to: view.beginningOfDocument)
        view.accessibilityLabel = "Mermaid source editor"
        view.accessibilityHint = "Edit Mermaid syntax. Diagnostics from the Rust parser appear after rendering."
        context.coordinator.lastTextScale = Lab.clampedTextScale(uiTextScale)
        context.coordinator.applyHighlight(to: view, replacingText: text)
        return view
    }

    func updateUIView(_ view: MermaidTextView, context: Context) {
        context.coordinator.parent = self
        let clampedTextScale = Lab.clampedTextScale(uiTextScale)
        view.refreshTypographyLayout()
        if view.text != text
            || context.coordinator.lastColorScheme != colorScheme
            || context.coordinator.lastTextScale != clampedTextScale {
            context.coordinator.lastColorScheme = colorScheme
            context.coordinator.lastTextScale = clampedTextScale
            context.coordinator.applyHighlight(to: view, replacingText: text)
        } else {
            context.coordinator.refreshTypingAttributes(in: view)
        }
        if isFocused, !view.isFirstResponder {
            view.becomeFirstResponder()
        } else if !isFocused, view.isFirstResponder {
            view.resignFirstResponder()
        }
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MermaidCodeEditor
        var lastColorScheme: ColorScheme?
        var lastTextScale: Double?
        private var isApplyingHighlight = false

        init(_ parent: MermaidCodeEditor) { self.parent = parent }

        func textViewDidBeginEditing(_ textView: UITextView) {
            parent.isFocused = true
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            parent.isFocused = false
        }

        func textViewDidChange(_ textView: UITextView) {
            guard !isApplyingHighlight, let view = textView as? MermaidTextView else { return }
            parent.text = view.text
            applyHighlight(to: view, replacingText: nil)
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            textView.setNeedsDisplay()
        }

        func scrollViewDidScroll(_ scrollView: UIScrollView) {
            scrollView.setNeedsDisplay()
        }

        func applyHighlight(to view: MermaidTextView, replacingText replacement: String?) {
            isApplyingHighlight = true
            let selectedRange = view.selectedRange
            let contentOffset = view.contentOffset
            if let replacement { view.text = replacement }
            let source = view.text ?? ""
            let fullRange = NSRange(location: 0, length: (source as NSString).length)
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 4
            paragraph.paragraphSpacing = 1
            let baseFont = UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: .regular)
            let storage = NSMutableAttributedString(
                string: source,
                attributes: [
                    .font: baseFont,
                    .foregroundColor: UIColor(Lab.text),
                    .paragraphStyle: paragraph
                ]
            )

            apply(Self.keywordRegex, color: UIColor(Lab.cyan), fontWeight: .bold,
                  to: storage, source: source, range: fullRange)
            apply(Self.statementKeywordRegex, color: UIColor(Lab.cyan).withAlphaComponent(0.9),
                  fontWeight: .semibold, to: storage, source: source, range: fullRange)
            apply(Self.arrowRegex, color: UIColor(Lab.emerald), fontWeight: .semibold,
                  to: storage, source: source, range: fullRange)
            apply(Self.nodeLabelRegex, color: UIColor(Lab.cyan),
                  to: storage, source: source, range: fullRange)
            apply(Self.edgeLabelRegex, color: UIColor(Lab.amber), fontWeight: .semibold,
                  to: storage, source: source, range: fullRange)
            apply(Self.stringRegex, color: UIColor(Lab.emerald),
                  to: storage, source: source, range: fullRange)
            apply(Self.commentRegex, color: UIColor(Lab.secondary).withAlphaComponent(0.72),
                  italic: true, to: storage, source: source, range: fullRange)
            apply(Self.directiveRegex, color: UIColor(Lab.amber), fontWeight: .bold,
                  to: storage, source: source, range: fullRange)
            apply(Self.numberRegex, color: UIColor(Lab.amber),
                  to: storage, source: source, range: fullRange)

            view.attributedText = storage
            let selectionLocation = min(selectedRange.location, fullRange.length)
            let selectionLength = min(selectedRange.length, fullRange.length - selectionLocation)
            view.selectedRange = NSRange(location: selectionLocation, length: selectionLength)
            refreshTypingAttributes(in: view)
            view.setContentOffset(contentOffset, animated: false)
            view.setNeedsDisplay()
            isApplyingHighlight = false
        }

        func refreshTypingAttributes(in view: UITextView) {
            view.typingAttributes = [
                .font: UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: .regular),
                .foregroundColor: UIColor(Lab.text)
            ]
        }

        private func apply(
            _ regex: NSRegularExpression?,
            color: UIColor,
            fontWeight: UIFont.Weight? = nil,
            italic: Bool = false,
            to storage: NSMutableAttributedString,
            source: String,
            range: NSRange
        ) {
            guard let regex else { return }
            regex.enumerateMatches(in: source, options: [], range: range) { match, _, _ in
                guard let match else { return }
                storage.addAttribute(.foregroundColor, value: color, range: match.range)
                if let fontWeight {
                    let font = UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: fontWeight)
                    storage.addAttribute(.font, value: font, range: match.range)
                } else if italic {
                    let size = Lab.size(15)
                    let baseFont = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
                    let descriptor = baseFont.fontDescriptor.withSymbolicTraits(.traitItalic)
                        ?? baseFont.fontDescriptor
                    let font = UIFont(descriptor: descriptor, size: size)
                    storage.addAttribute(.font, value: font, range: match.range)
                }
            }
        }

        private static func regex(_ pattern: String, options: NSRegularExpression.Options = []) -> NSRegularExpression? {
            try? NSRegularExpression(pattern: pattern, options: options)
        }

        private static let keywordRegex = regex(
            #"(?m)^\s*(?:flowchart|graph|sequenceDiagram|classDiagram|stateDiagram(?:-v2)?|erDiagram|journey|gantt|pie|quadrantChart|requirementDiagram|gitGraph|mindmap|timeline|sankey-beta|xychart-beta|block-beta|packet-beta|kanban|architecture-beta|C4(?:Context|Container|Component|Dynamic|Deployment))\b"#
        )
        private static let statementKeywordRegex = regex(
            #"\b(?:title|participant|actor|section|class|state|direction|dateFormat|axisFormat|excludes|todayMarker|autonumber|loop|alt|else|opt|par|and|critical|break|rect|note|activate|deactivate|branch|checkout|commit|merge|cherry-pick|column|requirement|element|Person|System|System_Ext|Container|ContainerDb|Container_Boundary|Component|Deployment_Node|Rel|group|service|columns|block|end|root|bar|line)\b"#,
            options: [.caseInsensitive]
        )
        private static let arrowRegex = regex(#"(?:<-->|-->|---|-.->|==>|~~~|--o|--x|<--|--|\.->|\.=)"#)
        private static let nodeLabelRegex = regex(#"(?<=\[)[^\]\n]+(?=\])|(?<=\{)[^}\n]+(?=\})"#)
        private static let edgeLabelRegex = regex(#"(?<=\|)[^|\n]+(?=\|)"#)
        private static let stringRegex = regex(#"\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'"#)
        private static let commentRegex = regex(#"(?m)%%(?!\{).*$"#)
        private static let directiveRegex = regex(#"(?m)%%\{.*?\}%%"#)
        private static let numberRegex = regex(#"(?<![A-Za-z_])\d+(?:\.\d+)?(?:d|w|h|m|s)?\b"#)
    }
}

final class MermaidTextView: UITextView {
    private var gutterWidth: CGFloat { max(42, Lab.size(31)) }
    private var editorInsets: UIEdgeInsets {
        UIEdgeInsets(top: 14, left: gutterWidth + 12, bottom: 18, right: 14)
    }

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        textContainerInset = editorInsets
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    func refreshTypographyLayout() {
        if textContainerInset != editorInsets {
            textContainerInset = editorInsets
            setNeedsLayout()
            setNeedsDisplay()
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        refreshTypographyLayout()
    }

    override func draw(_ rect: CGRect) {
        drawCurrentLine()
        super.draw(rect)
        drawGutter()
    }

    private func drawCurrentLine() {
        guard !text.isEmpty else { return }
        let location = min(selectedRange.location, (text as NSString).length - 1)
        let glyph = layoutManager.glyphIndexForCharacter(at: max(0, location))
        let fragment = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let y = fragment.minY + textContainerInset.top - contentOffset.y
        let row = CGRect(x: gutterWidth, y: y, width: bounds.width - gutterWidth, height: fragment.height)
        UIColor(Lab.cyan).withAlphaComponent(0.055).setFill()
        UIBezierPath(rect: row).fill()
    }

    private func drawGutter() {
        let context = UIGraphicsGetCurrentContext()
        context?.saveGState()
        UIColor(Lab.cyan).withAlphaComponent(0.16).setStroke()
        let divider = UIBezierPath()
        divider.move(to: CGPoint(x: gutterWidth, y: 0))
        divider.addLine(to: CGPoint(x: gutterWidth, y: bounds.height))
        divider.lineWidth = 0.7
        divider.stroke()

        let nsText = text as NSString
        var lineStarts = [0]
        if nsText.length > 0 {
            for index in 0 ..< nsText.length where nsText.character(at: index) == 10 {
                if index + 1 < nsText.length { lineStarts.append(index + 1) }
            }
        }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont.monospacedDigitSystemFont(ofSize: Lab.size(10), weight: .medium),
            .foregroundColor: UIColor(Lab.secondary).withAlphaComponent(0.58)
        ]
        if nsText.length == 0 {
            let label = "1" as NSString
            let size = label.size(withAttributes: attributes)
            label.draw(
                at: CGPoint(
                    x: gutterWidth - size.width - 8,
                    y: textContainerInset.top - contentOffset.y + 2
                ),
                withAttributes: attributes
            )
            context?.restoreGState()
            return
        }
        for (line, start) in lineStarts.enumerated() {
            let glyph = layoutManager.glyphIndexForCharacter(at: min(start, max(0, nsText.length - 1)))
            let fragment = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            let y = fragment.minY + textContainerInset.top - contentOffset.y + 2
            guard y > -20, y < bounds.height + 20 else { continue }
            let label = "\(line + 1)" as NSString
            let size = label.size(withAttributes: attributes)
            label.draw(at: CGPoint(x: gutterWidth - size.width - 8, y: y), withAttributes: attributes)
        }
        context?.restoreGState()
    }
}
