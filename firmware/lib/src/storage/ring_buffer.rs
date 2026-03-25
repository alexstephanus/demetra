cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::vec::Vec;
        use std::vec;
    } else {
        use alloc::vec::Vec;
        use alloc::vec;
    }
}
use core::option::Option;
use core::{cmp::min, fmt::Debug, marker::PhantomData};
#[cfg(any(test, feature = "simulation"))]
use embedded_storage::ReadStorage;
use embedded_storage::Storage;
use log::debug;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

use crate::config::calibration::types::TimestampedValue;

#[derive(Error, Debug)]
#[error("Address 0x{address:08X} is not aligned to flash page size (4096 bytes)")]
pub struct MisalignedAddress {
    pub address: u32,
}

#[derive(Error, Debug)]
pub enum RingBufferError<T, E> {
    #[error("Failed to serialize record: {error}")]
    SerializationError { record: T, error: serde_json::Error },
    #[error("Failed to deserialize record: {error}")]
    DeserializationError {
        raw_bytes: Vec<u8>,
        error: serde_json::Error,
    },
    #[error("Flash storage error: {0:?}")]
    StorageError(E),
    #[error("Record too large: {size} bytes (max: {max_size} bytes)")]
    RecordTooLarge { size: usize, max_size: usize },
    #[error("Too many chunks: {chunk_count} (max: {max_chunks})")]
    TooManyChunks {
        chunk_count: usize,
        max_chunks: usize,
    },
    #[error("Chunk too big: {chunk_size} bytes (max: {max_chunk_size} bytes)")]
    ChunkTooBig {
        chunk_size: usize,
        max_chunk_size: usize,
    },
    #[error(
        "CRC mismatch at address 0x{address:08X}: stored 0x{stored:08X}, computed 0x{computed:08X}"
    )]
    CrcMismatch {
        address: u32,
        stored: u32,
        computed: u32,
    },
}

const PARTITION_SIZE: u32 = 4096;

// Trait for safe metadata serialization
pub trait MetadataSerialize: Copy + Clone + Default {
    fn serialize_to_bytes(&self) -> Vec<u8>;
    fn deserialize_from_bytes(bytes: &[u8]) -> Self;
    fn serialized_size() -> usize;
}

// Empty metadata for simple ring buffers
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyMetadata;

impl MetadataSerialize for EmptyMetadata {
    fn serialize_to_bytes(&self) -> Vec<u8> {
        vec![] // Empty metadata serializes to empty bytes
    }

    fn deserialize_from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.is_empty(), "EmptyMetadata must have zero bytes");
        EmptyMetadata
    }

    fn serialized_size() -> usize {
        0
    }
}

// Type alias for simple ring buffers without metadata
pub type SimpleRingBuffer<T, S, E> = RingBuffer<T, EmptyMetadata, S, E>;

#[repr(C, packed)]
struct PageMetadataHeader {
    first_record_id: u32,
    first_record_address: u32,
}

#[repr(C, packed)]
pub struct RecordHeader {
    pub record_length: u32,
    pub previous_record_length: u32,
    pub payload_crc: u32,
}

const PAGE_METADATA_SIZE: u32 = core::mem::size_of::<PageMetadataHeader>() as u32;
const RECORD_HEADER_SIZE: u32 = core::mem::size_of::<RecordHeader>() as u32;
const UNINITIALIZED_PAGE_OFFSET: u32 = PAGE_METADATA_SIZE;
const MAX_CHUNK_SIZE: u32 = PARTITION_SIZE - PAGE_METADATA_SIZE;
const MAX_CHUNKS: usize = 10;
const MAX_RECORD_SIZE: u32 = 10000;

#[derive(Clone, Copy, Debug)]
pub struct PageAddress(u32);
#[derive(Clone, Copy, Debug)]
struct RecordAddress(u32);

struct PageMetadata {
    address: PageAddress,
    first_record_id: u32,
    first_record_address: RecordAddress,
}

pub struct RingBuffer<T, M, S, E> {
    start_address: PageAddress,
    end_address: PageAddress,
    latest_record_address: RecordAddress,
    next_record_id: u32,
    flash: S,
    _record_type: PhantomData<T>,
    _metadata_type: PhantomData<M>,
    _storage_error_type: PhantomData<E>,
}

impl<T, M, S, E> RingBuffer<T, M, S, E>
where
    T: Serialize + DeserializeOwned + TimestampedValue + Clone,
    M: MetadataSerialize,
    S: Storage<Error = E>,
    E: Debug,
{
    /// Generic ring buffer that stores records of type T with metadata of type M
    /// Record format: [Record Length: 4][Previous Record Length: 4][Payload CRC-32: 4][Metadata: M][JSON Data: T]
    pub fn new(start_address: u32, end_address: u32, flash: S) -> Result<Self, MisalignedAddress> {
        if !start_address.is_multiple_of(PARTITION_SIZE) {
            return Err(MisalignedAddress {
                address: start_address,
            });
        }
        if !end_address.is_multiple_of(PARTITION_SIZE) {
            return Err(MisalignedAddress {
                address: end_address,
            });
        }

        let mut ring_buffer = Self {
            start_address: PageAddress(start_address),
            end_address: PageAddress(end_address),
            latest_record_address: RecordAddress(start_address + UNINITIALIZED_PAGE_OFFSET),
            next_record_id: 0,
            flash,
            _record_type: PhantomData,
            _metadata_type: PhantomData,
            _storage_error_type: PhantomData,
        };

        match ring_buffer.find_latest_record_address() {
            Ok(Some((address, next_id))) => {
                ring_buffer.latest_record_address = address;
                ring_buffer.next_record_id = next_id;
            }
            Ok(None) => {}
            Err(e) => {
                log::error!("Flash read error scanning ring buffer on init: {}", e);
            }
        }

        Ok(ring_buffer)
    }

    fn is_empty(&mut self) -> Result<bool, RingBufferError<T, E>> {
        let mut record_length_bytes = [0u8; 4];
        self.flash
            .read(self.latest_record_address.0, &mut record_length_bytes)
            .map_err(RingBufferError::StorageError)?;
        Ok(!Self::are_bytes_written(&record_length_bytes))
    }

    pub fn read_latest_record(&mut self) -> Result<Option<(T, M)>, RingBufferError<T, E>> {
        if self.is_empty()? {
            return Ok(None);
        }
        let mut current = self.latest_record_address;
        loop {
            match self.read_record(current) {
                Ok(result) => return Ok(result),
                Err(RingBufferError::CrcMismatch { address, .. }) => {
                    log::warn!(
                        "CRC mismatch at 0x{:08X}, falling back to previous record",
                        address
                    );
                    match self.get_previous_record_address(current)? {
                        Some(prev) => current = prev,
                        None => return Ok(None),
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn write_record(&mut self, record: &T, metadata: M) -> Result<(), RingBufferError<T, E>> {
        let record_body_bytes = match serde_json::to_vec(record) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(RingBufferError::SerializationError {
                    record: record.clone(),
                    error,
                });
            }
        };

        let json_data_length = record_body_bytes.len() as u32;

        let next_record_address = if self.is_empty()? {
            self.start_address.0 + UNINITIALIZED_PAGE_OFFSET
        } else {
            match self.get_next_record_address(self.latest_record_address)? {
                Some(address) => address.0,
                None => self.start_address.0 + UNINITIALIZED_PAGE_OFFSET,
            }
        };

        let previous_record_address_distance = if self.is_empty()? {
            0
        } else {
            if next_record_address >= self.latest_record_address.0 {
                next_record_address - self.latest_record_address.0
            } else {
                (self.end_address.0 - self.latest_record_address.0)
                    + (next_record_address - self.start_address.0)
            }
        };

        let metadata_bytes = self.serialize_metadata(&metadata);

        let mut payload_bytes = Vec::new();
        payload_bytes.extend_from_slice(&metadata_bytes);
        payload_bytes.extend_from_slice(&record_body_bytes);
        let payload_crc = crc32fast::hash(&payload_bytes);

        let mut record_bytes = Vec::new();
        record_bytes.extend_from_slice(&json_data_length.to_le_bytes());
        record_bytes.extend_from_slice(&previous_record_address_distance.to_le_bytes());
        record_bytes.extend_from_slice(&payload_crc.to_le_bytes());
        record_bytes.extend(payload_bytes);

        let mut address_to_write = next_record_address;
        let mut bytes_written = 0;
        let mut actual_record_address = next_record_address;

        let total_record_size = record_bytes.len() as u32;
        let chunks = self.chunk_record(total_record_size, next_record_address);

        debug!("Chunks: {:?}", chunks);

        for (chunk_number, &chunk_length) in chunks.iter().enumerate() {
            if address_to_write % PARTITION_SIZE <= PAGE_METADATA_SIZE {
                let current_page_address = address_to_write - (address_to_write % PARTITION_SIZE);
                let first_record_address = if chunk_number == 0 {
                    current_page_address + PAGE_METADATA_SIZE
                } else {
                    current_page_address + PAGE_METADATA_SIZE + self.pad_to_word(chunk_length)
                };

                self.flash
                    .write(current_page_address, &[0xff; PARTITION_SIZE as usize])
                    .map_err(RingBufferError::StorageError)?;

                self.write_page_metadata(
                    PageAddress(current_page_address),
                    RecordAddress(first_record_address),
                    self.next_record_id,
                )?;
                address_to_write = current_page_address + PAGE_METADATA_SIZE;
                if chunk_number == 0 {
                    actual_record_address = address_to_write;
                }
            }

            self.flash
                .write(
                    address_to_write,
                    &record_bytes
                        [bytes_written as usize..bytes_written as usize + chunk_length as usize],
                )
                .map_err(RingBufferError::StorageError)?;
            bytes_written += chunk_length;
            address_to_write = self.get_next_page_address(address_to_write).0;
        }

        self.latest_record_address = RecordAddress(actual_record_address);
        self.next_record_id += 1;
        Ok(())
    }

    pub fn iter(&mut self) -> RingBufferIterator<'_, T, M, S, E> {
        RingBufferIterator::new(self, self.latest_record_address, IterDirection::Forward)
    }

    pub fn iter_reverse(&mut self) -> RingBufferIterator<'_, T, M, S, E> {
        RingBufferIterator::new(self, self.latest_record_address, IterDirection::Reverse)
    }

    pub fn iter_filtered<F>(&mut self, predicate: F) -> RingBufferIterator<'_, T, M, S, E, F>
    where
        F: Fn(&M) -> bool,
    {
        RingBufferIterator::new_filtered(self, self.latest_record_address, predicate)
    }

    fn read_record(
        &mut self,
        address: RecordAddress,
    ) -> Result<Option<(T, M)>, RingBufferError<T, E>> {
        let json_data_length = match self.get_record_length(address)? {
            None => return Ok(None),
            Some(length) => length,
        };

        if json_data_length > MAX_RECORD_SIZE {
            debug!("Record length too big: {}, skipping", json_data_length);
            return Ok(None);
        }

        let metadata_size = M::serialized_size() as u32;
        let total_read_size = RECORD_HEADER_SIZE + metadata_size + json_data_length;

        let mut record_bytes = vec![0xff; total_read_size as usize];

        let mut address_to_read = address.0;
        debug!(
            "Reading {} bytes from address {}",
            total_read_size, address_to_read
        );
        let mut bytes_read: usize = 0;
        let chunks = self.chunk_record(total_read_size, address_to_read);

        if chunks.len() > MAX_CHUNKS {
            return Err(RingBufferError::TooManyChunks {
                chunk_count: chunks.len(),
                max_chunks: MAX_CHUNKS,
            });
        }

        for &chunk in chunks.iter() {
            if chunk > MAX_CHUNK_SIZE {
                return Err(RingBufferError::ChunkTooBig {
                    chunk_size: chunk as usize,
                    max_chunk_size: MAX_CHUNK_SIZE as usize,
                });
            }
            self.flash
                .read(
                    address_to_read,
                    &mut record_bytes[bytes_read..bytes_read + chunk as usize],
                )
                .map_err(RingBufferError::StorageError)?;
            bytes_read += chunk as usize;
            address_to_read = self.get_next_page_address(address_to_read).0 + PAGE_METADATA_SIZE;
        }

        let stored_crc = u32::from_le_bytes([
            record_bytes[8],
            record_bytes[9],
            record_bytes[10],
            record_bytes[11],
        ]);

        let metadata_start = RECORD_HEADER_SIZE as usize;
        let metadata_end = metadata_start + metadata_size as usize;
        let json_start = metadata_end;
        let json_end = json_start + json_data_length as usize;

        let computed_crc = crc32fast::hash(&record_bytes[metadata_start..json_end]);
        if stored_crc != computed_crc {
            return Err(RingBufferError::CrcMismatch {
                address: address.0,
                stored: stored_crc,
                computed: computed_crc,
            });
        }

        let metadata = self.deserialize_metadata(&record_bytes[metadata_start..metadata_end]);
        let json_bytes = &record_bytes[json_start..json_end];

        let record: T = match serde_json::from_slice::<T>(json_bytes) {
            Ok(decoded) => decoded,
            Err(e) => {
                debug!(
                    "Failed to deserialize record at address {}: {:?}",
                    address.0, e
                );
                debug!(
                    "JSON length: {}, bytes length: {}",
                    json_data_length,
                    json_bytes.len()
                );
                return Err(RingBufferError::DeserializationError {
                    raw_bytes: json_bytes.to_vec(),
                    error: e,
                });
            }
        };

        Ok(Some((record, metadata)))
    }

    fn get_previous_record_address(
        &mut self,
        current_address: RecordAddress,
    ) -> Result<Option<RecordAddress>, RingBufferError<T, E>> {
        let mut prev_length_bytes = [0u8; 4];
        self.flash
            .read(current_address.0 + 4, &mut prev_length_bytes)
            .map_err(RingBufferError::StorageError)?;
        let previous_record_total_length = u32::from_le_bytes(prev_length_bytes);

        if previous_record_total_length == 0 {
            return Ok(None);
        }

        if current_address.0 < previous_record_total_length {
            return Ok(None);
        }

        Ok(Some(RecordAddress(
            current_address.0 - previous_record_total_length,
        )))
    }

    fn chunk_record(&self, record_length: u32, start_address: u32) -> Vec<u32> {
        let mut chunks: Vec<u32> = Vec::<u32>::new();
        let mut remaining_record_length = record_length;
        let mut chunk_start = start_address;
        while remaining_record_length > 0 {
            let current_page_start = chunk_start - chunk_start % PARTITION_SIZE;
            let current_page_end = current_page_start + PARTITION_SIZE;
            let chunk_size = min(remaining_record_length, current_page_end - chunk_start);
            chunks.push(chunk_size);
            remaining_record_length -= chunk_size;
            let next_chunk_address = self.get_next_page_address(chunk_start).0;
            chunk_start = next_chunk_address + PAGE_METADATA_SIZE;
        }
        chunks
    }

    fn find_latest_record_address(
        &mut self,
    ) -> Result<Option<(RecordAddress, u32)>, RingBufferError<T, E>> {
        let latest_page = match self.get_last_written_page()? {
            None => return Ok(None),
            Some(metadata) => metadata,
        };
        let base_id = latest_page.first_record_id;
        self.find_latest_record_in_page(latest_page, base_id, 0)
    }

    fn write_page_metadata(
        &mut self,
        page_address: PageAddress,
        first_record_address: RecordAddress,
        record_id: u32,
    ) -> Result<(), RingBufferError<T, E>> {
        let id_bytes = record_id.to_le_bytes();
        let address_bytes = first_record_address.0.to_le_bytes();
        self.flash
            .write(page_address.0, &id_bytes)
            .map_err(RingBufferError::StorageError)?;
        self.flash
            .write(page_address.0 + 4, &address_bytes)
            .map_err(RingBufferError::StorageError)?;
        Ok(())
    }

    fn find_latest_record_in_page(
        &mut self,
        page: PageMetadata,
        base_id: u32,
        records_counted: u32,
    ) -> Result<Option<(RecordAddress, u32)>, RingBufferError<T, E>> {
        debug!("Getting latest record for page {}", page.address.0);
        match page.first_record_address.0 == page.address.0 + PARTITION_SIZE {
            true => {
                debug!("Page full of a record chunk, going to previous page");
                let previous_page_address = self.get_previous_page_address(page.address.0);
                match self.get_page_metadata(previous_page_address.0)? {
                    None => Ok(None),
                    Some(previous_page) => {
                        self.find_latest_record_in_page(previous_page, base_id, records_counted)
                    }
                }
            }
            false => {
                let mut latest_record_address = page.first_record_address;
                let mut count = records_counted;
                loop {
                    debug!("Getting next record address in page {}", page.address.0);
                    match self.get_next_record_address(latest_record_address)? {
                        None => {
                            let previous_page_address =
                                self.get_previous_page_address(page.address.0);
                            match self.get_page_metadata(previous_page_address.0)? {
                                None => return Ok(None),
                                Some(previous_page) => {
                                    return self.find_latest_record_in_page(
                                        previous_page,
                                        base_id,
                                        count,
                                    )
                                }
                            }
                        }
                        Some(next_address) => match self.get_record_length(next_address)? {
                            None => return Ok(Some((latest_record_address, base_id + count + 1))),
                            Some(_valid_length) => {
                                latest_record_address = next_address;
                                count += 1;
                            }
                        },
                    }
                }
            }
        }
    }

    fn get_last_written_page(&mut self) -> Result<Option<PageMetadata>, RingBufferError<T, E>> {
        let first_page_metadata = self.get_page_metadata(self.start_address.0)?;

        match first_page_metadata {
            None => Ok(None),
            Some(mut latest_page_metadata) => {
                debug!(
                    "Got page metadata for address {}.  First record address: {}",
                    latest_page_metadata.address.0, latest_page_metadata.first_record_address.0
                );
                for page_address in ((self.start_address.0 + PARTITION_SIZE)..self.end_address.0)
                    .step_by(PARTITION_SIZE as usize)
                {
                    if let Some(metadata) = self.get_page_metadata(page_address)? {
                        debug!(
                            "Got page metadata for address {}.  First record address: {}",
                            metadata.address.0, metadata.first_record_address.0
                        );
                        if metadata.first_record_id > latest_page_metadata.first_record_id {
                            latest_page_metadata = metadata;
                        }
                    }
                }
                Ok(Some(latest_page_metadata))
            }
        }
    }

    fn get_next_record_address(
        &mut self,
        record_address: RecordAddress,
    ) -> Result<Option<RecordAddress>, RingBufferError<T, E>> {
        match self.get_record_length(record_address)? {
            Some(json_length) => {
                debug!(
                    "Got record length for address {} %4 = {}: {}",
                    record_address.0,
                    record_address.0 % 4,
                    json_length
                );
                let metadata_size = M::serialized_size() as u32;
                let total_record_length = RECORD_HEADER_SIZE + metadata_size + json_length;

                let mut next_record_address = record_address.0;
                let chunks = self.chunk_record(total_record_length, record_address.0);
                for _ in 0..(chunks.len() - 1) {
                    next_record_address =
                        self.get_next_page_address(next_record_address).0 + PAGE_METADATA_SIZE
                }
                debug!(
                    "Returning new address: {}",
                    self.pad_to_word(next_record_address + chunks[chunks.len() - 1])
                );
                Ok(Some(RecordAddress(self.pad_to_word(
                    next_record_address + chunks[chunks.len() - 1],
                ))))
            }
            None => Ok(None),
        }
    }

    fn get_record_length(
        &mut self,
        address: RecordAddress,
    ) -> Result<Option<u32>, RingBufferError<T, E>> {
        let mut record_length_bytes = [0xffu8; 4];
        self.flash
            .read(address.0, &mut record_length_bytes)
            .map_err(RingBufferError::StorageError)?;
        match Self::are_bytes_written(&record_length_bytes) {
            true => {
                let length = u32::from_le_bytes(record_length_bytes);
                if length < 100000 && length > 0 {
                    Ok(Some(length))
                } else {
                    Ok(None)
                }
            }
            false => Ok(None),
        }
    }

    fn get_next_page_address(&self, address: u32) -> PageAddress {
        let current_page_address = match address % PARTITION_SIZE {
            0 => address,
            _ => address - (address % PARTITION_SIZE),
        };
        let next_page_address = current_page_address + PARTITION_SIZE;
        if next_page_address >= self.end_address.0 {
            return self.start_address;
        }
        PageAddress(next_page_address)
    }

    fn get_previous_page_address(&mut self, address: u32) -> PageAddress {
        let current_page_address = match address % PARTITION_SIZE {
            0 => address,
            _ => address - (address % PARTITION_SIZE),
        };
        if current_page_address == self.start_address.0 {
            return PageAddress(self.end_address.0 - PARTITION_SIZE);
        }
        PageAddress(current_page_address - PARTITION_SIZE)
    }

    fn get_page_metadata(
        &mut self,
        address: u32,
    ) -> Result<Option<PageMetadata>, RingBufferError<T, E>> {
        let page_start = address - (address % PARTITION_SIZE);
        let mut id_bytes = [0xffu8; 4];
        let mut first_record_bytes = [0xffu8; 4];
        self.flash
            .read(page_start, &mut id_bytes)
            .map_err(RingBufferError::StorageError)?;
        self.flash
            .read(page_start + 4, &mut first_record_bytes)
            .map_err(RingBufferError::StorageError)?;
        match Self::are_bytes_written(&id_bytes) {
            false => Ok(None),
            true => Ok(Some(PageMetadata {
                address: PageAddress(page_start),
                first_record_id: u32::from_le_bytes(id_bytes),
                first_record_address: RecordAddress(u32::from_le_bytes(first_record_bytes)),
            })),
        }
    }

    fn are_bytes_written(bytes: &[u8]) -> bool {
        for byte in bytes.iter() {
            // Flash memory on the ESP32 starts out as all 1's, and when you
            // write data to that flash, it does so by setting certain bits to 0.
            // So, to look for unwritten memory we simply look for 0b11111111, or 0xff
            if *byte != 0xff {
                return true;
            }
        }
        false
    }

    fn pad_to_word(&self, address: u32) -> u32 {
        match address % 4 {
            0 => address,
            _ => address + 4 - (address % 4),
        }
    }

    #[cfg(any(test, feature = "simulation"))]
    pub fn storage(self) -> S {
        self.flash
    }

    fn serialize_metadata(&self, metadata: &M) -> Vec<u8> {
        metadata.serialize_to_bytes()
    }

    fn deserialize_metadata(&self, bytes: &[u8]) -> M {
        M::deserialize_from_bytes(bytes)
    }
}

#[derive(Clone, Copy)]
enum IterDirection {
    Forward,
    Reverse,
}

pub struct RingBufferIterator<'a, T, M, S, E, F = fn(&M) -> bool> {
    buffer: &'a mut RingBuffer<T, M, S, E>,
    current_address: Option<RecordAddress>,
    direction: IterDirection,
    predicate: Option<F>,
}

impl<'a, T, M, S, E, F> RingBufferIterator<'a, T, M, S, E, F>
where
    T: Serialize + DeserializeOwned + TimestampedValue + Clone,
    M: MetadataSerialize,
    S: Storage<Error = E>,
    E: Debug,
    F: Fn(&M) -> bool,
{
    fn new(
        buffer: &'a mut RingBuffer<T, M, S, E>,
        start_address: RecordAddress,
        direction: IterDirection,
    ) -> Self {
        let current_address = match buffer.is_empty() {
            Ok(true) | Err(_) => None,
            Ok(false) => Some(start_address),
        };
        Self {
            buffer,
            current_address,
            direction,
            predicate: None,
        }
    }

    fn new_filtered(
        buffer: &'a mut RingBuffer<T, M, S, E>,
        start_address: RecordAddress,
        predicate: F,
    ) -> Self {
        Self {
            buffer,
            current_address: Some(start_address),
            direction: IterDirection::Reverse, // Filtered iteration always goes reverse (latest first)
            predicate: Some(predicate),
        }
    }
}

impl<'a, T, M, S, E, F> Iterator for RingBufferIterator<'a, T, M, S, E, F>
where
    T: Serialize + DeserializeOwned + TimestampedValue + Clone,
    M: MetadataSerialize,
    S: Storage<Error = E>,
    E: Debug,
    F: Fn(&M) -> bool,
{
    type Item = (T, M);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let current = self.current_address?;

            match self.buffer.read_record(current) {
                Ok(Some((record, metadata))) => {
                    self.current_address = match self.direction {
                        IterDirection::Forward => {
                            match self.buffer.get_next_record_address(current) {
                                Ok(addr) => addr,
                                Err(e) => {
                                    log::error!("Storage error advancing iterator: {}", e);
                                    return None;
                                }
                            }
                        }
                        IterDirection::Reverse => {
                            match self.buffer.get_previous_record_address(current) {
                                Ok(addr) => addr,
                                Err(e) => {
                                    log::error!("Storage error advancing iterator: {}", e);
                                    return None;
                                }
                            }
                        }
                    };

                    if let Some(ref predicate) = self.predicate {
                        if predicate(&metadata) {
                            return Some((record, metadata));
                        }
                    } else {
                        return Some((record, metadata));
                    }
                }
                Ok(None) => {
                    return None;
                }
                Err(RingBufferError::CrcMismatch { address, .. }) => {
                    log::warn!("CRC mismatch at 0x{:08X}, skipping corrupt record", address);
                    self.current_address = match self.direction {
                        IterDirection::Forward => {
                            match self.buffer.get_next_record_address(current) {
                                Ok(addr) => addr,
                                Err(e) => {
                                    log::error!(
                                        "Storage error navigating past corrupt record: {}",
                                        e
                                    );
                                    return None;
                                }
                            }
                        }
                        IterDirection::Reverse => match self
                            .buffer
                            .get_previous_record_address(current)
                        {
                            Ok(addr) => addr,
                            Err(e) => {
                                log::error!("Storage error navigating past corrupt record: {}", e);
                                return None;
                            }
                        },
                    };
                }
                Err(e) => {
                    log::error!("Error reading record during iteration: {}", e);
                    return None;
                }
            }
        }
    }
}

#[cfg(any(test, feature = "simulation"))]
pub struct MockFlashStorage {
    data: Vec<u8>,
    start_address: u32,
    end_address: u32,
}

#[derive(Debug)]
pub enum MockFlashStorageError {
    OutOfBounds,
}

#[cfg(any(test, feature = "simulation"))]
impl MockFlashStorage {
    pub fn new(start_address: u32, end_address: u32, data: Option<Vec<u8>>) -> MockFlashStorage {
        let data_vector = match data {
            None => {
                let vector_capacity = end_address - start_address;
                let mut data_vector = Vec::<u8>::with_capacity(vector_capacity as usize);
                for _ in 0..vector_capacity {
                    data_vector.push(0xff);
                }
                data_vector
            }
            Some(existing) => existing,
        };
        MockFlashStorage {
            data: data_vector,
            start_address,
            end_address,
        }
    }
}

#[cfg(any(test, feature = "simulation"))]
impl ReadStorage for MockFlashStorage {
    type Error = MockFlashStorageError;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        if offset < self.start_address || offset + bytes.len() as u32 > self.end_address {
            return Err(Self::Error::OutOfBounds);
        }
        for i in 0..bytes.len() {
            bytes[i] = self.data[(offset - self.start_address) as usize + i];
        }
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.end_address as usize - self.start_address as usize
    }
}

#[cfg(any(test, feature = "simulation"))]
impl Storage for MockFlashStorage {
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        if offset < self.start_address || offset + bytes.len() as u32 > self.end_address {
            return Err(Self::Error::OutOfBounds);
        }
        for i in 0..bytes.len() {
            self.data[(offset - self.start_address) as usize + i] = bytes[i];
        }
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::calibration::types::TimestampedValue;
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::{
        string::{String, ToString},
        vec,
    };

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestTimestampedRecord {
        ts: DateTime<Utc>,
        id: String,
    }

    impl TimestampedValue for TestTimestampedRecord {
        fn get_written_timestamp(&self) -> DateTime<Utc> {
            self.ts
        }
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    struct TestMetadata {
        value: u8,
    }

    impl MetadataSerialize for TestMetadata {
        fn serialize_to_bytes(&self) -> Vec<u8> {
            vec![self.value]
        }

        fn deserialize_from_bytes(bytes: &[u8]) -> Self {
            assert_eq!(bytes.len(), 1, "TestMetadata must be exactly 1 byte");
            Self { value: bytes[0] }
        }

        fn serialized_size() -> usize {
            1
        }
    }

    fn get_test_ring_buffer(
        storage: Option<MockFlashStorage>,
    ) -> RingBuffer<TestTimestampedRecord, TestMetadata, MockFlashStorage, MockFlashStorageError>
    {
        let start_address = 0x0000;
        let end_address = 0x4000;
        let flash = match storage {
            None => MockFlashStorage::new(start_address, end_address, None),
            Some(existing) => existing,
        };
        RingBuffer::<TestTimestampedRecord, TestMetadata, MockFlashStorage, MockFlashStorageError>::new(
            start_address,
            end_address,
            flash,
        ).expect("test ring buffer addresses must be page-aligned")
    }

    fn get_simple_ring_buffer(
        storage: Option<MockFlashStorage>,
    ) -> RingBuffer<TestTimestampedRecord, EmptyMetadata, MockFlashStorage, MockFlashStorageError>
    {
        let start_address = 0x0000;
        let end_address = 0x4000;
        let flash = match storage {
            None => MockFlashStorage::new(start_address, end_address, None),
            Some(existing) => existing,
        };
        RingBuffer::<TestTimestampedRecord, EmptyMetadata, MockFlashStorage, MockFlashStorageError>::new(
            start_address,
            end_address,
            flash,
        ).expect("test ring buffer addresses must be page-aligned")
    }

    fn create_test_record(id: &str, millis: i64) -> TestTimestampedRecord {
        TestTimestampedRecord {
            ts: DateTime::<Utc>::from_timestamp_millis(millis).expect("Invalid timestamp"),
            id: id.to_string(),
        }
    }

    fn create_large_record(size: usize, millis: i64) -> TestTimestampedRecord {
        TestTimestampedRecord {
            ts: DateTime::<Utc>::from_timestamp_millis(millis).expect("Invalid timestamp"),
            id: String::from_utf8(vec![0x41; size]).expect("String decode error"),
        }
    }

    // =============================================================================
    // BASIC FUNCTIONALITY TESTS
    // =============================================================================

    #[test]
    fn test_read_from_empty_buffer_returns_none() {
        let mut buffer = get_test_ring_buffer(None);
        let result = buffer.read_latest_record();
        assert!(
            result.unwrap().is_none(),
            "Reading from empty buffer should return None, not panic"
        );
    }

    #[test]
    fn test_simple_write_and_read() {
        let mut buffer = get_test_ring_buffer(None);
        let record = create_test_record("test_record", 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 42 })
            .unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0, record);
        assert_eq!(result.1.value, 42);
    }

    #[test]
    fn test_write_multiple_records_read_latest() {
        let mut buffer = get_test_ring_buffer(None);
        let record1 = create_test_record("first", 1722581155825);
        let record2 = create_test_record("second", 1722581155826);

        buffer
            .write_record(&record1, TestMetadata { value: 1 })
            .unwrap();
        buffer
            .write_record(&record2, TestMetadata { value: 2 })
            .unwrap();

        let result = buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(result.0, record2);
        assert_eq!(result.1.value, 2);
    }

    #[test]
    fn test_write_over_two_partitions() {
        let mut buffer = get_test_ring_buffer(None);
        let record = create_large_record(5000, 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 99 })
            .unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0, record);
        assert_eq!(result.1.value, 99);
    }

    #[test]
    fn test_write_over_multiple_partitions() {
        let mut buffer = get_test_ring_buffer(None);
        let record = create_large_record(9000, 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 128 })
            .unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0, record);
        assert_eq!(result.1.value, 128);
    }

    #[test]
    fn test_wrap_around_buffer() {
        let mut buffer = get_test_ring_buffer(None);

        let large_record = create_large_record(9000, 1722581155825);
        let small_record = create_large_record(8000, 1722581155826);

        buffer
            .write_record(&large_record, TestMetadata { value: 1 })
            .unwrap();
        buffer
            .write_record(&small_record, TestMetadata { value: 2 })
            .unwrap();

        let result = buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(result.0, small_record);
        assert_eq!(result.1.value, 2);
    }

    #[test]
    fn test_instantiation_with_existing_records() {
        let mut buffer1 = get_test_ring_buffer(None);
        let record1 = create_test_record("first", 1722581155825);
        let record2 = create_test_record("second", 1722581155826);

        buffer1
            .write_record(&record1, TestMetadata { value: 1 })
            .unwrap();
        buffer1
            .write_record(&record2, TestMetadata { value: 2 })
            .unwrap();

        let mut buffer2 = get_test_ring_buffer(Some(buffer1.storage()));
        let result = buffer2.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0, record2);
        assert_eq!(result.1.value, 2);
    }

    #[test]
    fn test_complex_instantiation_with_multiple_record_types() {
        let record1 = create_test_record("small", 1722581155825);
        let record2 = create_test_record("medium", 1722581155826);
        let record3 = create_large_record(9000, 1722581155827);
        let record4 = create_large_record(8000, 1722581155828);

        let mut buffer = get_test_ring_buffer(None);
        buffer
            .write_record(&record1, TestMetadata { value: 1 })
            .unwrap();

        let mut buffer2 = get_test_ring_buffer(Some(buffer.storage()));
        assert_eq!(buffer2.read_latest_record().unwrap().unwrap().0, record1);
        buffer2
            .write_record(&record2, TestMetadata { value: 2 })
            .unwrap();

        let mut buffer3 = get_test_ring_buffer(Some(buffer2.storage()));
        assert_eq!(buffer3.read_latest_record().unwrap().unwrap().0, record2);
        buffer3
            .write_record(&record3, TestMetadata { value: 3 })
            .unwrap();

        let mut buffer4 = get_test_ring_buffer(Some(buffer3.storage()));
        assert_eq!(buffer4.read_latest_record().unwrap().unwrap().0, record3);
        buffer4
            .write_record(&record4, TestMetadata { value: 4 })
            .unwrap();

        let mut buffer5 = get_test_ring_buffer(Some(buffer4.storage()));
        let result = buffer5.read_latest_record().unwrap().unwrap();
        assert_eq!(result.0, record4);
        assert_eq!(result.1.value, 4);
    }

    // =============================================================================
    // EMPTY METADATA TESTS
    // =============================================================================

    #[test]
    fn test_empty_metadata_functionality() {
        let mut buffer = get_simple_ring_buffer(None);
        let record = create_test_record("empty_meta", 1722581155825);

        buffer.write_record(&record, EmptyMetadata).unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0, record);
    }

    // =============================================================================
    // ITERATION TESTS
    // =============================================================================

    #[test]
    fn test_forward_iteration_empty_buffer() {
        let mut buffer = get_test_ring_buffer(None);
        let mut iter = buffer.iter();
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_reverse_iteration_empty_buffer() {
        let mut buffer = get_test_ring_buffer(None);
        let mut iter = buffer.iter_reverse();
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_forward_iteration_single_record() {
        let mut buffer = get_test_ring_buffer(None);
        let record = create_test_record("single", 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 10 })
            .unwrap();

        let mut iter = buffer.iter();
        let result = iter.next().unwrap();
        assert_eq!(result.0, record);
        assert_eq!(result.1.value, 10);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_reverse_iteration_single_record() {
        let mut buffer = get_test_ring_buffer(None);
        let record = create_test_record("single", 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 10 })
            .unwrap();

        let mut iter = buffer.iter_reverse();
        let result = iter.next().unwrap();
        assert_eq!(result.0, record);
        assert_eq!(result.1.value, 10);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_forward_iteration_multiple_records() {
        let mut buffer = get_test_ring_buffer(None);
        let records = vec![
            (
                create_test_record("first", 1722581155825),
                TestMetadata { value: 1 },
            ),
            (
                create_test_record("second", 1722581155826),
                TestMetadata { value: 2 },
            ),
            (
                create_test_record("third", 1722581155827),
                TestMetadata { value: 3 },
            ),
        ];

        for (record, metadata) in &records {
            buffer.write_record(&record, *metadata).unwrap();
        }

        let mut iter = buffer.iter();
        let result1 = iter.next().unwrap();
        assert_eq!(result1.0.id, "third");
        assert_eq!(result1.1.value, 3);
    }

    #[test]
    fn test_reverse_iteration_multiple_records() {
        let mut buffer = get_test_ring_buffer(None);
        let records = vec![
            (
                create_test_record("first", 1722581155825),
                TestMetadata { value: 1 },
            ),
            (
                create_test_record("second", 1722581155826),
                TestMetadata { value: 2 },
            ),
            (
                create_test_record("third", 1722581155827),
                TestMetadata { value: 3 },
            ),
        ];

        for (record, metadata) in &records {
            buffer.write_record(&record, *metadata).unwrap();
        }

        let mut iter = buffer.iter_reverse();

        let result1 = iter.next().unwrap();
        assert_eq!(result1.0.id, "third");
        assert_eq!(result1.1.value, 3);

        let result2 = iter.next().unwrap();
        assert_eq!(result2.0.id, "second");
        assert_eq!(result2.1.value, 2);

        let result3 = iter.next().unwrap();
        assert_eq!(result3.0.id, "first");
        assert_eq!(result3.1.value, 1);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_iteration_with_large_records() {
        let mut buffer = get_test_ring_buffer(None);
        let record1 = create_large_record(5000, 1722581155825);
        let record2 = create_large_record(6000, 1722581155826);

        buffer
            .write_record(&record1, TestMetadata { value: 1 })
            .unwrap();
        buffer
            .write_record(&record2, TestMetadata { value: 2 })
            .unwrap();

        let mut iter = buffer.iter_reverse();
        let result1 = iter.next().unwrap();
        assert_eq!(result1.0, record2);
        assert_eq!(result1.1.value, 2);

        let result2 = iter.next().unwrap();
        assert_eq!(result2.0, record1);
        assert_eq!(result2.1.value, 1);

        assert!(iter.next().is_none());
    }

    // =============================================================================
    // FILTERED ITERATION TESTS
    // =============================================================================

    #[test]
    fn test_filtered_iteration_empty_buffer() {
        let mut buffer = get_test_ring_buffer(None);
        let mut iter = buffer.iter_filtered(|meta| meta.value > 100);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_filtered_iteration_no_matches() {
        let mut buffer = get_test_ring_buffer(None);
        let record = create_test_record("low_value", 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 50 })
            .unwrap();

        let mut iter = buffer.iter_filtered(|meta| meta.value > 100);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_filtered_iteration_by_value() {
        let mut buffer = get_test_ring_buffer(None);
        let records_and_metadata = vec![
            (
                create_test_record("low1", 1722581155825),
                TestMetadata { value: 10 },
            ),
            (
                create_test_record("high1", 1722581155826),
                TestMetadata { value: 150 },
            ),
            (
                create_test_record("low2", 1722581155827),
                TestMetadata { value: 20 },
            ),
            (
                create_test_record("high2", 1722581155828),
                TestMetadata { value: 200 },
            ),
            (
                create_test_record("mid", 1722581155829),
                TestMetadata { value: 100 },
            ),
        ];

        for (record, metadata) in &records_and_metadata {
            buffer.write_record(&record, *metadata).unwrap();
        }

        let high_values: Vec<_> = buffer.iter_filtered(|meta| meta.value > 100).collect();

        assert_eq!(high_values.len(), 2);
        assert_eq!(high_values[0].0.id, "high2");
        assert_eq!(high_values[1].0.id, "high1");
    }

    #[test]
    fn test_filtered_iteration_with_wrap_around() {
        let mut buffer = get_test_ring_buffer(None);

        let large_record1 = create_large_record(8000, 1722581155825);
        let large_record2 = create_large_record(8000, 1722581155826);
        let small_record = create_test_record("small", 1722581155827);

        buffer
            .write_record(&large_record1, TestMetadata { value: 100 })
            .unwrap();
        buffer
            .write_record(&large_record2, TestMetadata { value: 50 })
            .unwrap();
        buffer
            .write_record(&small_record, TestMetadata { value: 100 })
            .unwrap();

        let filtered: Vec<_> = buffer.iter_filtered(|meta| meta.value == 100).collect();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].0.id, "small");
        assert_eq!(filtered[1].0, large_record1);
    }

    // =============================================================================
    // STRESS TESTS
    // =============================================================================

    #[test]
    fn test_many_small_records() {
        let mut buffer = get_test_ring_buffer(None);
        let record_count = 100;

        for i in 0..record_count {
            let record = create_test_record(&format!("record_{}", i), 1722581155825 + i);
            let metadata = TestMetadata {
                value: if i % 4 == 0 { 200 } else { 50 },
            };
            buffer.write_record(&record, metadata).unwrap();
        }

        let result = buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(result.0.id, format!("record_{}", record_count - 1));

        let high_count = buffer.iter_filtered(|meta| meta.value == 200).count();
        assert!(high_count > 0, "Should have some high-value records");
        assert!(
            high_count <= 25,
            "Should not exceed total high-value records written"
        );
    }

    #[test]
    fn test_alternating_record_sizes() {
        let mut buffer = get_test_ring_buffer(None);
        let mut written_records = Vec::new();

        for i in 0..20 {
            let record = if i % 2 == 0 {
                create_test_record(&format!("small_{}", i), 1722581155825 + i)
            } else {
                create_large_record(3000, 1722581155825 + i)
            };
            buffer
                .write_record(&record, TestMetadata { value: i as u8 })
                .unwrap();
            written_records.push(record);
        }

        let all_records: Vec<_> = buffer.iter_reverse().collect();

        assert!(all_records.len() > 0, "Should have at least some records");
        assert!(
            all_records.len() <= 20,
            "Should not have more records than written"
        );

        let num_survived = all_records.len();
        let start_index = 20 - num_survived;

        for (i, (record, _)) in all_records.iter().enumerate() {
            let expected_written_index = (20 - 1) - i;

            if expected_written_index >= start_index {
                let expected_record = &written_records[expected_written_index];
                assert_eq!(record.id, expected_record.id);

                if expected_written_index % 2 == 0 {
                    assert!(record.id.starts_with("small_"));
                } else {
                    assert_eq!(record.id.len(), 3000);
                }
            }
        }

        let latest = buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(latest.0.id.len(), 3000);
    }

    #[test]
    fn test_buffer_persistence_across_iterations() {
        let mut buffer = get_test_ring_buffer(None);

        let records_and_metadata = vec![
            (
                create_test_record("first", 1722581155825),
                TestMetadata { value: 1 },
            ),
            (
                create_test_record("second", 1722581155826),
                TestMetadata { value: 100 },
            ),
            (
                create_test_record("third", 1722581155827),
                TestMetadata { value: 3 },
            ),
        ];

        for (record, metadata) in &records_and_metadata {
            buffer.write_record(&record, *metadata).unwrap();
        }

        let first_pass: Vec<_> = buffer.iter_reverse().collect();
        let second_pass: Vec<_> = buffer.iter_reverse().collect();

        assert_eq!(first_pass.len(), 3);
        assert_eq!(second_pass.len(), 3);
        assert_eq!(first_pass, second_pass);

        let filtered1: Vec<_> = buffer.iter_filtered(|meta| meta.value == 100).collect();
        let filtered2: Vec<_> = buffer.iter_filtered(|meta| meta.value == 100).collect();

        assert_eq!(filtered1, filtered2);
        assert_eq!(filtered1.len(), 1);
        assert_eq!(filtered1[0].0.id, "second");
    }

    // =============================================================================
    // EDGE CASE TESTS
    // =============================================================================

    #[test]
    fn test_record_with_special_characters() {
        let mut buffer = get_test_ring_buffer(None);
        let record = TestTimestampedRecord {
            ts: DateTime::<Utc>::from_timestamp_millis(1722581155825).expect("Invalid timestamp"),
            id: "Special chars: 你好 🦀 \n\t\r\"\\".to_string(),
        };

        buffer
            .write_record(&record, TestMetadata { value: 1 })
            .unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0, record);
    }

    #[test]
    fn test_boundary_conditions() {
        let mut buffer = get_test_ring_buffer(None);

        let partition_data_size = PARTITION_SIZE as usize - PAGE_METADATA_SIZE as usize;
        let record_overhead = 4 + 4 + TestMetadata::serialized_size();
        let target_json_size = partition_data_size - record_overhead - 100;

        let record = create_large_record(target_json_size, 1722581155825);

        buffer
            .write_record(&record, TestMetadata { value: 1 })
            .unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0.id.len(), target_json_size);
    }

    #[test]
    fn test_comprehensive_wrap_around_with_storage_persistence() {
        let mut buffer = get_test_ring_buffer(None);

        let almost_full_record = create_large_record(10000, 1722581155825);
        buffer
            .write_record(&almost_full_record, TestMetadata { value: 0 })
            .unwrap();

        let mut expected_records = Vec::new();
        let mut expected_values = Vec::new();

        for i in 0..6 {
            let record = create_large_record(1500, 1722581155826 + i);
            let value = if i % 2 == 0 { 50 } else { 150 };
            buffer
                .write_record(&record, TestMetadata { value })
                .unwrap();
            expected_records.push(record);
            expected_values.push(value);
        }

        let collected_records: Vec<_> = buffer.iter_reverse().collect();

        let num_collected = collected_records.len();
        assert!(
            num_collected > 0,
            "Should have at least some records after wrap-around"
        );
        assert!(
            num_collected <= 6,
            "Should not have more records than we wrote"
        );

        for (i, (record, metadata)) in collected_records.iter().enumerate() {
            let expected_index = (expected_records.len() - 1) - i;
            if expected_index < expected_records.len() {
                assert_eq!(record.id, expected_records[expected_index].id);
                assert_eq!(metadata.value, expected_values[expected_index]);
            }
        }

        let latest = buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(latest.0.id.len(), 1500);
        assert_eq!(latest.1.value, 150);

        let storage = buffer.storage();
        let mut new_buffer = get_test_ring_buffer(Some(storage));

        let new_collected_records: Vec<_> = new_buffer.iter_reverse().collect();

        assert_eq!(new_collected_records.len(), num_collected);

        for (i, ((old_record, old_meta), (new_record, new_meta))) in collected_records
            .iter()
            .zip(new_collected_records.iter())
            .enumerate()
        {
            assert_eq!(old_record.id, new_record.id, "Record {} should match", i);
            assert_eq!(
                old_meta.value, new_meta.value,
                "Metadata {} should match",
                i
            );
        }

        let new_latest = new_buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(new_latest.0.id.len(), 1500);
        assert_eq!(new_latest.1.value, 150);

        let final_record = create_test_record("post_reload_record", 1722581155831);
        new_buffer
            .write_record(&final_record, TestMetadata { value: 99 })
            .unwrap();

        let final_latest = new_buffer.read_latest_record().unwrap().unwrap();
        assert_eq!(final_latest.0.id, "post_reload_record");
        assert_eq!(final_latest.1.value, 99);

        let final_collected: Vec<_> = new_buffer.iter_reverse().collect();
        assert!(final_collected.len() > num_collected);
        assert_eq!(final_collected[0].0.id, "post_reload_record");
    }
}
