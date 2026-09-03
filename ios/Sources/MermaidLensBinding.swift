import Foundation

enum MermaidLensEditError: LocalizedError {
    case unavailable
    case staleSelection
    case invalidReceipt
    case emptyReplacement
    case replacementTooLarge

    var errorDescription: String? {
        switch self {
        case .unavailable:
            "Select a source-bound diagram element after the current render finishes."
        case .staleSelection:
            "The source or rendered element changed. Select the element again before editing."
        case .invalidReceipt:
            "The Rust source lens did not return a valid edit receipt."
        case .emptyReplacement:
            "Enter a non-empty source replacement."
        case .replacementTooLarge:
            "A source-lens replacement cannot exceed 4 KB."
        }
    }
}

struct MermaidLensBinding: Identifiable, Equatable, Sendable {
    let id: String
    let kind: String
    let sourceID: String?
    let snippet: String?
    let startByte: Int?
    let endByte: Int?
    let line: Int?
    let column: Int?

    init?(payload: [String: Any]) {
        guard let elementID = payload["elementId"] as? String, !elementID.isEmpty else { return nil }
        id = elementID
        kind = (payload["kind"] as? String ?? "element").lowercased()
        sourceID = (payload["sourceId"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        snippet = (payload["snippet"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let textRange = payload["textRange"] as? [String: Any]
        startByte = textRange?["startByte"] as? Int
        endByte = textRange?["endByte"] as? Int
        let start = (payload["span"] as? [String: Any])?["start"] as? [String: Any]
        line = (start?["line"] as? Int).flatMap { $0 > 0 ? $0 : nil }
        column = (start?["col"] as? Int).flatMap { $0 > 0 ? $0 : nil }
    }

    func exactSourceSnippet(in source: String) -> String? {
        guard let startByte, let endByte,
              startByte >= 0, endByte > startByte,
              endByte <= source.utf8.count else { return nil }
        let sourceBytes = Data(source.utf8)
        guard let exact = String(
            data: sourceBytes.subdata(in: startByte..<endByte),
            encoding: .utf8
        ), exact == snippet else { return nil }
        return exact
    }
}
