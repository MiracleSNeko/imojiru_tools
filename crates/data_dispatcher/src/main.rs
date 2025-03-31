use anyhow::Result as AnyResult;
use clap::{Parser, Subcommand, ValueEnum};
use encoding_rs::SHIFT_JIS;
use ron::ser::{PrettyConfig, to_string_pretty};
use serde::Serialize;
use std::{
    fs::File,
    io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write},
};
#[allow(unused_imports)]
use utils::IntoAnyResult;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct ConsoleArgs {
    // input file path
    #[arg(short, long)]
    input: String,

    // output file path
    #[arg(short, long)]
    output: String,

    // patch type
    #[arg(short, long)]
    patch_type: DataDispatcherType,
}

// 0                   1
// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |     HEADER    |  CNT  |  UNK  |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |                               |
// +            PAYLOAD            +
// |                               |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
pub trait DataDispatcherHeader: DeserializePatch {
    const MAGIC_HEADER: &[u8];
}

/// deserialize trait for reading data from binary file.
pub trait DeserializePatch {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self>
    where
        Self: Sized;
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
pub enum DataDispatcher {
    StringTable(StringTable),
    NameTable(NameTable),
    FileNameTable(FileNameTable),
}

#[derive(Debug, Clone, Copy, Subcommand, ValueEnum)]
enum DataDispatcherType {
    StringTable,
    NameTable,
    FileNameTable,
}

impl DeserializePatch for DataDispatcher {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        Ok(match self {
            DataDispatcher::StringTable(string_table) => {
                Self::StringTable(string_table.deserialize_patch(cursor)?)
            }
            DataDispatcher::NameTable(name_table) => {
                Self::NameTable(name_table.deserialize_patch(cursor)?)
            }
            DataDispatcher::FileNameTable(fname_table) => {
                Self::FileNameTable(fname_table.deserialize_patch(cursor)?)
            }
        })
    }
}

/// item in string table patch:
///
/// ```{text}
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   ID  |LEN|                   |
/// +-+-+-+-+-+-+                   +
/// |              DATA             |
/// +                         +-+-+-+
/// |                         | PAD |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// 0-3: id, u32 little-endian;
/// 4-5: length, u16 little-endian;
/// 6-: data, string (length bytes, padding to 2 bytes alignment);
/// ```
///
/// ## Note
/// the actual content of the string needs to be obtained by bitwise negation.
/// the end of the string can be identified by the following characteristics：
/// - string ends with a LF (\n): 0xF5 0xFF
/// - string ends with a null (\0): 0xFF 0xFF
#[derive(Debug, Default, Serialize, PartialEq, Eq, Clone)]
struct StringTableItem {
    id: u32,
    length: u16,
    data: String,
}

impl DeserializePatch for StringTableItem {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        let mut string_table_item = Self::default();

        // id: u32, little-endian <4 bytes>
        let mut id_bytes = [0; 4];
        cursor.read_exact(&mut id_bytes)?;
        string_table_item.id = u32::from_le_bytes(id_bytes);

        // length: u16, little-endian <2 byte>
        let mut length_bytes = [0; 2];
        cursor.read_exact(&mut length_bytes)?;
        string_table_item.length = u16::from_le_bytes(length_bytes);

        // data: string (length bytes, padding to 4 bytes alignment)
        //
        // NOTE:
        // the actual content of the string needs to be obtained by bitwise negation.
        // and the padding can be ignored by [String::from_utf8] automatically.
        let mut raw_data = vec![0; string_table_item.length as usize];
        cursor.read_exact(&mut raw_data)?;
        raw_data.iter_mut().for_each(|byte| *byte = !*byte);
        let (string, _, _) = SHIFT_JIS.decode(&raw_data);
        string_table_item.data = string.to_string();

        Ok(string_table_item)
    }
}

/// structure of string table patch:
///
/// ```{text}
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |[|S|T|R|T|B|L|]|  CNT  |  UNK  |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   ID  |   L   |   U   |       |
/// +-+-+-+-+-+-+-+-+-+-+-+-+       +
/// |                               |
/// +              DATA             +
/// |                               |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// 0-7: header, `[STRTBL]`;
/// 8-11: item_count, u32 little-endian;
/// 12-15: unknown (assume as magic number), u32 little-endian;
/// 16-: item, [StringTableItem];
/// ```
#[derive(Debug, Default, Serialize, PartialEq, Eq, Clone)]
pub struct StringTable {
    item_count: u32,
    assume_magic_number: u32,
    items: Vec<StringTableItem>,
}

impl DataDispatcherHeader for StringTable {
    const MAGIC_HEADER: &[u8] = b"[STRTBL]";
}

impl DeserializePatch for StringTable {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        // header: string <8 bytes>
        cursor.seek(SeekFrom::Current(8))?;

        // item_count: u32, little-endian <4 bytes>
        let mut item_count_bytes = [0; 4];
        cursor.read_exact(&mut item_count_bytes)?;
        let item_count = u32::from_le_bytes(item_count_bytes);

        // unknown (assume as magic number): u32, little-endian <4 bytes>
        let mut assume_magic_number_bytes = [0; 4];
        cursor.read_exact(&mut assume_magic_number_bytes)?;
        let assume_magic_number = u32::from_le_bytes(assume_magic_number_bytes);

        // item: [StringTableItem]
        let mut items = vec![];
        for _ in 0..item_count {
            items.push(StringTableItem::default().deserialize_patch(cursor)?);
        }

        Ok(Self {
            item_count,
            assume_magic_number,
            items,
        })
    }
}

/// item in name table patch:
///
/// ```{text}
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |LEN|            DATA           |
/// +-+-+                     +-+-+-+
/// |                         | PAD |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// 0-1: length, u16 little-endian;
/// 2-: data, string (length bytes, padding to 2 bytes alignment);
/// ```
#[derive(Debug, Default, Serialize, PartialEq, Eq, Clone)]
struct NameTableItem {
    length: u16,
    data: String,
}

impl DeserializePatch for NameTableItem {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        let mut name_table_item = Self::default();

        // length: u16, little-endian <2 byte>
        let mut length_bytes = [0; 2];
        cursor.read_exact(&mut length_bytes)?;
        name_table_item.length = u16::from_le_bytes(length_bytes);

        // data: string (length bytes, padding to 4 bytes alignment)
        //
        // NOTE:
        // the padding can be ignored by [String::from_utf8] automatically.
        let mut raw_data = vec![0; name_table_item.length as usize];
        cursor.read_exact(&mut raw_data)?;
        let (string, _, _) = SHIFT_JIS.decode(&raw_data);
        name_table_item.data = string.to_string();

        Ok(name_table_item)
    }
}

/// structure of name table patch:
///
/// ```{text}
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |[|M|E|S|N|A|M|]|UNK|CNT|LEN|   |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |    DATA   |LEN|      DATA     |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// 0-7: header, `[MESNAM]`;
/// 8-9: unknown (assume as padding), u16 little-endian;
/// 10-11: item_count, u16 little-endian;
/// 12-: item, [NameTableItem];
/// ```
#[derive(Debug, Default, Serialize, PartialEq, Eq, Clone)]
pub struct NameTable {
    assume_padding: u16,
    item_count: u16,
    items: Vec<NameTableItem>,
}

impl DataDispatcherHeader for NameTable {
    const MAGIC_HEADER: &[u8] = b"[MESNAM]";
}

impl DeserializePatch for NameTable {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        // header: string <8 bytes>
        cursor.seek(SeekFrom::Current(8))?;

        // unknown (assume as padding): u16, little-endian <2 bytes>
        let mut assume_padding_bytes = [0; 2];
        cursor.read_exact(&mut assume_padding_bytes)?;
        let assume_padding = u16::from_le_bytes(assume_padding_bytes);

        // item_count: u16, little-endian <2 bytes>
        let mut item_count_bytes = [0; 2];
        cursor.read_exact(&mut item_count_bytes)?;
        let item_count = u16::from_le_bytes(item_count_bytes);

        // item: [NameTableItem]
        let mut items = vec![];
        for _ in 0..item_count {
            items.push(NameTableItem::default().deserialize_patch(cursor)?);
        }

        Ok(Self {
            assume_padding,
            item_count,
            items,
        })
    }
}

///
#[derive(Debug, Default, Serialize, PartialEq, Eq, Clone)]
struct FileNameTableItem {
    length: u16,
    data: String,
}

impl DeserializePatch for FileNameTableItem {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        let mut file_name_table_item = Self::default();

        // length: u16, little-endian <2 byte>
        let mut length_bytes = [0; 2];
        cursor.read_exact(&mut length_bytes)?;
        file_name_table_item.length = u16::from_le_bytes(length_bytes);

        // data: string (length bytes, padding to 4 bytes alignment)
        //
        // NOTE:
        // the actual content of the string needs to be obtained by bitwise negation.
        // and the padding can be ignored by [String::from_utf8] automatically.
        let mut raw_data = vec![0; file_name_table_item.length as usize];
        cursor.read_exact(&mut raw_data)?;
        raw_data.iter_mut().for_each(|byte| *byte = !*byte);
        let (string, _, _) = SHIFT_JIS.decode(&raw_data);
        file_name_table_item.data = string.to_string();

        Ok(file_name_table_item)
    }
}

///
#[derive(Debug, Default, Serialize, PartialEq, Eq, Clone)]
pub struct FileNameTable {
    item_count: u32,
    assume_magic_number: u32,
    items: Vec<FileNameTableItem>,
}

impl DataDispatcherHeader for FileNameTable {
    const MAGIC_HEADER: &[u8] = b"[F-NAME]";
}

impl DeserializePatch for FileNameTable {
    fn deserialize_patch(&self, cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        // header: string <8 bytes>
        cursor.seek(SeekFrom::Current(8))?;

        // item_count: u32, little-endian <4 bytes>
        let mut item_count_bytes = [0; 4];
        cursor.read_exact(&mut item_count_bytes)?;
        let item_count = u32::from_le_bytes(item_count_bytes);

        // unknown (assume as magic number): u32, little-endian <4 bytes>
        let mut assume_magic_number_bytes = [0; 4];
        cursor.read_exact(&mut assume_magic_number_bytes)?;
        let assume_magic_number = u32::from_le_bytes(assume_magic_number_bytes);

        // item: [FileNameTableItem]
        let mut items = vec![];
        for _ in 0..item_count {
            items.push(FileNameTableItem::default().deserialize_patch(cursor)?);
        }

        Ok(Self {
            item_count,
            assume_magic_number,
            items,
        })
    }
}

fn main() -> AnyResult<()> {
    let args = ConsoleArgs::parse();

    let file = File::open(&args.input)?;
    let mut buffer = vec![0; file.metadata()?.len() as usize];
    let mut reader = BufReader::new(file);

    reader.read_exact(&mut buffer)?;

    let mut writer = BufWriter::new(File::create(args.output)?);

    let (data_patcher, header) = match args.patch_type {
        DataDispatcherType::StringTable => (
            DataDispatcher::StringTable(StringTable::default()),
            StringTable::MAGIC_HEADER,
        ),
        DataDispatcherType::NameTable => (
            DataDispatcher::NameTable(NameTable::default()),
            NameTable::MAGIC_HEADER,
        ),
        DataDispatcherType::FileNameTable => (
            DataDispatcher::FileNameTable(FileNameTable::default()),
            FileNameTable::MAGIC_HEADER,
        ),
    };

    let start_pos = buffer
        .windows(header.len())
        .position(|window| window == header)
        .expect(&format!("header `{}` not found", String::from_utf8_lossy(header)));
    let mut cursor = Cursor::new(buffer);
    cursor.seek(SeekFrom::Start(start_pos as u64))?;

    let data = data_patcher.deserialize_patch(&mut cursor)?;

    writer.write_all(to_string_pretty(&data, PrettyConfig::default())?.as_bytes())?;

    Ok(())
}
