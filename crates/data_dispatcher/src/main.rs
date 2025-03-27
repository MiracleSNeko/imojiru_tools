use anyhow::Result as AnyResult;
use std::io::{self, BufRead, BufReader, Cursor, Seek, SeekFrom};
use utils::IntoAnyResult;

/// file head part of .arc patch:
///
/// header: <str, 8 bytes>
/// assume_item_count: <u32 LE, 4 bytes>
/// assume_magic_number: <u32 LE, 4 bytes>
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
struct MagicHeader {
    header: [u8; 8],
    assume_item_count: u32,
    assume_magic_number: u32,
}

impl MagicHeader {
    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> AnyResult<Self> {
        todo!()
    }
}

fn main() -> AnyResult<()> {
    Ok(())
}
