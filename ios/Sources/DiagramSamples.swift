import SwiftUI

struct DiagramSample: Identifiable, Hashable {
    let id: String
    let name: String
    let category: String
    let summary: String
    let symbol: String
    let source: String
}

extension DiagramSample {
    /// One verified starter for every diagram family currently implemented by
    /// FrankenMermaid. The source comes from the engine's own conformance
    /// corpus, so the gallery cannot quietly promise syntax the bundled Rust
    /// renderer does not understand.
    static let all: [DiagramSample] = [
        .init(id: "flowchart", name: "Decision Flow", category: "Graphs", summary: "Branches, labeled edges, and a shared destination.", symbol: "point.3.connected.trianglepath.dotted", source: """
        flowchart TD
            A[Start] --> B{Decision}
            B -->|Yes| C[Action 1]
            B -->|No| D[Action 2]
            C --> E[End]
            D --> E
        """),
        .init(id: "sequence", name: "Conversation", category: "Software", summary: "A request and response between two participants.", symbol: "arrow.left.arrow.right", source: """
        sequenceDiagram
            participant Alice
            participant Bob
            Alice->>Bob: Hello Bob
            Bob-->>Alice: Hi Alice
            Alice->>Bob: How are you?
            Bob-->>Alice: Good, thanks!
        """),
        .init(id: "class", name: "Type Hierarchy", category: "Software", summary: "Inheritance, fields, and methods.", symbol: "square.stack.3d.up", source: """
        classDiagram
            Animal <|-- Duck
            Animal <|-- Fish
            Animal : +int age
            Animal : +String gender
            Animal : +isMammal() bool
            Duck : +String beakColor
            Duck : +swim()
            Fish : +int sizeInFeet
            Fish : +canEat()
        """),
        .init(id: "state", name: "State Machine", category: "Software", summary: "Start, terminal, and labeled state transitions.", symbol: "circle.hexagongrid", source: """
        stateDiagram-v2
            [*] --> Still
            Still --> [*]
            Still --> Moving
            Moving --> Still
            Moving --> Crash
            Crash --> [*]
        """),
        .init(id: "er", name: "Data Relationships", category: "Data", summary: "Entities, fields, keys, and cardinality.", symbol: "cylinder.split.1x2", source: """
        erDiagram
            CUSTOMER ||--o{ ORDER : places
            ORDER ||--|{ LINE-ITEM : contains
            PRODUCT ||--o{ LINE-ITEM : references
            CUSTOMER {
                string name
                string id PK
            }
            ORDER {
                int orderNumber PK
                date created
            }
        """),
        .init(id: "c4-context", name: "C4 Context", category: "Architecture", summary: "People, systems, and external dependencies.", symbol: "person.2.crop.square.stack", source: """
        C4Context
            title System Context diagram
            Person(user, "User", "A person using the system")
            System(system, "Main System", "The main application")
            System_Ext(email, "Email System", "Sends emails")
            Rel(user, system, "Uses")
            Rel(system, email, "Sends emails via")
        """),
        .init(id: "c4-container", name: "C4 Containers", category: "Architecture", summary: "Frontend, API, database, and system boundary.", symbol: "shippingbox.and.arrow.backward", source: """
        C4Container
            title Container diagram
            Person(user, "User", "A person using the system")
            Container_Boundary(c1, "Main System") {
                Container(web, "Web App", "React", "The frontend")
                Container(api, "API", "Rust", "The backend")
                ContainerDb(db, "Database", "PostgreSQL", "Stores data")
            }
            Rel(user, web, "Uses")
            Rel(web, api, "Makes API calls")
            Rel(api, db, "Reads/writes")
        """),
        .init(id: "c4-component", name: "C4 Components", category: "Architecture", summary: "Controller, service, and repository internals.", symbol: "square.grid.3x3.square", source: """
        C4Component
            title Component diagram
            Container_Boundary(api, "API") {
                Component(ctrl, "Controller", "Rust", "Handles requests")
                Component(svc, "Service", "Rust", "Business logic")
                Component(repo, "Repository", "Rust", "Data access")
            }
            Rel(ctrl, svc, "Calls")
            Rel(svc, repo, "Uses")
        """),
        .init(id: "c4-dynamic", name: "C4 Dynamic", category: "Architecture", summary: "Numbered interactions across a running system.", symbol: "bolt.horizontal.circle", source: """
        C4Dynamic
            title Dynamic diagram
            Person(user, "User", "A person")
            Container(web, "Web App", "React", "Frontend")
            Container(api, "API", "Rust", "Backend")
            Rel(user, web, "1. Opens app")
            Rel(web, api, "2. Fetches data")
            Rel(api, web, "3. Returns data")
        """),
        .init(id: "c4-deployment", name: "C4 Deployment", category: "Architecture", summary: "Nested cloud infrastructure and deployed services.", symbol: "cloud", source: """
        C4Deployment
            title Deployment diagram
            Deployment_Node(aws, "AWS Cloud", "Cloud Provider") {
                Deployment_Node(ec2, "EC2 Instance", "t3.medium") {
                    Container(api, "API Server", "Rust", "Backend service")
                }
                Deployment_Node(rds, "RDS", "db.t3.micro") {
                    ContainerDb(db, "Database", "PostgreSQL", "Data store")
                }
            }
            Rel(api, db, "Connects to")
        """),
        .init(id: "architecture", name: "Service Architecture", category: "Architecture", summary: "Groups, services, icons, and directional ports.", symbol: "building.2.crop.circle", source: """
        architecture-beta
            group api(cloud)[API Layer]
            service gateway(server)[Gateway] in api
            service auth(lock)[Auth] in api
            gateway:R --> L:auth
        """),
        .init(id: "block", name: "Block Layout", category: "Graphs", summary: "Nested blocks with explicit column spans.", symbol: "rectangle.3.group", source: """
        block-beta
          columns 4
          a["A"]:2 b["B"]:2
          block:inner1:2
            columns 2
            c["C"] d["D"]
          end
        """),
        .init(id: "gantt", name: "Project Schedule", category: "Planning", summary: "Sections, dates, durations, and dependencies.", symbol: "calendar.badge.clock", source: """
        gantt
            title Project Schedule
            dateFormat YYYY-MM-DD
            section Planning
                Research       :a1, 2024-01-01, 7d
                Design         :a2, after a1, 5d
            section Development
                Implementation :b1, after a2, 14d
                Testing        :b2, after b1, 7d
        """),
        .init(id: "timeline", name: "Event Timeline", category: "Planning", summary: "A chronological narrative of notable events.", symbol: "timeline.selection", source: """
        timeline
            title History of major events
            2020 : COVID-19 pandemic begins
            2021 : Vaccines roll out
            2022 : Pandemic restrictions ease
        """),
        .init(id: "journey", name: "User Journey", category: "Planning", summary: "Activities grouped into scored sections.", symbol: "figure.walk.motion", source: """
        journey
            title My working day
            section Go to work
                Make tea: 5: Me
                Go upstairs: 3: Me
            section At work
                Sit down: 5: Me
        """),
        .init(id: "gitgraph", name: "Git History", category: "Software", summary: "Commits, branches, checkout, and merge.", symbol: "arrow.triangle.branch", source: """
        gitGraph
            commit
            commit
            branch develop
            commit
            commit
            checkout main
            merge develop
            commit
        """),
        .init(id: "sankey", name: "Energy Flow", category: "Data", summary: "Weighted flows between named stages.", symbol: "water.waves", source: """
        sankey-beta

        Agriculture,Bio-energy,124.729
        "Bio-energy",Electricity,35.793
        """),
        .init(id: "mindmap", name: "Mind Map", category: "Ideas", summary: "An indented radial hierarchy.", symbol: "brain.head.profile", source: """
        mindmap
            root((Main Topic))
                Branch1
                    Leaf1
                    Leaf2
                Branch2
                    Leaf3
        """),
        .init(id: "pie", name: "Pie Chart", category: "Data", summary: "A compact categorical breakdown.", symbol: "chart.pie", source: """
        pie title Pets adopted
            "Dogs" : 386
            "Cats" : 85
            "Rats" : 15
        """),
        .init(id: "quadrant", name: "Priority Matrix", category: "Data", summary: "Two axes, named quadrants, and positioned items.", symbol: "square.grid.2x2", source: """
        quadrantChart
            title Priority Matrix
            x-axis Low Effort --> High Effort
            y-axis Low Impact --> High Impact
            quadrant-1 Quick wins
            quadrant-2 Major projects
            quadrant-3 Fill-ins
            quadrant-4 Thankless tasks
            Item A: [0.2, 0.8]
            Item B: [0.7, 0.9]
            Item C: [0.3, 0.3]
        """),
        .init(id: "xychart", name: "Series Chart", category: "Data", summary: "Axes with bar and line series.", symbol: "chart.xyaxis.line", source: """
        xychart-beta
          title Sales
          x-axis [jan, feb, mar]
          y-axis "Revenue" 0 --> 100
          bar Revenue [10, 20, 30]
          line Target [15, 25, 35]
        """),
        .init(id: "requirement", name: "Requirements", category: "Software", summary: "Requirements, elements, risks, and verification.", symbol: "checklist.checked", source: """
        requirementDiagram
            requirement req1 {
                id: 1
                text: System shall process requests
                risk: high
                verifymethod: test
            }
            element elem1 {
                type: module
                docref: doc1
            }
            elem1 - satisfies -> req1
        """),
        .init(id: "packet", name: "Packet Map", category: "Data", summary: "Bit ranges and named protocol fields.", symbol: "shippingbox", source: """
        packet-beta
            0-7: "Header"
            8-15: "Payload"
            16-31: "Checksum"
        """),
        .init(id: "kanban", name: "Kanban Board", category: "Planning", summary: "Columns and tasks in their current stage.", symbol: "rectangle.split.3x1", source: """
        kanban
            column todo[To Do]
            column doing[In Progress]
            column done[Done]
            task1[Task 1] in todo
            task2[Task 2] in doing
            task3[Task 3] in done
        """)
    ]
}

struct DiagramSampleGallery: View {
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    let select: (DiagramSample) -> Void

    private var filteredSamples: [DiagramSample] {
        guard !query.isEmpty else { return DiagramSample.all }
        return DiagramSample.all.filter {
            $0.name.localizedCaseInsensitiveContains(query)
                || $0.category.localizedCaseInsensitiveContains(query)
                || $0.summary.localizedCaseInsensitiveContains(query)
                || $0.source.localizedCaseInsensitiveContains(query)
        }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                LaboratoryBackground()
                ScrollView {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 240), spacing: 12)],
                        spacing: 12
                    ) {
                        ForEach(filteredSamples) { sample in
                            Button {
                                select(sample)
                                dismiss()
                            } label: {
                                VStack(alignment: .leading, spacing: 10) {
                                    HStack(spacing: 9) {
                                        Image(systemName: sample.symbol)
                                            .font(.title3.weight(.bold))
                                            .foregroundStyle(Lab.cyan)
                                            .frame(width: 28)
                                        VStack(alignment: .leading, spacing: 1) {
                                            Text(sample.name)
                                                .font(.headline.weight(.bold))
                                                .foregroundStyle(Lab.text)
                                            Text(sample.category.uppercased())
                                                .font(.caption2.weight(.black).monospaced())
                                                .tracking(1.2)
                                                .foregroundStyle(Lab.secondary)
                                        }
                                    }
                                    Text(sample.summary)
                                        .font(.subheadline)
                                        .foregroundStyle(Lab.secondary)
                                        .multilineTextAlignment(.leading)
                                    Text(sample.source.split(separator: "\n").first.map(String.init) ?? sample.id)
                                        .font(.caption.weight(.bold).monospaced())
                                        .foregroundStyle(Lab.emerald)
                                        .lineLimit(1)
                                }
                                .frame(maxWidth: .infinity, minHeight: 118, alignment: .topLeading)
                                .padding(15)
                                .background(Lab.panel, in: RoundedRectangle(cornerRadius: 16))
                                .overlay {
                                    RoundedRectangle(cornerRadius: 16)
                                        .stroke(Lab.cyan.opacity(0.18))
                                }
                            }
                            .buttonStyle(.plain)
                            .accessibilityHint("Loads this verified sample into the source editor")
                        }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Diagram Specimens")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $query, prompt: "Search 24 diagram families")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .accessibilityIdentifier("sample-gallery")
    }
}
