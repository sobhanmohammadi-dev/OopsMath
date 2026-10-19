//! Stage-specific construction rules, kept separate from global defaults.
//!
//! Precedence: a stage rule's `cost`, when present, replaces the definition's
//! `economy.default_unit_price`. Everything else in the definition is global.
//! This module only *parses and resolves* rules; it does not change the stage
//! schema, the compiler or the DAT format, and nothing enforces counts yet.

use std::collections::HashMap;

use bevy::prelude::Resource;
use serde_json::Value;

use super::definition::{BlockDefinition, BlockId, MAX_UNIT_PRICE};
use super::error::BlockError;
use super::registry::BlockRegistry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageBlockRule {
    pub block: BlockId,
    /// Stage price override in coins.
    pub cost: Option<u32>,
    pub min_count: u32,
    pub max_count: Option<u32>,
}

impl StageBlockRule {
    pub fn new(block: BlockId) -> Self {
        Self {
            block,
            cost: None,
            min_count: 0,
            max_count: None,
        }
    }

    pub fn validate(&self) -> Result<(), BlockError> {
        let bad = |msg: String| BlockError::InvalidStageRule(format!("'{}': {msg}", self.block));
        if let Some(cost) = self.cost
            && cost > MAX_UNIT_PRICE
        {
            return Err(bad(format!("cost {cost} exceeds {MAX_UNIT_PRICE}")));
        }
        if let Some(max) = self.max_count
            && self.min_count > max
        {
            return Err(bad(format!(
                "min_count {} is greater than max_count {max}",
                self.min_count
            )));
        }
        Ok(())
    }

    /// Stage cost if given, otherwise the definition's default.
    pub fn effective_price(&self, def: &BlockDefinition) -> u32 {
        self.cost.unwrap_or(def.economy.default_unit_price)
    }

    pub fn allows_count(&self, count: u32) -> bool {
        count >= self.min_count && self.max_count.is_none_or(|max| count <= max)
    }

    /// The registered definition this rule refers to.
    pub fn definition<'r>(
        &self,
        registry: &'r BlockRegistry,
    ) -> Result<&'r BlockDefinition, BlockError> {
        registry.require(&self.block)
    }
}

/// Parses the `construction.allowed_blocks` array of a stage definition.
///
/// A missing array yields no rules. Ids are *not* checked against the registry
/// here (existing stages use ids such as `brick` that are not registered yet);
/// use [`StageBlockRule::definition`] when resolution is required.
pub fn parse_allowed_blocks(construction: &Value) -> Result<Vec<StageBlockRule>, BlockError> {
    let Some(entries) = construction.get("allowed_blocks") else {
        return Ok(Vec::new());
    };
    let entries = entries
        .as_array()
        .ok_or_else(|| BlockError::InvalidStageRule("allowed_blocks must be an array".into()))?;

    let mut rules = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| BlockError::InvalidStageRule("entry without a string 'id'".into()))?;
        let count = |key: &str| -> Result<Option<u32>, BlockError> {
            match entry.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => v
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .map(Some)
                    .ok_or_else(|| {
                        BlockError::InvalidStageRule(format!(
                            "'{id}': '{key}' must be a non-negative integer"
                        ))
                    }),
            }
        };
        let rule = StageBlockRule {
            block: BlockId::new(id),
            cost: count("cost")?,
            min_count: count("min_count")?.unwrap_or(0),
            max_count: count("max_count")?,
        };
        rule.validate()?;
        rules.push(rule);
    }
    Ok(rules)
}

/// The active stage's authoritative placement permissions.
///
/// Semantics (no schema change; the existing `construction.allowed_blocks`
/// entries are reused):
/// * A block may be placed only if it has an entry. A stage with no entries
///   (including legacy stages) permits **nothing**; absence never means
///   "unrestricted".
/// * `max_count` is the maximum number of placed *instances* of that block that
///   may exist at once (not cells, not the starting inventory). Absent means
///   no instance cap for that block; invalid values are a parse error.
/// * A malformed list yields no permissions and a diagnostic in `error`.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct StagePlacementRules {
    rules: HashMap<BlockId, StageBlockRule>,
    pub error: Option<String>,
}

impl StagePlacementRules {
    pub fn from_construction(construction: &Value) -> Self {
        match parse_allowed_blocks(construction) {
            Ok(list) => Self {
                rules: list.into_iter().map(|r| (r.block.clone(), r)).collect(),
                error: None,
            },
            Err(err) => Self {
                rules: HashMap::new(),
                error: Some(err.to_string()),
            },
        }
    }

    pub fn is_allowed(&self, id: &BlockId) -> bool {
        self.rules.contains_key(id)
    }

    pub fn max_placed(&self, id: &BlockId) -> Option<u32> {
        self.rules.get(id).and_then(|r| r.max_count)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
