//! The central, deterministic collection of block definitions.

use std::collections::HashMap;

use bevy::prelude::*;

use super::definition::{BlockDefinition, BlockId};
use super::error::BlockError;

/// All registered [`BlockDefinition`]s. Insertion order is preserved so
/// iteration is deterministic. Definitions are read-only after registration.
#[derive(Resource, Default, Debug)]
pub struct BlockRegistry {
    definitions: Vec<BlockDefinition>,
    index: HashMap<BlockId, usize>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates and adds a definition. Rejects duplicate ids.
    pub fn register(&mut self, definition: BlockDefinition) -> Result<(), BlockError> {
        definition.validate()?;
        if self.index.contains_key(&definition.id) {
            return Err(BlockError::DuplicateId(definition.id));
        }
        self.index
            .insert(definition.id.clone(), self.definitions.len());
        self.definitions.push(definition);
        Ok(())
    }

    pub fn get(&self, id: &BlockId) -> Option<&BlockDefinition> {
        self.index.get(id).map(|&i| &self.definitions[i])
    }

    /// Like [`get`](Self::get) but a missing id is an error.
    pub fn require(&self, id: &BlockId) -> Result<&BlockDefinition, BlockError> {
        self.get(id)
            .ok_or_else(|| BlockError::NotRegistered(id.clone()))
    }

    pub fn contains(&self, id: &BlockId) -> bool {
        self.index.contains_key(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &BlockDefinition> {
        self.definitions.iter()
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}
