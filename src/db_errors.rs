//Code by Sohum Pathak
//sohum.pathak@protonmail.com
use std::fmt;

//Data is corrupted at the byte level
#[derive(Debug)]
pub struct CorruptedDataError;
impl fmt::Display for CorruptedDataError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Corrupted data")
    }
}
impl std::error::Error for CorruptedDataError {}

//Record found with the same key
#[derive(Debug)]
pub struct DuplicateKey;
impl fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Duplicate key")
    }
}
impl std::error::Error for DuplicateKey {}

//Page type out of known values
#[derive(Debug)]
pub struct PageTypeMismatch;
impl fmt::Display for PageTypeMismatch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Unknown page type")
    }
}
impl std::error::Error for PageTypeMismatch {}

//Page flags out of known values
#[derive(Debug)]
pub struct PageFlagMismatch;
impl fmt::Display for PageFlagMismatch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Unknown page flag")
    }
}
impl std::error::Error for PageFlagMismatch {}

//Page id not found
#[derive(Debug)]
pub struct PageAbsent;
impl fmt::Display for PageAbsent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Given page ID absent")
    }
}
impl std::error::Error for PageAbsent {}

//Page has corrupted flag
#[derive(Debug)]
pub struct PageCorrupted;
impl fmt::Display for PageCorrupted {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Corrupted page")
    }
}
impl std::error::Error for PageCorrupted {}

//Page doesn't have the record id
#[derive(Debug)]
pub struct RecordAbsent;
impl fmt::Display for RecordAbsent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Record doesn't exist.")
    }
}
impl std::error::Error for RecordAbsent {}

//Page doesn't have space to add a record
#[derive(Debug)]
pub struct SpaceOver;
impl fmt::Display for SpaceOver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Page is out of space.")
    }
}
impl std::error::Error for SpaceOver {}

//error during file IO
#[derive(Debug)]
pub struct FileError;
impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error while writing file.")
    }
}
impl std::error::Error for FileError {}

//page magic is wrong
#[derive(Debug)]
pub struct MagicMismatch;
impl fmt::Display for MagicMismatch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Page magic doesn't match DB magic.")
    }
}
impl std::error::Error for MagicMismatch {}

//Record id doesn't match the slot id or an offset issue
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
    DuplicateKey,
    FileError,
    PageTypeMismatch,
    PageFlagMismatch,
    PageCorrupted,
    MagicMismatch,
}
