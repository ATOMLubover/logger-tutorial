use std::fmt;

use crate::level::Level;

pub struct Metadata {
    level: Level,
    // TODO: target
}

impl Metadata {
    pub fn new(level: Level) -> Self {
        Self { level }
    }

    pub fn level(&self) -> Level {
        self.level
    }
}

pub struct Record<'a> {
    metadata: Metadata,
    timestamp: u64,
    arguments: fmt::Arguments<'a>,
    // TODO: values
}

impl<'a> Record<'a> {
    pub fn new(metadata: Metadata, timestamp: u64, arguments: fmt::Arguments<'a>) -> Self {
        Self {
            metadata,
            timestamp,
            arguments,
        }
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn arguments(&self) -> fmt::Arguments<'a> {
        self.arguments
    }
}
