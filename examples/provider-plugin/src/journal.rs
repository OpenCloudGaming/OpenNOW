use crate::State;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;

const MAX_RECORD: usize = 512 * 1024;
const MAX_JOURNAL: u64 = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    sequence: u64,
    state: State,
}

pub(crate) struct Journal {
    _lock: File,
    file: File,
    sequence: u64,
}

fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

impl Journal {
    pub(crate) fn read_state(directory: &Path) -> io::Result<State> {
        let file = File::open(directory.join("state.ndjson"))?;
        if file.metadata()?.len() > MAX_JOURNAL {
            return Err(io::Error::other("Demo journal is full"));
        }
        let mut reader = BufReader::new(file);
        let mut state = State::default();
        let mut sequence = 0;
        loop {
            let mut bytes = Vec::new();
            if read_record(&mut reader, &mut bytes)? == 0 || bytes.last() != Some(&b'\n') {
                return Ok(state);
            }
            let record: Record = serde_json::from_slice(&bytes)
                .map_err(|_| io::Error::other("Demo journal is invalid"))?;
            if record.version != 1 || record.sequence != sequence + 1 || !record.state.valid() {
                return Err(io::Error::other("Demo journal ownership is invalid"));
            }
            sequence = record.sequence;
            state = record.state;
        }
    }

    pub(crate) fn open(directory: &Path) -> io::Result<(Self, State)> {
        fs::create_dir_all(directory)?;
        let lock = private_file(&directory.join("control.lock"))?;
        lock.try_lock_exclusive()?;
        let mut file = private_file(&directory.join("state.ndjson"))?;
        if file.metadata()?.len() > MAX_JOURNAL {
            return Err(io::Error::other("Demo journal is full"));
        }
        let mut state = State::default();
        let mut sequence = 0;
        let mut accepted_bytes = 0;
        let mut reader = BufReader::new(file.try_clone()?);
        loop {
            let mut bytes = Vec::new();
            let count = read_record(&mut reader, &mut bytes)?;
            if count == 0 {
                break;
            }
            if bytes.last() != Some(&b'\n') {
                file.set_len(accepted_bytes)?;
                file.sync_all()?;
                break;
            }
            let record: Record = serde_json::from_slice(&bytes)
                .map_err(|_| io::Error::other("Demo journal is invalid"))?;
            if record.version != 1 || record.sequence != sequence + 1 || !record.state.valid() {
                return Err(io::Error::other("Demo journal ownership is invalid"));
            }
            sequence = record.sequence;
            state = record.state;
            accepted_bytes += count as u64;
        }
        file.seek(SeekFrom::End(0))?;
        Ok((
            Self {
                _lock: lock,
                file,
                sequence,
            },
            state,
        ))
    }

    pub(crate) fn append(&mut self, state: &State) -> io::Result<()> {
        if !state.valid() {
            return Err(io::Error::other("Demo state exceeds its bounds"));
        }
        let record = Record {
            version: 1,
            sequence: self.sequence + 1,
            state: state.clone(),
        };
        let mut bytes = serde_json::to_vec(&record)
            .map_err(|_| io::Error::other("Demo journal cannot be encoded"))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RECORD
            || self.file.metadata()?.len() + bytes.len() as u64 > MAX_JOURNAL
        {
            return Err(io::Error::other("Demo journal is full"));
        }
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.sequence += 1;
        Ok(())
    }
}

fn read_record(reader: &mut impl BufRead, output: &mut Vec<u8>) -> io::Result<usize> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(output.len());
        }
        let end = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if output.len() + end > MAX_RECORD {
            return Err(io::Error::other("Demo journal record is oversized"));
        }
        let complete = available[end - 1] == b'\n';
        output.extend_from_slice(&available[..end]);
        reader.consume(end);
        if complete {
            return Ok(output.len());
        }
    }
}
