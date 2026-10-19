//! Tool identities and the tool registry.
//!
//! Tools are usable, never placeable, items. A tool has no grid footprint, no
//! model and no price here; those belong to blocks (see `blocks::definition`)
//! or to later milestones. `ToolKind` is the single extension point: a future
//! tool-use system matches on it. Only the hammer exists, and using it is not
//! implemented yet.
//!
//! Id convention: tool ids follow the stage format's stable ids (`hammer`,
//! `tape_measure`), i.e. dot-separated `[a-z][a-z0-9_]*` segments with at least
//! one segment. Block ids additionally require a namespace
//! (`block.wood_log`); the two id kinds never collide because the inventory
//! keys them by kind (see [`super::ItemRef`]).

use std::collections::HashMap;
use std::fmt;

use bevy::prelude::*;
use serde_json::Value;
use thiserror::Error;

use crate::blocks::definition::is_valid_segment;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToolId(String);

impl ToolId {
    /// Wraps a string without checking it; validation happens at registration.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), ToolError> {
        if !self.0.is_empty() && self.0.split('.').all(is_valid_segment) {
            Ok(())
        } else {
            Err(ToolError::InvalidId(self.0.clone()))
        }
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a tool does when used. Behaviour is implemented per kind in a later
/// milestone; adding a kind here is the only registration step a new tool needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Hammer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub display_name: String,
    pub kind: ToolKind,
}

impl ToolDefinition {
    pub fn new(id: ToolId, display_name: impl Into<String>, kind: ToolKind) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolError {
    #[error("invalid tool id '{0}': expected dot-separated [a-z][a-z0-9_]* segments")]
    InvalidId(String),
    #[error("tool id '{0}' is already registered")]
    Duplicate(ToolId),
    #[error("tool '{0}' has an empty display name")]
    EmptyName(ToolId),
    #[error("invalid tool list: {0}")]
    InvalidList(String),
}

/// All known tools, in registration order.
#[derive(Resource, Default, Debug)]
pub struct ToolRegistry {
    defs: Vec<ToolDefinition>,
    index: HashMap<ToolId, usize>,
}

impl ToolRegistry {
    pub fn register(&mut self, def: ToolDefinition) -> Result<(), ToolError> {
        def.id.validate()?;
        if def.display_name.trim().is_empty() {
            return Err(ToolError::EmptyName(def.id));
        }
        if self.index.contains_key(&def.id) {
            return Err(ToolError::Duplicate(def.id));
        }
        self.index.insert(def.id.clone(), self.defs.len());
        self.defs.push(def);
        Ok(())
    }

    pub fn get(&self, id: &ToolId) -> Option<&ToolDefinition> {
        self.index.get(id).map(|&i| &self.defs[i])
    }

    pub fn contains(&self, id: &ToolId) -> bool {
        self.index.contains_key(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ToolDefinition> {
        self.defs.iter()
    }
}

pub const HAMMER_ID: &str = "hammer";

pub fn hammer_id() -> ToolId {
    ToolId::new(HAMMER_ID)
}

/// Registers the built-in tools (currently only the hammer).
pub fn register_builtin_tools(registry: &mut ToolRegistry) -> Result<(), ToolError> {
    registry.register(ToolDefinition::new(hammer_id(), "Hammer", ToolKind::Hammer))
}

/// Parses `construction.allowed_tools` (`[{id: hammer}, ...]`). A missing list
/// means no tools are allowed. Ids are not checked against the registry here.
pub fn parse_allowed_tools(construction: &Value) -> Result<Vec<ToolId>, ToolError> {
    let Some(list) = construction.get("allowed_tools") else {
        return Ok(Vec::new());
    };
    let list = list
        .as_array()
        .ok_or_else(|| ToolError::InvalidList("allowed_tools must be an array".into()))?;
    list.iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(ToolId::new)
                .ok_or_else(|| ToolError::InvalidList("entry without a string 'id'".into()))
        })
        .collect()
}
