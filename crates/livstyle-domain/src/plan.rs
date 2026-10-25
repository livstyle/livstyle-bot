use std::collections::{BTreeMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Analysis,
    CodeChange,
    ConfigChange,
    Build,
    Flash,
    Verify,
    Report,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Pending,
    Running,
    Done,
    Invalidated,
    Skipped,
}

/// How a node may be interrupted (PRD §8.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interruptibility {
    Free,
    SafePoint,
    NonInterruptible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub depends_on: Vec<NodeId>,
    /// Input key -> digest. Keys are shared across nodes, e.g. `source_revision`,
    /// `sdkconfig`, `chip_profile`, `requirement.boot_assertions`.
    pub inputs: BTreeMap<String, String>,
    pub status: NodeStatus,
    pub interruptibility: Interruptibility,
}

impl PlanNode {
    pub fn new(id: &str, kind: NodeKind, title: &str) -> Self {
        Self {
            id: NodeId::new(id),
            kind,
            title: title.to_owned(),
            depends_on: Vec::new(),
            inputs: BTreeMap::new(),
            status: NodeStatus::Pending,
            interruptibility: Interruptibility::Free,
        }
    }

    pub fn after(mut self, deps: &[&str]) -> Self {
        self.depends_on.extend(deps.iter().map(|d| NodeId::new(*d)));
        self
    }

    pub fn input(mut self, key: &str, digest: &str) -> Self {
        self.inputs.insert(key.to_owned(), digest.to_owned());
        self
    }

    pub fn interruptibility(mut self, value: Interruptibility) -> Self {
        self.interruptibility = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("duplicate node id {0}")]
    DuplicateNode(NodeId),
    #[error("node {node} depends on {dependency}, which is unknown or not declared earlier")]
    BadDependency { node: NodeId, dependency: NodeId },
    #[error("unknown node {0}")]
    UnknownNode(NodeId),
}

/// Result of applying input changes to a plan (shown to the user as the plan diff).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDiff {
    pub plan_version: u64,
    /// Completed nodes whose results are still valid.
    pub kept: Vec<NodeId>,
    /// Completed nodes whose results are void and must be redone.
    pub invalidated: Vec<NodeId>,
    /// Running nodes affected by the change; the caller must interrupt them
    /// according to their interruptibility.
    pub interrupted: Vec<NodeId>,
    /// Pending nodes that picked up the new inputs.
    pub updated_pending: Vec<NodeId>,
}

impl PlanDiff {
    pub fn is_noop(&self) -> bool {
        self.invalidated.is_empty()
            && self.interrupted.is_empty()
            && self.updated_pending.is_empty()
    }
}

/// Product-level plan graph. Nodes are stored in a topological order: every
/// dependency must be declared before its dependents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub version: u64,
    nodes: Vec<PlanNode>,
}

impl Plan {
    pub fn new(nodes: Vec<PlanNode>) -> Result<Self, PlanError> {
        let mut seen = HashSet::new();
        for node in &nodes {
            for dep in &node.depends_on {
                if !seen.contains(dep) {
                    return Err(PlanError::BadDependency {
                        node: node.id.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
            if !seen.insert(node.id.clone()) {
                return Err(PlanError::DuplicateNode(node.id.clone()));
            }
        }
        Ok(Self { version: 1, nodes })
    }

    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    pub fn node(&self, id: &NodeId) -> Option<&PlanNode> {
        self.nodes.iter().find(|n| &n.id == id)
    }

    pub fn set_status(&mut self, id: &NodeId, status: NodeStatus) -> Result<(), PlanError> {
        let node = self
            .nodes
            .iter_mut()
            .find(|n| &n.id == id)
            .ok_or_else(|| PlanError::UnknownNode(id.clone()))?;
        node.status = status;
        Ok(())
    }

    /// Nodes ready to run: pending or invalidated, with every dependency done or skipped.
    pub fn runnable(&self) -> Vec<&PlanNode> {
        self.nodes
            .iter()
            .filter(|n| matches!(n.status, NodeStatus::Pending | NodeStatus::Invalidated))
            .filter(|n| {
                n.depends_on.iter().all(|d| {
                    self.node(d).is_some_and(|dep| {
                        matches!(dep.status, NodeStatus::Done | NodeStatus::Skipped)
                    })
                })
            })
            .collect()
    }

    /// Applies new input digests and invalidates affected nodes and everything
    /// downstream of them. Unaffected completed nodes keep their results.
    pub fn apply_input_changes(&mut self, changes: &BTreeMap<String, String>) -> PlanDiff {
        let mut affected: HashSet<NodeId> = HashSet::new();
        let mut diff = PlanDiff::default();

        for node in &mut self.nodes {
            let mut hit = node.depends_on.iter().any(|d| affected.contains(d));
            for (key, digest) in changes {
                if let Some(current) = node.inputs.get_mut(key) {
                    if current != digest {
                        *current = digest.clone();
                        hit = true;
                    }
                }
            }

            if !hit {
                if node.status == NodeStatus::Done {
                    diff.kept.push(node.id.clone());
                }
                continue;
            }

            affected.insert(node.id.clone());
            match node.status {
                NodeStatus::Done => {
                    node.status = NodeStatus::Invalidated;
                    diff.invalidated.push(node.id.clone());
                }
                NodeStatus::Running => {
                    node.status = NodeStatus::Invalidated;
                    diff.interrupted.push(node.id.clone());
                }
                NodeStatus::Pending | NodeStatus::Invalidated => {
                    diff.updated_pending.push(node.id.clone());
                }
                NodeStatus::Skipped => {}
            }
        }

        if !diff.is_noop() {
            self.version += 1;
        }
        diff.plan_version = self.version;
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[NodeId]) -> Vec<&str> {
        list.iter().map(|n| n.0.as_str()).collect()
    }

    fn esp32_plan() -> Plan {
        Plan::new(vec![
            PlanNode::new("code", NodeKind::CodeChange, "Fix boot issue")
                .input("requirement.feature", "r1"),
            PlanNode::new("build", NodeKind::Build, "idf.py build")
                .after(&["code"])
                .input("sdkconfig", "s1")
                .input("chip_profile", "esp32s3@1"),
            PlanNode::new("flash", NodeKind::Flash, "Flash app")
                .after(&["build"])
                .input("board_profile", "devkitc@1")
                .interruptibility(Interruptibility::NonInterruptible),
            PlanNode::new("verify", NodeKind::Verify, "Boot log assertions")
                .after(&["flash"])
                .input("requirement.boot_assertions", "a1"),
            PlanNode::new("report", NodeKind::Report, "Evidence").after(&["verify"]),
        ])
        .unwrap()
    }

    fn finish(plan: &mut Plan, nodes: &[&str]) {
        for n in nodes {
            plan.set_status(&NodeId::new(*n), NodeStatus::Done).unwrap();
        }
    }

    fn change(key: &str, value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(key.to_owned(), value.to_owned())])
    }

    #[test]
    fn assertion_change_does_not_rebuild_or_reflash() {
        let mut plan = esp32_plan();
        finish(&mut plan, &["code", "build", "flash", "verify", "report"]);

        let diff = plan.apply_input_changes(&change("requirement.boot_assertions", "a2"));

        assert_eq!(ids(&diff.kept), ["code", "build", "flash"]);
        assert_eq!(ids(&diff.invalidated), ["verify", "report"]);
        assert_eq!(plan.version, 2);
        assert_eq!(
            plan.runnable()
                .iter()
                .map(|n| n.id.0.as_str())
                .collect::<Vec<_>>(),
            ["verify"]
        );
    }

    #[test]
    fn sdkconfig_change_invalidates_build_and_downstream() {
        let mut plan = esp32_plan();
        finish(&mut plan, &["code", "build"]);
        plan.set_status(&NodeId::new("flash"), NodeStatus::Running)
            .unwrap();

        let diff = plan.apply_input_changes(&change("sdkconfig", "s2"));

        assert_eq!(ids(&diff.kept), ["code"]);
        assert_eq!(ids(&diff.invalidated), ["build"]);
        assert_eq!(ids(&diff.interrupted), ["flash"]);
        assert_eq!(ids(&diff.updated_pending), ["verify", "report"]);
    }

    #[test]
    fn same_digest_is_a_noop() {
        let mut plan = esp32_plan();
        finish(&mut plan, &["code", "build"]);
        let diff = plan.apply_input_changes(&change("sdkconfig", "s1"));
        assert!(diff.is_noop());
        assert_eq!(plan.version, 1);
    }

    #[test]
    fn rejects_forward_or_unknown_dependencies() {
        let err = Plan::new(vec![
            PlanNode::new("b", NodeKind::Build, "b").after(&["a"]),
            PlanNode::new("a", NodeKind::Analysis, "a"),
        ])
        .unwrap_err();
        assert!(matches!(err, PlanError::BadDependency { .. }));
    }
}
