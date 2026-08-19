use std::fmt;

#[derive(Debug)]
pub struct CorruptedDataError;

impl fmt::Display for CorruptedDataError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Corrupted data")
    }
}

impl std::error::Error for CorruptedDataError {}

#[derive(Debug)]
pub struct PageAbsent;

impl fmt::Display for PageAbsent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Given page ID absent")
    }
}

impl std::error::Error for PageAbsent {}

#[derive(Debug)]
pub struct RecordAbsent;

impl fmt::Display for RecordAbsent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Record doesn't exist.")
    }
}

impl std::error::Error for RecordAbsent {}

#[derive(Debug)]
pub struct SpaceOver;

impl fmt::Display for SpaceOver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Page is out of space.")
    }
}

impl std::error::Error for SpaceOver {}

#[derive(Debug)]
pub struct RecordMismatch;

impl fmt::Display for RecordMismatch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Record data corrupted.")
    }
}

impl std::error::Error for RecordMismatch {}

#[derive(Debug)]
pub enum DbError{
    RecordMismatch,
    SpaceOver,
    RecordAbsent,
    PageAbsent,
    CorruptedDataError,
}
