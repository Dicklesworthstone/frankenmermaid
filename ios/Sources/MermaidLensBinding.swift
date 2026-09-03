import Foundation

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
}
