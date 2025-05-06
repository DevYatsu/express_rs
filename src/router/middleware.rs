use crate::{handler::Handler, router::layer::LayerKind};
use std::collections::HashMap;

pub trait Middleware {
    fn target_path(&self) -> impl Into<String>;
    fn create_handler(&self) -> impl Into<Handler>;

    fn layer_kind() -> LayerKind {
        LayerKind::Middleware
    }
}

/// A parsed segment in a route path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Segment {
    Static(String),
    Param(String),
    ParamCatchAll(String),
    Mixed {
        prefix: String,
        param: String,
        suffix: String,
    },
    Wildcard,
}

/// A single node in the routing tree.
#[derive(Debug, Clone, Default)]
struct Node {
    children: HashMap<String, Box<Node>>,
    param: Option<(String, Box<Node>)>,
    catch_all: Option<(String, Box<Node>)>,
    wildcard: Option<Box<Node>>,
    value: Option<Vec<usize>>,
}

/// A radix-style router specifically for middleware.
#[derive(Debug, Clone, Default)]
pub struct MiddlewareRouter {
    root: Box<Node>,
}

/// Result of a successful path match.
#[derive(Debug, Clone)]
pub struct Match {
    pub value: Vec<usize>,
    pub params: HashMap<String, String>,
}

impl MiddlewareRouter {
    pub fn new() -> Self {
        Self {
            root: Box::new(Node::default()),
        }
    }

    pub fn insert(&mut self, path: &str, index: usize) -> Result<(), String> {
        let segments = parse_segments(path)?;
        let mut current = &mut self.root;

        for segment in segments {
            match segment {
                Segment::Static(s) => {
                    current = current.children.entry(s).or_default();
                }
                Segment::Param(p) => {
                    current = &mut current
                        .param
                        .get_or_insert_with(|| (p.clone(), Box::new(Node::default())))
                        .1;
                }
                Segment::ParamCatchAll(p) => {
                    current = &mut current
                        .catch_all
                        .get_or_insert_with(|| (p.clone(), Box::new(Node::default())))
                        .1;
                    break; // Catch-all consumes the rest
                }
                Segment::Wildcard => {
                    current = current
                        .wildcard
                        .get_or_insert_with(|| Box::new(Node::default()));
                    break;
                }
                Segment::Mixed {
                    prefix,
                    param,
                    suffix,
                } => {
                    return Err(format!(
                        "Mixed segments not yet supported: {}{{{}}}{}",
                        prefix, param, suffix
                    ));
                }
            }
        }

        current.value.get_or_insert(vec![]).push(index);
                        println!("{:?}", current.value);

        Ok(())
    }

    pub fn at(&self, path: &str) -> Option<Match> {
        let segments = split_path(path);
        let mut params = HashMap::new();
        let mut current = &self.root;

        for segment in &segments {
            if let Some(child) = current.children.get(*segment) {
                current = child;
                continue;
            }
            if let Some((param_name, param_node)) = &current.param {
                params.insert(param_name.clone(), segment.to_string());
                current = param_node;
                continue;
            }
            if let Some((param_name, catch_node)) = &current.catch_all {
                params.insert(param_name.clone(), segments.join("/"));
                current = catch_node;
                break;
            }
            if let Some(wild_node) = &current.wildcard {
                current = wild_node;
                break;
            }
            return None;
        }

        current.value.clone().map(|v| Match { value: v, params })
    }

    pub fn get_all_matches(&self, path: &str) -> Vec<Match> {
        let segments = split_path(path);
        let mut matches = Vec::new();
        let mut params = HashMap::new();
        Self::recurse(&self.root, &segments, 0, &mut params, &mut matches);
        matches
    }

    fn recurse(
        node: &Node,
        segments: &[&str],
        depth: usize,
        params: &mut HashMap<String, String>,
        matches: &mut Vec<Match>,
    ) {
        if depth == segments.len() {
            if let Some(value) = &node.value {
                matches.push(Match {
                    value: value.clone(),
                    params: params.clone(),
                });
            }
            return;
        }

        let segment = segments[depth];

        // Try static
        if let Some(child) = node.children.get(segment) {
            Self::recurse(child, segments, depth + 1, params, matches);
        }

        // Try param
        if let Some((name, param_node)) = &node.param {
            params.insert(name.clone(), segment.to_string());
            Self::recurse(param_node, segments, depth + 1, params, matches);
            params.remove(name);
        }

        // Try wildcard
        if let Some(wild_node) = &node.wildcard {
            if let Some(value) = &wild_node.value {
                matches.push(Match {
                    value: value.clone(),
                    params: params.clone(),
                });
            }

            // Allow wildcard to match the rest of the path
            Self::recurse(wild_node, segments, segments.len(), params, matches);
        }

        // Try catch_all
        if let Some((name, catch_node)) = &node.catch_all {
            let remaining = segments[depth..].join("/");
            params.insert(name.clone(), remaining);
            if let Some(value) = &catch_node.value {
                matches.push(Match {
                    value: value.clone(),
                    params: params.clone(),
                });
            }
            // Since catch_all absorbs the rest, we don't recurse further
            params.remove(name);
        }
    }
}

fn split_path(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_segments(path: &str) -> Result<Vec<Segment>, String> {
    let mut segments = vec![];
    let parts = split_path(path);

    for part in parts {
        if part == "*" {
            segments.push(Segment::Wildcard);
        } else if part.starts_with("{{") && part.ends_with("}}") {
            segments.push(Segment::Static(part[1..part.len() - 1].to_owned())); // unescape
        } else if part.contains('{') && part.contains('}') {
            let start = part.find('{').unwrap();
            let end = part.find('}').unwrap();
            let prefix = &part[..start];
            let param_section = &part[start + 1..end];
            let suffix = &part[end + 1..];

            if param_section.is_empty() {
                return Err(format!("Empty parameter in segment: {part}"));
            }

            if prefix.is_empty() && suffix.is_empty() {
                if param_section.starts_with('*') {
                    segments.push(Segment::ParamCatchAll(param_section[1..].to_owned()));
                } else {
                    segments.push(Segment::Param(param_section.to_owned()));
                }
            } else {
                segments.push(Segment::Mixed {
                    prefix: prefix.to_owned(),
                    param: param_section.to_owned(),
                    suffix: suffix.to_owned(),
                });
            }
        } else {
            segments.push(Segment::Static(part.to_owned()));
        }
    }

    Ok(segments)
}
