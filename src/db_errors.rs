//Code by Sohum Pathak
//sohum.pathak@protonmail.com
use std::fmt;


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
    ChecksumMismatch,
    CorruptedWAL,
    InsufficientParams,
    CorruptedLRU,
    TableNameExists,
    ColumnAbsent,
    InvalidComparison,
    TypeMismatch,
    TableAbsent,
    MalformedRequest,
    InvalidOperation,
    InvalidColumn,
    DBExists,
    DBAbsent,
    QueueClosed,
    TooBigRecord,
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            DbError::RecordMismatch => "Record data corrupted.",
            DbError::SpaceOver => "Page is out of space.",
            DbError::RecordAbsent => "Record doesn't exist.",
            DbError::PageAbsent => "Given page ID absent.",
            DbError::CorruptedDataError => "Corrupted data.",
            DbError::DuplicateKey => "Duplicate key.",
            DbError::FileError => "Error while writing file.",
            DbError::PageTypeMismatch => "Unknown page type.",
            DbError::PageFlagMismatch => "Unknown page flag.",
            DbError::PageCorrupted => "Corrupted page.",
            DbError::MagicMismatch => "Page magic doesn't match DB magic.",
            DbError::ChecksumMismatch => "Checksum mismatch.",
            DbError::CorruptedWAL => "Corrupted WAL.",
            DbError::InsufficientParams => "Not enough args to function.",
            DbError::CorruptedLRU => "Corrupted LRU.",
            DbError::TableNameExists => "Table name exists.",
            DbError::ColumnAbsent => "No such column.",
            DbError::InvalidComparison => "Can't compare these two values.",
            DbError::TypeMismatch => "No such type.",
            DbError::TableAbsent => "No such table.",
            DbError::MalformedRequest => "Bad request.",
            DbError::InvalidOperation => "No such operation.",
            DbError::InvalidColumn => "No such column.",
            DbError::DBExists => "Database exists.",
            DbError::DBAbsent => "Database doesn't exist.",
            DbError::QueueClosed => "Queue is full.",
            DbError::TooBigRecord => "The record is too big.",
        };

        write!(f, "{message}")
    }
}
