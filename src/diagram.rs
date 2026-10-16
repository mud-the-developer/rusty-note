//! Bounded Mermaid admission + upstream parser only. Never use upstream layout,
//! font metrics, SVG, or rendering. Unknown syntax must not be silently discarded.
use mermaid_rs_renderer::{DiagramKind, Direction, EdgeStyle, NodeShape, parse_mermaid_strict};
use regex::Regex;
use std::sync::OnceLock;

pub(crate) const MAX_SOURCE: usize = 8192;
const MAX_NODES: usize = 32;
const MAX_EDGES: usize = 48;
const MAX_LABEL: usize = 256;
const MAX_LABELS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Rounded,
    Diamond,
}
#[derive(Debug)]
pub(crate) struct Node {
    pub label: String,
    pub shape: Shape,
}
#[derive(Debug)]
pub(crate) struct Edge {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
}
#[derive(Debug)]
pub(crate) struct Diagram {
    pub horizontal: bool,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub ranks: Vec<Vec<usize>>,
}

/// Per-scene work/cache budget; failed attempts also consume parser admission.
/// Ordinary code fallback uses the existing Markdown cache, not a diagram cache.
#[derive(Default)]
pub(crate) struct Budget {
    attempts: usize,
    source: usize,
    nodes: usize,
    edges: usize,
    labels: usize,
}
impl Budget {
    pub fn prepare(&mut self, source: &str) -> Option<Diagram> {
        if source.len() > MAX_SOURCE || self.attempts >= 32 || self.source + source.len() > 65536 {
            return None;
        }
        self.attempts += 1;
        self.source += source.len();
        let diagram = parse(source)?;
        let labels = diagram.nodes.iter().map(|n| n.label.len()).sum::<usize>()
            + diagram
                .edges
                .iter()
                .filter_map(|e| e.label.as_ref())
                .map(String::len)
                .sum::<usize>();
        if self.nodes + diagram.nodes.len() > 128
            || self.edges + diagram.edges.len() > 192
            || self.labels + labels > 16384
        {
            return None;
        }
        self.nodes += diagram.nodes.len();
        self.edges += diagram.edges.len();
        self.labels += labels;
        Some(diagram)
    }
}

fn statement_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        // This is an anchored *admission allowlist*, not a replacement Mermaid
        // parser. No nesting, escaping, HTML, entities, directives or CSS. The
        // upstream parser is permissive, so checking only its IR loses syntax.
        let plain = r#"[^\[\](){}<>|;"'`\\&%#\r\n\t]+"#;
        let label = format!(r#"(?:{plain}|"{plain}")"#);
        let node =
            format!(r"[A-Za-z_][A-Za-z0-9_]{{0,31}}(?:\[{label}\]|\({label}\)|\{{{label}\}})?");
        // One labelled edge, or an unlabelled chain. Labelled chains are not
        // admitted: upstream 0.3.1 can partially consume them.
        Regex::new(&format!(
            r"\A(?:{node}(?:[ \t]*-->[ \t]*{node})*|{node}[ \t]*-->\|{plain}\|[ \t]*{node})\z"
        ))
        .unwrap()
    })
}

fn parse(source: &str) -> Option<Diagram> {
    if source.len() > MAX_SOURCE
        || source
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let mut statements = source
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut header = statements.next()?.split_ascii_whitespace();
    if !matches!(
        (header.next(), header.next(), header.next()),
        (Some("flowchart" | "graph"), Some("TD" | "TB" | "LR"), None)
    ) {
        return None;
    }
    let mut count = 0;
    for statement in statements {
        count += 1;
        if count > 96
            || !statement_pattern().is_match(statement)
            || statement.replace("-->", "").contains("--")
        {
            return None;
        }
        // Upstream treats these prefixes as instructions and can ignore them,
        // even when they look like a bare node. Reject rather than lose content.
        if [
            "end",
            "subgraph",
            "direction",
            "class",
            "style",
            "linkStyle",
            "click",
            "accTitle",
            "accDescr",
            "title",
            "flowchart",
            "graph",
        ]
        .iter()
        .any(|prefix| statement.starts_with(prefix))
        {
            return None;
        }
    }
    let arrows = source.matches("-->").count();
    if count == 0 || arrows > MAX_EDGES {
        return None;
    }
    let parsed = parse_mermaid_strict(source).ok()?;
    let graph = parsed.graph;
    if parsed.init_config.is_some()
        || graph.kind != DiagramKind::Flowchart
        || !matches!(graph.direction, Direction::TopDown | Direction::LeftRight)
        || graph.nodes.is_empty()
        || graph.nodes.len() > MAX_NODES
        || graph.edges.len() != arrows
        || !graph.subgraphs.is_empty()
        || !graph.node_links.is_empty()
        || !graph.class_defs.is_empty()
        || !graph.node_styles.is_empty()
        || !graph.node_classes.is_empty()
        || !graph.edge_styles.is_empty()
        || graph.edge_style_default.is_some()
    {
        return None;
    }
    let mut ordered: Vec<_> = graph.nodes.values().collect();
    ordered.sort_by_key(|n| graph.node_order.get(&n.id).copied());
    let mut nodes = Vec::new();
    let mut label_bytes = 0;
    for node in &ordered {
        let shape = match node.shape {
            NodeShape::Rectangle => Shape::Rectangle,
            NodeShape::RoundRect => Shape::Rounded,
            NodeShape::Diamond => Shape::Diamond,
            _ => return None,
        };
        if !valid_label(&node.label) || node.icon.is_some() || node.value.is_some() {
            return None;
        }
        label_bytes += node.label.len();
        nodes.push(Node {
            label: node.label.clone(),
            shape,
        });
    }
    let mut edges: Vec<Edge> = Vec::new();
    for edge in graph.edges {
        if !edge.directed
            || edge.arrow_start
            || !edge.arrow_end
            || edge.style != EdgeStyle::Solid
            || edge.arrow_start_kind.is_some()
            || edge.arrow_end_kind.is_some()
            || edge.start_decoration.is_some()
            || edge.end_decoration.is_some()
            || edge.start_label.is_some()
            || edge.end_label.is_some()
        {
            return None;
        }
        let from = ordered.iter().position(|n| n.id == edge.from)?;
        let to = ordered.iter().position(|n| n.id == edge.to)?;
        if from == to || edges.iter().any(|e| e.from == from && e.to == to) {
            return None;
        }
        if let Some(label) = &edge.label {
            if !valid_label(label) {
                return None;
            }
            label_bytes += label.len();
        }
        edges.push(Edge {
            from,
            to,
            label: edge.label,
        });
    }
    if label_bytes > MAX_LABELS {
        return None;
    }
    // Bounded topological ranks. Back edges/cycles and rank-skipping edges are
    // deliberately unsupported: straight native connectors must not cross nodes.
    let mut rank = vec![0; nodes.len()];
    let mut seen = vec![false; nodes.len()];
    for _ in 0..nodes.len() {
        let next = (0..nodes.len())
            .find(|&i| !seen[i] && edges.iter().filter(|e| e.to == i).all(|e| seen[e.from]))?;
        rank[next] = edges
            .iter()
            .filter(|e| e.to == next)
            .map(|e| rank[e.from] + 1)
            .max()
            .unwrap_or(0);
        seen[next] = true;
    }
    if edges.iter().any(|e| rank[e.to] != rank[e.from] + 1) {
        return None;
    }
    let mut ranks = vec![Vec::new(); rank.iter().max()? + 1];
    for (index, r) in rank.into_iter().enumerate() {
        ranks[r].push(index);
    }
    Some(Diagram {
        horizontal: graph.direction == Direction::LeftRight,
        nodes,
        edges,
        ranks,
    })
}
fn valid_label(label: &str) -> bool {
    !label.trim().is_empty() && label.len() <= MAX_LABEL && !label.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_parser_keeps_branching_unicode_and_directions() {
        for direction in ["TD", "TB", "LR"] {
            let source = format!(
                "graph {direction}; a[시작 α] --> b{{선택?}}; b -->|예| c(완료 ✅); b -->|No| d[Cancel]"
            );
            let d = parse(&source).unwrap();
            assert_eq!(d.nodes.len(), 4);
            assert_eq!(d.edges.len(), 3);
            assert_eq!(d.nodes[0].label, "시작 α");
            assert_eq!(d.nodes[1].shape, Shape::Diamond);
            assert_eq!(d.edges[1].label.as_deref(), Some("예"));
            assert_eq!(d.horizontal, direction == "LR");
        }
        assert_eq!(parse("flowchart LR; A-->B-->C").unwrap().edges.len(), 2);
        assert_eq!(
            parse("graph\tTD; A[\"한국어 Start\"]-->B").unwrap().nodes[0].label,
            "한국어 Start"
        );
        assert!(parse("graph TD; A-->B; A-->C; B-->D; C-->D").is_some());
    }
    #[test]
    fn unknown_unsafe_and_malformed_input_is_not_partially_rendered() {
        for source in [
            "sequenceDiagram\nA->>B: hello",
            "flowchart RL; A-->B",
            "flowchart sideways; A-->B",
            "flowchart TD ignored; A-->B",
            "graph TD; A-->B; nonsense ignored",
            "graph TD; A-->B; click A \"https://example.com\"",
            "graph TD; A-->B; style A fill:red",
            "graph TD; subgraph one; A-->B; end",
            "graph TD; A:::red-->B",
            "%%{init: {}}%%\ngraph TD; A-->B",
            "graph TD; A[<b>HTML</b>]-->B",
            "graph TD; A[&lt;]-->B",
            "graph TD; A[\"`markdown`\"]-->B",
            "graph TD; A[[subroutine]]-->B",
            "graph TD; A--xB",
            "graph TD; A-.->B",
            "graph TD; A<-->B",
            "graph TD; A & B-->C",
            "graph TD; A-->B-->A",
            "graph TD; A-->A",
            "graph TD; A-->B; A-->B",
            "graph TD; A-->B-->C; A-->C",
            "graph TD; A[unclosed-->B",
            "graph TD; A-->",
            "graph TD; A-->B; classDefIgnored",
            "graph TD; A[label--lost]",
            "graph TD; A-->B; %% comment",
            "graph TD; A-->B; A--> |label| C",
            "graph TD; A-->|yes|B-->C",
            "graph TD; A[one;two]",
            "graph TD; A\0-->B",
            "graph TD; A[\"unmatched]",
            "graph TD; A[]",
        ] {
            assert!(parse(source).is_none(), "{source}");
        }
    }
    #[test]
    fn resource_limits_include_failed_attempts_and_total_cache() {
        assert!(
            Budget::default()
                .prepare(&"a".repeat(MAX_SOURCE + 1))
                .is_none()
        );
        assert!(parse(&format!("graph TD; A[{}]", "가".repeat(86))).is_none());
        assert!(
            parse(&format!(
                "graph TD; {}",
                (0..33)
                    .map(|i| format!("N{i}"))
                    .collect::<Vec<_>>()
                    .join(";")
            ))
            .is_none()
        );
        assert!(parse(&format!("graph TD; {}", "A-->B;".repeat(49))).is_none());
        let mut b = Budget::default();
        for _ in 0..32 {
            assert!(b.prepare("invalid").is_none());
        }
        assert!(b.prepare("graph TD; A-->B").is_none());
        let source = format!(
            "graph TD; {}",
            (0..32)
                .map(|i| format!("N{i}"))
                .collect::<Vec<_>>()
                .join(";")
        );
        let mut b = Budget::default();
        for _ in 0..4 {
            assert!(b.prepare(&source).is_some());
        }
        assert!(b.prepare("graph TD; A").is_none());
        let labels = |size| {
            format!(
                "graph TD; {}",
                (0..32)
                    .map(|i| format!("N{i}[{}]", "x".repeat(size)))
                    .collect::<Vec<_>>()
                    .join(";")
            )
        };
        assert!(parse(&labels(128)).is_some());
        assert!(parse(&labels(129)).is_none());
        let source = format!(
            "graph TD; A[{}]-->|{}|B[{}]",
            "a".repeat(256),
            "b".repeat(256),
            "c".repeat(256)
        );
        let mut b = Budget::default();
        for _ in 0..21 {
            assert!(b.prepare(&source).is_some());
        }
        assert!(b.prepare(&source).is_none());
        let dense = format!(
            "graph TD; {}",
            (0..6)
                .flat_map(|a| (0..8).map(move |b| format!("A{a}-->B{b}")))
                .collect::<Vec<_>>()
                .join(";")
        );
        let mut b = Budget::default();
        for _ in 0..4 {
            assert!(b.prepare(&dense).is_some());
        }
        assert!(b.prepare(&dense).is_none());
        let mut b = Budget::default();
        for _ in 0..8 {
            assert!(b.prepare(&"x".repeat(8192)).is_none());
        }
        assert!(b.prepare("graph TD; A").is_none());
    }
}
