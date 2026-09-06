// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Crash-safe local Write-Ahead Log (WAL) buffer.
//!
//! Append-only, segmented files of fixed-size binary records. Each record is
//! `[magic u32][payload_len u32][crc32 u32][payload = postcard(SampleBatch)]`.
//! The CRC plus the magic byte lets recovery distinguish a clean EOF from a
//! torn or corrupt tail and resume at the last valid sequence.

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use daqcore_core::{Error, Result, SampleBatch, Seq};

/// Record magic: "DQAC".
const MAGIC: u32 = 0x4451_4143;
/// magic (4) + payload_len (4) + crc32 (4).
const HEADER_LEN: usize = 12;

/// When to flush the write buffer to stable storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// `fsync` (data + metadata) after every append.
    SyncAll,
    /// `fdatasync` (data) after every append.
    SyncData,
    /// `fdatasync` at most once per interval (milliseconds).
    Interval(u64),
    /// Rely on the OS page cache; fastest, least durable.
    None,
}

/// WAL storage configuration.
#[derive(Debug, Clone)]
pub struct WalConfig {
    /// Directory holding segment files.
    pub dir: PathBuf,
    /// Maximum bytes per segment before rolling to the next file.
    pub segment_size: u64,
    /// Durability policy.
    pub durability: Durability,
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("wal"),
            segment_size: 64 * 1024 * 1024,
            durability: Durability::SyncData,
        }
    }
}

/// An append-only, crash-safe buffer of `SampleBatch`es.
pub struct Wal {
    cfg: WalConfig,
    segment_index: u64,
    writer: BufWriter<File>,
    segment_len: u64,
    written_seq: Seq,
    acknowledged_seq: Seq,
    last_fsync: Instant,
}

impl Wal {
    /// Open (or create) the WAL, recovering the cursor from existing segments.
    pub fn open(cfg: WalConfig) -> Result<Self> {
        fs::create_dir_all(&cfg.dir)?;
        let (segment_index, written_seq, segment_len) = Self::recover(&cfg)?;
        let writer = Self::open_segment(&cfg, segment_index)?;
        Ok(Wal {
            cfg,
            segment_index,
            writer,
            segment_len,
            written_seq,
            acknowledged_seq: 0,
            last_fsync: Instant::now(),
        })
    }

    /// Append a batch. Returns the assigned sequence number.
    ///
    /// The batch's `seq` must be strictly greater than the previous batch's.
    pub fn append(&mut self, batch: &SampleBatch) -> Result<Seq> {
        // Enforce strict monotonicity so recovery never double-plays a batch.
        if batch.seq <= self.written_seq {
            return Err(Error::Wal(format!(
                "non-monotonic seq {} after {}",
                batch.seq, self.written_seq
            )));
        }

        // Serialize the batch, then wrap it in a [magic][len][crc] header.
        let payload = postcard::to_allocvec(batch).map_err(|e| Error::Encode(e.to_string()))?;
        let record_len = (HEADER_LEN + payload.len()) as u64;

        // Roll to a new segment if this record would overflow the current one.
        if self.segment_len > 0 && self.segment_len + record_len > self.cfg.segment_size {
            self.roll()?;
        }

        let mut header = [0u8; HEADER_LEN];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4..8].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[8..12].copy_from_slice(&crc32fast::hash(&payload).to_le_bytes());

        self.writer.write_all(&header)?;
        self.writer.write_all(&payload)?;
        self.segment_len += record_len;
        self.written_seq = batch.seq;
        self.maybe_sync()?;
        Ok(self.written_seq)
    }

    /// Mark `seq` (and everything before it) as acknowledged by the consumer.
    ///
    /// Note: the acknowledgement cursor is in-memory only in this milestone;
    /// persisting it is a later enhancement.
    pub fn acknowledge(&mut self, seq: Seq) {
        if seq > self.acknowledged_seq {
            self.acknowledged_seq = seq;
        }
    }

    pub fn written_seq(&self) -> Seq {
        self.written_seq
    }

    pub fn acknowledged_seq(&self) -> Seq {
        self.acknowledged_seq
    }

    pub fn segment_index(&self) -> u64 {
        self.segment_index
    }

    pub fn segment_len(&self) -> u64 {
        self.segment_len
    }

    /// Flush the write buffer to the OS.
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    /// Flush buffered data and force it to stable storage.
    pub fn sync(&mut self) -> Result<()> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        Ok(())
    }

    fn maybe_sync(&mut self) -> Result<()> {
        match self.cfg.durability {
            Durability::SyncAll => {
                self.writer.flush()?;
                self.writer.get_ref().sync_all()?;
            }
            Durability::SyncData => {
                self.writer.flush()?;
                self.writer.get_ref().sync_data()?;
            }
            Durability::Interval(ms) => {
                if self.last_fsync.elapsed() >= Duration::from_millis(ms) {
                    self.writer.flush()?;
                    self.writer.get_ref().sync_data()?;
                    self.last_fsync = Instant::now();
                }
            }
            Durability::None => {}
        }
        Ok(())
    }

    fn roll(&mut self) -> Result<()> {
        self.writer.flush()?;
        self.segment_index += 1;
        self.writer = Self::open_segment(&self.cfg, self.segment_index)?;
        self.segment_len = 0;
        Ok(())
    }

    fn segment_path(dir: &Path, index: u64) -> PathBuf {
        dir.join(format!("seg-{index:020}.wal"))
    }

    fn open_segment(cfg: &WalConfig, index: u64) -> Result<BufWriter<File>> {
        let path = Self::segment_path(&cfg.dir, index);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;
        Ok(BufWriter::new(file))
    }

    /// Scan segments, truncating a torn/corrupt tail and returning
    /// `(segment_index, written_seq, segment_len)` to resume from.
    fn recover(cfg: &WalConfig) -> Result<(u64, Seq, u64)> {
        // Collect and sort existing segment indices (seg-000....wal).
        let mut indices: Vec<u64> = Vec::new();
        for entry in fs::read_dir(&cfg.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(idx) = name
                .strip_prefix("seg-")
                .and_then(|s| s.strip_suffix(".wal"))
            {
                if let Ok(i) = idx.parse::<u64>() {
                    indices.push(i);
                }
            }
        }
        indices.sort_unstable();

        let mut last_index = 0u64;
        let mut written_seq = 0u64;
        let mut last_len = 0u64;

        // Walk each segment record-by-record; a clean EOF sets last_len,
        // a bad/torn record truncates the segment to the last good offset.
        for idx in indices {
            last_index = idx;
            let path = Self::segment_path(&cfg.dir, idx);
            let mut reader = BufReader::new(File::open(&path)?);
            let mut offset = 0u64;
            let mut last_good_offset = 0u64;
            let mut clean = false;

            loop {
                match Self::read_record(&mut reader) {
                    Ok(Some((batch, consumed))) => {
                        offset += consumed;
                        last_good_offset = offset;
                        written_seq = batch.seq;
                    }
                    Ok(None) => {
                        last_len = offset;
                        clean = true;
                        break;
                    }
                    Err(_) => {
                        last_len = last_good_offset;
                        break;
                    }
                }
            }

            if !clean {
                let file = OpenOptions::new().write(true).open(&path)?;
                file.set_len(last_good_offset)?;
            }
        }

        Ok((last_index, written_seq, last_len))
    }

    /// Read one record, returning `(batch, bytes_consumed)`, or `None` at a clean EOF.
    fn read_record(reader: &mut impl Read) -> Result<Option<(SampleBatch, u64)>> {
        // Header first; a short read here means clean EOF or a torn tail.
        let mut header = [0u8; HEADER_LEN];
        let filled = read_exact_or_eof(reader, &mut header)?;
        if filled == 0 {
            return Ok(None);
        }
        if filled < HEADER_LEN {
            return Err(Error::Wal("truncated record header".into()));
        }

        // Validate magic, then read the payload and check its CRC.
        let magic = u32::from_le_bytes(header[0..4].try_into().unwrap());
        if magic != MAGIC {
            return Err(Error::Wal("bad record magic".into()));
        }
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(header[8..12].try_into().unwrap());

        let mut payload = vec![0u8; len];
        reader
            .read_exact(&mut payload)
            .map_err(|_| Error::Wal("truncated record payload".into()))?;

        if crc32fast::hash(&payload) != crc {
            return Err(Error::Wal("record crc mismatch".into()));
        }

        let batch: SampleBatch =
            postcard::from_bytes(&payload).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(Some((batch, (HEADER_LEN + len) as u64)))
    }
}

impl Drop for Wal {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}

fn read_exact_or_eof(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daqcore_core::{Sample, Value};

    fn batch(seq: Seq, n: usize) -> SampleBatch {
        let mut b = SampleBatch::new(seq);
        for i in 0..n {
            b.push(Sample::new(
                i as u64,
                format!("ch{}", i % 3),
                Value::F64(i as f64),
            ));
        }
        b
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("daqcore-wal-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn cfg(dir: PathBuf, segment_size: u64) -> WalConfig {
        WalConfig {
            dir,
            segment_size,
            durability: Durability::None,
        }
    }

    #[test]
    fn append_and_reopen() {
        let dir = tmpdir("append");
        {
            let mut w = Wal::open(cfg(dir.clone(), 4096)).unwrap();
            for s in 1..=10 {
                w.append(&batch(s, 4)).unwrap();
            }
            w.flush().unwrap();
        }

        let w = Wal::open(cfg(dir.clone(), 4096)).unwrap();
        assert_eq!(w.written_seq(), 10);

        let mut reader = BufReader::new(File::open(Wal::segment_path(&dir, 0)).unwrap());
        let mut seqs = Vec::new();
        while let Ok(Some((b, _))) = Wal::read_record(&mut reader) {
            seqs.push(b.seq);
        }
        assert_eq!(seqs, (1..=10).collect::<Vec<_>>());
    }

    #[test]
    fn non_monotonic_seq_is_rejected() {
        let dir = tmpdir("mono");
        let mut w = Wal::open(cfg(dir, 4096)).unwrap();
        w.append(&batch(5, 1)).unwrap();
        assert!(w.append(&batch(5, 1)).is_err());
        assert!(w.append(&batch(4, 1)).is_err());
    }

    #[test]
    fn corrupt_tail_is_truncated() {
        let dir = tmpdir("corrupt");
        {
            let mut w = Wal::open(cfg(dir.clone(), 4096)).unwrap();
            for s in 1..=5 {
                w.append(&batch(s, 4)).unwrap();
            }
            w.sync().unwrap();
        }

        // Corrupt the final record by flipping a byte in its payload.
        {
            let path = Wal::segment_path(&dir, 0);
            let mut bytes = fs::read(&path).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 0xff;
            fs::write(&path, &bytes).unwrap();
        }

        let w = Wal::open(cfg(dir.clone(), 4096)).unwrap();
        assert_eq!(w.written_seq(), 4);

        let mut reader = BufReader::new(File::open(Wal::segment_path(&dir, 0)).unwrap());
        let mut seqs = Vec::new();
        while let Ok(Some((b, _))) = Wal::read_record(&mut reader) {
            seqs.push(b.seq);
        }
        assert_eq!(seqs, (1..=4).collect::<Vec<_>>());
    }

    #[test]
    fn rotation_creates_segments_and_recovers() {
        let dir = tmpdir("rotate");
        {
            let mut w = Wal::open(cfg(dir.clone(), 128)).unwrap();
            for s in 1..=50 {
                w.append(&batch(s, 4)).unwrap();
            }
            w.flush().unwrap();
        }
        assert!(Wal::segment_path(&dir, 0).exists());
        assert!(Wal::segment_path(&dir, 1).exists());

        let w = Wal::open(cfg(dir, 128)).unwrap();
        assert_eq!(w.written_seq(), 50);
    }
}
