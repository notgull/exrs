use ::smallvec::SmallVec;
use ::file::validity::*;

/// null-terminated text strings.
/// max 31 bytes long (if bit 10 is set to 0),
/// or max 255 bytes long (if bit 10 is set to 1).
// TODO non public fields?
/// must be at least 1 byte (to avoid confusion with null-terminators)
#[derive(Clone)]
pub struct Text {
    /// vector does not include null terminator
    pub bytes: SmallVec<[u8; 32]>,
}



#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: Text,

    /// kind can be inferred from value
    /// size in bytes can be inferred from value
    pub value: AttributeValue,
}


// TODO custom attribute
#[derive(Debug, Clone)]
pub enum AttributeValue {
    I32Box2(I32Box2),
    F32Box2(F32Box2),
    ChannelList(ChannelList),
    Chromaticities(Chromaticities),
    Compression(Compression),
    F64(f64),
    EnvironmentMap(EnvironmentMap),
    F32(f32),
    I32(i32),
    KeyCode(KeyCode),
    LineOrder(LineOrder),
    F32Matrix3x3([f32; 9]),
    F32Matrix4x4([f32; 16]),
    Preview(Preview),
    Rational(i32, u32),

    /// i32 of byte-length followed by u8 content
    Text(ParsedText),

    /// the number of strings can be inferred from the total attribute size
    TextVector(Vec<Text>),

    TileDescription(TileDescription),

    // TODO enable conversion to rust time
    TimeCode(u32, u32),

    I32Vec2(i32, i32),
    F32Vec2(f32, f32),
    I32Vec3(i32, i32, i32),
    F32Vec3(f32, f32, f32),
}


/// this enum parses strings to speed up comparisons
/// based on often-used string contents
#[derive(Debug, Clone)]
pub enum ParsedText {
    /// "scanlineimage"
    ScanLine,

    /// "tiledimage"
    Tile,

    /// "deepscanline"
    DeepScanLine,

    /// "deeptile"
    DeepTile,

    Arbitrary(Text),
}

pub mod kind {
    pub const SCAN_LINE: &'static [u8] = b"scanlineimage";
    pub const TILE: &'static [u8] = b"tiledimage";

    pub const DEEP_SCAN_LINE: &'static [u8] = b"deepscanline";
    pub const DEEP_TILE: &'static [u8] = b"deeptile";
}


pub use ::file::compress::Compression;

pub type DataWindow = I32Box2;
pub type DisplayWindow = I32Box2;

#[derive(Debug, Clone, Copy)]
pub struct I32Box2 {
    pub x_min: i32, pub y_min: i32,
    pub x_max: i32, pub y_max: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct F32Box2 {
    pub x_min: f32, pub y_min: f32,
    pub x_max: f32, pub y_max: f32,
}

/// followed by a null byte
/// sorted alphabetically?
pub type ChannelList = SmallVec<[Channel; 5]>;

#[derive(Debug, Clone)]
pub struct Channel {
    /// zero terminated, 1 to 255 bytes
    pub name: Text,

    /// int
    pub pixel_type: PixelType,

    pub is_linear: bool,

    /// three signed chars, should be zero
    pub reserved: [i8; 3],

    /// can be used for chroma-subsampling
    pub x_sampling: i32,

    /// can be used for chroma-subsampling
    pub y_sampling: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum PixelType {
    U32, F16, F32,
}

#[derive(Debug, Clone, Copy)]
pub struct Chromaticities {
    pub red_x: f32,     pub red_y: f32,
    pub green_x: f32,   pub green_y: f32,
    pub blue_x: f32,    pub blue_y: f32,
    pub white_x: f32,   pub white_y: f32
}

#[derive(Debug, Clone, Copy)]
pub enum EnvironmentMap {
    LatitudeLongitude,
    Cube,
}

/// uniquely identifies a motion picture film frame
#[derive(Debug, Clone, Copy)]
pub struct KeyCode {
    pub film_manufacturer_code: i32,
    pub film_type: i32,
    pub film_roll_prefix: i32,

    pub count: i32,

    pub perforation_offset: i32,
    pub perforations_per_frame: i32,
    pub perforations_per_count: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum LineOrder {
    IncreasingY,
    DecreasingY,
    RandomY,
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub width: u32,
    pub height: u32,

    /// 4 × width × height bytes,
    /// Scan lines are stored top to bottom; within a scan line pixels are stored from left
    /// to right. A pixel consists of four unsigned chars, R, G, B, A
    pub pixel_data: Vec<i8>,
}

#[derive(Debug, Clone, Copy)]
pub struct TileDescription {
    pub x_size: u32, pub y_size: u32,
    pub level_mode: LevelMode,
    pub rounding_mode: RoundingMode,
}

#[derive(Debug, Clone, Copy)]
pub enum LevelMode {
    One, MipMap, RipMap,
}

#[derive(Debug, Clone, Copy)]
pub enum RoundingMode {
    Down, Up,
}


use ::file::io::*;
use ::file::io;


impl Text {
    // TODO make sure this does not allocate, but uses the stack for string literals
    fn unchecked_from_str(str_value: &str) -> Self {
        Text { bytes: SmallVec::from_slice(str_value.as_bytes()) }
    }

    pub fn from_bytes(bytes: SmallVec<[u8; 32]>) -> Self {
        Text { bytes }
    }

    /// panics if value is too long (31 bytes max)
    pub fn short_from_str(str_value: &str) -> Self {
        assert!(str_value.as_bytes().len() < 32, "max text length is 31");
        Self::unchecked_from_str(str_value)
    }

    /// panics if value is too long (31 bytes max)
    pub fn long_from_str(str_value: &str) -> Self {
        assert!(str_value.as_bytes().len() < 256, "max text length is 255");
        Self::unchecked_from_str(str_value)
    }

    pub fn to_string(&self) -> String {
        self.bytes.iter()
            .map(|&byte| byte as char)
            .collect() // TODO is this ascii and can be treated as utf-8?
    }

    pub fn validate(&self, long_names: Option<bool>) -> Validity {
        Self::validate_bytes(self.bytes.as_slice(), long_names)
    }

    pub fn validate_bytes(text: &[u8], long_names: Option<bool>) -> Validity {
        let is_valid = !text.is_empty() && match long_names {
            Some(false) => text.len() < 32,
            Some(true) => text.len() < 256,
            None => true,
        };

        if is_valid { Ok(()) } else {
            if text.is_empty() {
                Err(Invalid::Content(Value::Text, Required::Min(1)).into())
            } else if long_names.unwrap() {
                Err(Invalid::Content(Value::Text, Required::Max(255)).into())
            } else {
                Err(Invalid::Content(Value::Text, Required::Max(31)).into())
            }
        }
    }


    pub fn write_i32_sized<W: Write>(&self, write: &mut W, long_names: Option<bool>) -> WriteResult {
        (self.bytes.len() as i32).write(write)?;
        Self::write_unsized_bytes(self.bytes.as_slice(), write, long_names)
    }

    pub fn write_unsized_bytes<W: Write>(bytes: &[u8], write: &mut W, long_names: Option<bool>) -> WriteResult {
        Text::validate_bytes(bytes, long_names)?;
        io::write_u8_array(write, bytes)
    }

    pub fn read_i32_sized<R: Read>(read: &mut R) -> ReadResult<Self> {
        let size = i32::read(read)? as usize;
        Text::read_sized(read, size)
    }

    pub fn read_sized<R: Read>(read: &mut R, size: usize) -> ReadResult<Self> {
        // TODO read into small vec without heap
        Ok(Text::from_bytes(SmallVec::from_vec(read_u8_vec(read, size, 1024)?)))
    }

    pub fn write_null_terminated<W: Write>(&self, write: &mut W, long_names: Option<bool>) -> WriteResult {
        Self::write_unsized_bytes(self.bytes.as_slice(), write, long_names)?;
        io::SequenceEnd::write(write)
    }

    pub fn write_null_terminated_bytes<W: Write>(bytes: &[u8], write: &mut W, long_names: Option<bool>) -> WriteResult {
        Text::write_unsized_bytes(bytes, write, long_names)?;
        io::SequenceEnd::write(write)
    }

    pub fn read_null_terminated<R: Read>(read: &mut R) -> ReadResult<Self> {
        let mut bytes = SmallVec::new();

        loop {
            match u8::read(read)? {
                0 => break,
                non_terminator => bytes.push(non_terminator),
            }
        }

        Ok(Text { bytes })
    }

    fn read_vec_of_i32_sized<R: Read + Seek>(
        read: &mut R, attribute_value_byte_size: u32
    ) -> ReadResult<Vec<Text>>
    {
        let mut result = Vec::with_capacity(2);

        // length of the text-vector can be inferred from attribute size
        let mut processed_bytes = 0;

        while processed_bytes < attribute_value_byte_size {
            let text = Text::read_i32_sized(read)?;
            processed_bytes += ::std::mem::size_of::<i32>() as u32; // size i32 of the text
            processed_bytes += text.bytes.len() as u32;
            result.push(text);
        }

        debug_assert_eq!(processed_bytes, attribute_value_byte_size, "text lengths did not match attribute size");
        Ok(result)
    }

    /// allows any text length since it is only used for attribute values,
    /// but not attribute names, attribute type names, or channel names
    fn write_vec_of_i32_sized_texts<W: Write>(write: &mut W, texts: &[Text]) -> WriteResult {
        // length of the text-vector can be inferred from attribute size
        for text in texts {
            text.write_i32_sized(write, None)?;
        }
        Ok(())
    }

}

impl ::std::fmt::Debug for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "\"{}\"", self.to_string())
    }
}


impl ParsedText {


    pub fn parse(text: Text) -> Self {
        match text.bytes.as_slice() {
            kind::SCAN_LINE => ParsedText::ScanLine,
            kind::TILE => ParsedText::Tile,
            kind::DEEP_SCAN_LINE => ParsedText::DeepScanLine,
            kind::DEEP_TILE => ParsedText::DeepTile,
            _ => ParsedText::Arbitrary(text),
        }
    }

    /// This function does not do any length checks!
    /// When writing a file, checks will be made that the length
    /// does not exceed 31 or 255,
    /// depending on if the 'long strings' version bit is set
    pub fn to_text_bytes(&self) -> &[u8] {
        match self {
            // TODO make these constants
            ParsedText::ScanLine => kind::SCAN_LINE,
            ParsedText::Tile => kind::TILE,
            ParsedText::DeepScanLine => kind::DEEP_SCAN_LINE,
            ParsedText::DeepTile => kind::DEEP_TILE,
            ParsedText::Arbitrary(ref text) => &text.bytes,
        }
    }

    /// Sadly, "type" must be one of the specified texts
    /// instead of being a plain enumeration.
    /// This method checks if the value is one of the allowed ones.
    pub fn validate_kind(&self) -> Validity {
        match *self {
            ParsedText::Arbitrary(_) => Err(Invalid::Content(
                Value::Type("type"),
                Required::OneOf(&["scanlineimage","tiledimage","deepscanline","deeptile"])
            )),
            _ => Ok(())
        }
    }

    pub fn is_deep_kind(&self) -> bool {
        match *self {
            ParsedText::DeepScanLine
            | ParsedText::DeepTile => true,
            _ => false,
        }
    }

    pub fn is_tile_kind(&self) -> bool {
        match *self {
            ParsedText::DeepTile
            | ParsedText::Tile => true,
            _ => false,
        }
    }
}


impl I32Box2 {
    pub fn validate(&self) -> Validity {
        if self.x_min > self.x_max || self.y_min > self.y_max {
            Err(Invalid::Combination(&[
                Value::Attribute("box2i min"),
                Value::Attribute("box2i max")
            ]))
        } else {
            Ok(())
        }
    }

    pub fn dimensions(&self) -> (i32, i32) {
        (
            self.x_max - self.x_min,
            self.y_max - self.y_min,
        )
    }


    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        // validate?
        self.x_min.write(write)?;
        self.y_min.write(write)?;
        self.x_max.write(write)?;
        self.y_max.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(I32Box2 {
            x_min: i32::read(read)?,
            y_min: i32::read(read)?,
            x_max: i32::read(read)?,
            y_max: i32::read(read)?,
        })
    }
}

impl F32Box2 {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.x_min.write(write)?;
        self.y_min.write(write)?;
        self.x_max.write(write)?;
        self.y_max.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(F32Box2 {
            x_min: f32::read(read)?,
            y_min: f32::read(read)?,
            x_max: f32::read(read)?,
            y_max: f32::read(read)?,
        })
    }
}

impl PixelType {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        match *self {
            PixelType::U32 => 0_i32,
            PixelType::F16 => 1_i32,
            PixelType::F32 => 2_i32,
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        // there's definitely going to be more than 255 different pixel types
        // in the future, when exr is still used
        Ok(match i32::read(read)? {
            0 => PixelType::U32,
            1 => PixelType::F16,
            2 => PixelType::F32,
            _ => return Err(Invalid::Content(
                Value::Enum("pixelType"),
                Required::Range{ min: 0, max: 2 }
            ).into())
        })
    }
}

impl Channel {
    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> WriteResult {
        Text::write_null_terminated(&self.name, write, Some(long_names))?;
        self.pixel_type.write(write)?;

        match self.is_linear {
            false => 0_u8,
            true  => 1_u8,
        }.write(write)?;

        write_i8_array(write, &self.reserved)?;
        self.x_sampling.write(write)?;
        self.y_sampling.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let name = Text::read_null_terminated(read)?;
        let pixel_type = PixelType::read(read)?;

        let is_linear = match u8::read(read)? {
            1 => true,
            0 => false,
            _ => return Err(Invalid::Content(
                Value::Enum("pLinear"),
                Required::Range{ min: 0, max: 1 }
            ).into())
        };

        let mut reserved = [0; 3];
        read_i8_array(read, &mut reserved)?;

        let x_sampling = i32::read(read)?;
        let y_sampling = i32::read(read)?;

        Ok(Channel {
            name, pixel_type, is_linear,
            reserved, x_sampling, y_sampling,
        })
    }

    pub fn write_list<W: Write>(channels: &ChannelList, write: &mut W, long_names: bool) -> WriteResult {
        for channel in channels {
            channel.write(write, long_names)?;
        }

        SequenceEnd::write(write)
    }

    pub fn read_list<R: Read + Seek>(read: &mut R) -> ReadResult<ChannelList> {
        let mut channels = SmallVec::new();
        while !SequenceEnd::has_come(read)? {
            channels.push(Channel::read(read)?);
        }

        Ok(channels)
    }
}

impl Chromaticities {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.red_x.write(write)?;
        self.red_y.write(write)?;
        self.green_x.write(write)?;
        self.green_y.write(write)?;
        self.blue_x.write(write)?;
        self.blue_y.write(write)?;
        self.white_x.write(write)?;
        self.white_y.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(Chromaticities {
            red_x: f32::read(read)?,
            red_y: f32::read(read)?,
            green_x: f32::read(read)?,
            green_y: f32::read(read)?,
            blue_x: f32::read(read)?,
            blue_y: f32::read(read)?,
            white_x: f32::read(read)?,
            white_y: f32::read(read)?,
        })
    }
}

impl Compression {
    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use self::Compression::*;
        match self {
            None => 0_u8,
            RLE => 1_u8,
            ZIPSingle => 2_u8,
            ZIP => 3_u8,
            PIZ => 4_u8,
            PXR24 => 5_u8,
            B44 => 6_u8,
            B44A => 7_u8,
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use self::Compression::*;
        Ok(match u8::read(read)? {
            0 => None,
            1 => RLE,
            2 => ZIPSingle,
            3 => ZIP,
            4 => PIZ,
            5 => PXR24,
            6 => B44,
            7 => B44A,
            _ => return Err(Invalid::Content(
                Value::Enum("compression"),
                Required::Range { min: 0, max: 7 }
            ).into()),
        })
    }
}

impl EnvironmentMap {
    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use self::EnvironmentMap::*;
        match self {
            LatitudeLongitude => 0_u8,
            Cube => 1_u8
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use self::EnvironmentMap::*;
        Ok(match u8::read(read)? {
            0 => LatitudeLongitude,
            1 => Cube,

            _ => return Err(Invalid::Content(
                Value::Enum("envmap"),
                Required::Range { min: 0, max: 1 }
            ).into()),
        })
    }
}

impl KeyCode {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.film_manufacturer_code.write(write)?;
        self.film_type.write(write)?;
        self.film_roll_prefix.write(write)?;
        self.count.write(write)?;
        self.perforation_offset.write(write)?;
        self.perforations_per_count.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(KeyCode {
            film_manufacturer_code: i32::read(read)?,
            film_type: i32::read(read)?,
            film_roll_prefix: i32::read(read)?,
            count: i32::read(read)?,
            perforation_offset: i32::read(read)?,
            perforations_per_frame: i32::read(read)?,
            perforations_per_count: i32::read(read)?,
        })
    }
}

impl LineOrder {
    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use self::LineOrder::*;
        match self {
            IncreasingY => 0_u8,
            DecreasingY => 1_u8,
            RandomY => 2_u8,
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use self::LineOrder::*;
        Ok(match u8::read(read)? {
            0 => IncreasingY,
            1 => DecreasingY,
            2 => RandomY,
            _ => return Err(Invalid::Content(
                Value::Enum("lineOrder"),
                Required::Range { min: 0, max: 2 }
            ).into()),
        })
    }
}

impl Preview {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.width.write(write)?;
        self.height.write(write)?;
        write_i8_array(write, &self.pixel_data)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let components_per_pixel = 4;
        let width = u32::read(read)?;
        let height = u32::read(read)?;

        // TODO carefully allocate
        let mut pixel_data = vec![0; (width * height * components_per_pixel) as usize];
        read_i8_array(read, &mut pixel_data)?;

        Ok(Preview {
            width, height,
            pixel_data,
        })
    }
}

impl TileDescription {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.x_size, self.y_size)
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.x_size.write(write)?;
        self.y_size.write(write)?;

        let level_mode = match self.level_mode {
            LevelMode::One => 0_u8,
            LevelMode::MipMap => 1_u8,
            LevelMode::RipMap => 2_u8,
        };

        let rounding_mode = match self.rounding_mode {
            RoundingMode::Down => 0_u8,
            RoundingMode::Up => 1_u8,
        };

        let mode = level_mode + (rounding_mode * 16);
        mode.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let x_size = u32::read(read)?;
        let y_size = u32::read(read)?;

        let mode = u8::read(read)?; // wow you really saved that one byte here

        // mode = level_mode + (rounding_mode * 16)
        let level_mode = mode & 0b00001111; // wow that works
        let rounding_mode = mode >> 4; // wow that works

        let level_mode = match level_mode {
            0 => LevelMode::One,
            1 => LevelMode::MipMap,
            2 => LevelMode::RipMap,
            _ => return Err(Invalid::Content(
                Value::Enum("level mode"),
                Required::Range { min: 0, max: 2 }
            ).into()),
        };

        let rounding_mode = match rounding_mode {
            0 => RoundingMode::Down,
            1 => RoundingMode::Up,
            _ => return Err(Invalid::Content(
                Value::Enum("rounding mode"),
                Required::Range { min: 0, max: 1 }
            ).into()),
        };

        Ok(TileDescription { x_size, y_size, level_mode, rounding_mode, })
    }
}

impl Attribute {
    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> WriteResult {
        self.name.write_null_terminated(write, Some(long_names))?;
        Text::write_null_terminated_bytes(self.value.kind_name(), write, Some(long_names))?;
        (self.value.byte_size() as i32).write(write)?;
        self.value.write(write, long_names)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read + Seek>(read: &mut R) -> ReadResult<Self> {
        let name = Text::read_null_terminated(read)?;
        let kind = Text::read_null_terminated(read)?;
        let size = i32::read(read)? as u32; // TODO .checked_cast.ok_or(err:negative)
        let value = AttributeValue::read(read, kind, size)?;
        Ok(Attribute { name, value, })
    }
}



impl AttributeValue {
    pub fn byte_size(&self) -> usize {
//        use self::AttributeValue::*;
        match *self {
            _ => unimplemented!()
        }
    }

    pub fn kind_name(&self) -> &'static [u8] {
        use self::AttributeValue::*;
        match *self {
            // TODO replace these literals with constants
            I32Box2(_) =>  b"box2i",
            F32Box2(_) =>  b"box2f",
            I32(_) =>  b"int",
            F32(_) =>  b"float",
            F64(_) =>  b"double",
            Rational(_, _) => b"rational",
            TimeCode(_, _) => b"timecode",
            I32Vec2(_, _) => b"vec2i",
            F32Vec2(_, _) => b"vec2f",
            I32Vec3(_, _, _) => b"vec3i",
            F32Vec3(_, _, _) => b"vec3f",
            ChannelList(_) =>  b"chlist",
            Chromaticities(_) =>  b"chromaticities",
            Compression(_) =>  b"compression",
            EnvironmentMap(_) =>  b"envmap",
            KeyCode(_) =>  b"keycode",
            LineOrder(_) =>  b"lineOrder",
            F32Matrix3x3(_) =>  b"m33f",
            F32Matrix4x4(_) =>  b"m44f",
            Preview(_) =>  b"preview",
            Text(_) =>  b"string",
            TextVector(_) =>  b"stringvector",
            TileDescription(_) =>  b"tiledesc",
        }
    }

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> WriteResult {
        use self::AttributeValue::*;
        match *self {
            I32Box2(value) => value.write(write),
            F32Box2(value) => value.write(write),

            I32(value) => value.write(write),
            F32(value) => value.write(write),
            F64(value) => value.write(write),

            Rational(a, b) => { a.write(write)?; b.write(write) },
            TimeCode(a, b) => { a.write(write)?; b.write(write) },

            I32Vec2(x, y) => { x.write(write)?; y.write(write) },
            F32Vec2(x, y) => { x.write(write)?; y.write(write) },
            I32Vec3(x, y, z) => { x.write(write)?; y.write(write)?; z.write(write) },
            F32Vec3(x, y, z) => { x.write(write)?; y.write(write)?; z.write(write) },

            ChannelList(ref channels) => Channel::write_list(channels, write, long_names),
            Chromaticities(ref chroma) => chroma.write(write),
            Compression(value) => value.write(write),
            EnvironmentMap(value) => value.write(write),

            KeyCode(value) => value.write(write),
            LineOrder(value) => value.write(write),

            F32Matrix3x3(ref value) => write_f32_array(write, value),
            F32Matrix4x4(ref value) => write_f32_array(write, value),

            Preview(ref value) => value.write(write),

            // attribute value texts never have limited size.
            // also, don't serialize size, as it can be inferred from attribute size
            Text(ref value) => write_u8_array(write, value.to_text_bytes()),

            TextVector(ref value) => self::Text::write_vec_of_i32_sized_texts(write, value), // TODO check length 31 or 255
            TileDescription(ref value) => value.write(write),
        }
    }

    pub fn read<R: Read + Seek>(read: &mut R, kind: Text, byte_size: u32) -> ReadResult<Self> {
        use self::AttributeValue::*;
        Ok(match kind.bytes.as_slice() {
            // TODO replace these literals with constants
            b"box2i" => I32Box2(self::I32Box2::read(read)?),
            b"box2f" => F32Box2(self::F32Box2::read(read)?),

            b"int"    => I32(i32::read(read)?),
            b"float"  => F32(f32::read(read)?),
            b"double" => F64(f64::read(read)?),

            b"rational" => Rational(i32::read(read)?, u32::read(read)?),
            b"timecode" => TimeCode(u32::read(read)?, u32::read(read)?),

            b"v2i" => I32Vec2(i32::read(read)?, i32::read(read)?),
            b"v2f" => F32Vec2(f32::read(read)?, f32::read(read)?),
            b"v3i" => I32Vec3(i32::read(read)?, i32::read(read)?, i32::read(read)?),
            b"v3f" => F32Vec3(f32::read(read)?, f32::read(read)?, f32::read(read)?),

            b"chlist" => ChannelList(self::Channel::read_list(read)?),
            b"chromaticities" => Chromaticities(self::Chromaticities::read(read)?),
            b"compression" => Compression(self::Compression::read(read)?),
            b"envmap" => EnvironmentMap(self::EnvironmentMap::read(read)?),

            b"keycode" => KeyCode(self::KeyCode::read(read)?),
            b"lineOrder" => LineOrder(self::LineOrder::read(read)?),

            b"m33f" => F32Matrix3x3({
                let mut result = [0.0_f32; 9];
                read_f32_array(read, &mut result)?;
                result
            }),

            b"m44f" => F32Matrix4x4({
                let mut result = [0.0_f32; 16];
                read_f32_array(read, &mut result)?;
                result
            }),

            b"preview" => Preview(self::Preview::read(read)?),
            b"string" => Text(ParsedText::parse(self::Text::read_sized(read, byte_size as usize)?)),
            b"stringvector" => TextVector(self::Text::read_vec_of_i32_sized(read, byte_size)?),
            b"tiledesc" => TileDescription(self::TileDescription::read(read)?),

            _ => {
                println!("Unknown attribute type: {:?}", kind.to_string());
                return Err(ReadError::UnknownAttributeType { bytes_to_skip: byte_size })
            }
        })
    }

    pub fn to_tile_description(&self) -> Result<TileDescription, Invalid> {
        match *self {
            AttributeValue::TileDescription(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("tiledesc")).into()), // TODO make these constants!
        }
    }

    pub fn to_i32(&self) -> Result<i32, Invalid> {
        match *self {
            AttributeValue::I32(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("i32")).into()),
        }
    }

    pub fn to_i32_box_2(&self) -> Result<I32Box2, Invalid> {
        match *self {
            AttributeValue::I32Box2(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("box2i")).into()),
        }
    }

    pub fn to_compression(&self) -> Result<Compression, Invalid> {
        match *self {
            AttributeValue::Compression(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("compression")).into()),
        }
    }

    pub fn to_text(&self) -> Result<&ParsedText, Invalid> {
        match *self {
            AttributeValue::Text(ref value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("string")).into()),
        }
    }

    pub fn to_channel_list(&self) -> Result<&ChannelList, Invalid> {
        match *self {
            AttributeValue::ChannelList(ref value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("chlist")).into()),
        }
    }

    pub fn to_chromaticities(&self) -> Result<Chromaticities, Invalid> {
        match *self {
            AttributeValue::Chromaticities(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("chromaticities")).into()),
        }
    }
}


pub mod required {
    macro_rules! define_required_attribute_names {
        ( $($name: ident : $value: expr),* ) => {
            $(
                pub const $name: &'static [u8] = $value;
            )*
        };
    }

    define_required_attribute_names! {
        TILES: b"tiles",
        NAME: b"name",
        TYPE: b"type",
        VERSION: b"version",
        CHUNKS: b"chunkCount",
        MAX_SAMPLES: b"maxSamplesPerPixel",
        CHANNELS: b"channels",
        COMPRESSION: b"compression",
        DATA_WINDOW: b"dataWindow",
        DISPLAY_WINDOW: b"displayWindow",
        LINE_ORDER: b"lineOrder",
        PIXEL_ASPECT: b"pixelAspectRatio",
        WINDOW_CENTER: b"screenWindowCenter",
        WINDOW_WIDTH: b"screenWindowWidth"
    }
}

impl RoundingMode {
    pub fn divide(self, dividend: u32, divisor: u32) -> u32 {
        let result = dividend / divisor;
        match self {
            RoundingMode::Up if result * divisor < dividend => {
                result + 1
            },

            _ => result,
        }
    }
}