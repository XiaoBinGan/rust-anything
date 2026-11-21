//! Small helpers over tree-sitter syntax trees, shared by the language plugins.

use tree_sitter::{Language, Node, Parser, Tree};

/// Parses `text`; `None` only when the grammar cannot be loaded.
pub fn parse(language: &Language, text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    parser.parse(text, None)
}

pub fn text<'t>(node: Node, source: &'t str) -> &'t str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// 1-based line of a node.
pub fn line(node: Node) -> u32 {
    u32::try_from(node.start_position().row + 1).unwrap_or(u32::MAX)
}

/// Every node in the tree, depth first.
pub fn walk<'t>(root: Node<'t>, mut visit: impl FnMut(Node<'t>)) {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        visit(node);
        let children: Vec<Node> = node.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
}

pub fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Non-blank lines.
pub fn loc(text: &str) -> usize {
    text.lines().filter(|l| !l.trim().is_empty()).count()
}

/// `text` with its `<!-- -->` comments removed (a `pom.xml`, a project file).
pub fn without_xml_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}
