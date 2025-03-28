use anyhow::Result as AnyResult;
use clap::Parser;
use encoding_rs::SHIFT_JIS;
use std::{
    fs::File,
    io::{BufReader, BufWriter, Cursor, Read, Write},
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
}

/// deserialize trait for reading data from binary file.
pub trait Deserialize {
    fn deserialize(cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self>
    where
        Self: Sized;
}

/// file head part of .arc patch:
///
/// ```{text}
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |     HEADER    |  CNT  |  UKN  |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// 0-7: header, string;
/// 8-11: item_count, u32 little-endian;
/// 12-15: unknown (assume as magic number), u32 little-endian;
/// ```
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
struct MagicHeader {
    header: [u8; 8],
    item_count: u32,
    assume_magic_number: u32,
}

impl Deserialize for MagicHeader {
    fn deserialize(cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        let mut magic_header = Self::default();

        // header: string <8 bytes>
        cursor.read_exact(&mut magic_header.header)?;

        // assume_item_count: u32, little-endian <4 bytes>
        let mut item_count_bytes = [0; 4];
        cursor.read_exact(&mut item_count_bytes)?;
        magic_header.item_count = u32::from_le_bytes(item_count_bytes);

        // assume_magic_number: u32, little-endian <4 bytes>
        let mut magic_number_bytes = [0; 4];
        cursor.read_exact(&mut magic_number_bytes)?;
        magic_header.assume_magic_number = u32::from_le_bytes(magic_number_bytes);

        Ok(magic_header)
    }
}

/// item in tblstr.arc patch:
///
/// ```{text}
/// 0                   1
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |   ID  |L|U|                   |
/// +-+-+-+-+-+-+                   +
/// |              DATA             |
/// +                         +-+-+-+
/// |                         | PAD |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///
/// 0-3: id, u32 little-endian;
/// 4: length, u8;
/// 5: unknown (assumed as length extension, always be 0x00 now), u8;
/// 6-: data, string (length bytes, padding to ? bytes alignment);
/// ```
///
/// ## Note
/// the actual content of the string needs to be obtained by bitwise negation.
/// the end of the string can be identified by the following characteristics：
/// - string ends with a LF (\n): 0xF5 0xFF
/// - string ends with a null (\0): 0xFF 0xFF
#[derive(Debug, Default, PartialEq, Eq, Clone)]
struct StringTableItem {
    id: u32,
    length: u8,
    assume_length_ext: u8,
    data: String,
}

impl Deserialize for StringTableItem {
    fn deserialize(cursor: &mut Cursor<Vec<u8>>) -> AnyResult<Self> {
        let mut string_table_item = Self::default();

        // id: u32, little-endian <4 bytes>
        let mut id_bytes = [0; 4];
        cursor.read_exact(&mut id_bytes)?;
        string_table_item.id = u32::from_le_bytes(id_bytes);

        // length: u8 <1 byte>
        let mut length_bytes = [0; 1];
        cursor.read_exact(&mut length_bytes)?;
        string_table_item.length = length_bytes[0];

        // assume_length_ext: u8 <1 byte>
        let mut length_ext_bytes = [0; 1];
        cursor.read_exact(&mut length_ext_bytes)?;
        string_table_item.assume_length_ext = length_ext_bytes[0];
        assert_eq!(string_table_item.assume_length_ext, 0x00);

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

fn main() -> AnyResult<()> {
    let args = ConsoleArgs::parse();

    let file = File::open(&args.input)?;
    let mut buffer = vec![0; file.metadata()?.len() as usize];
    let mut reader = BufReader::new(file);

    reader.read_exact(&mut buffer)?;

    let mut writer = BufWriter::new(File::create(args.output)?);

    let mut cursor = Cursor::new(buffer);
    let magic_header = MagicHeader::deserialize(&mut cursor)?;
    writeln!(writer, "{:#?}", magic_header)?;

    for _ in 0..magic_header.item_count {
        let string_table_item = StringTableItem::deserialize(&mut cursor)?;
        writeln!(writer, "{:#?}", string_table_item)?;
    }

    Ok(())
}
