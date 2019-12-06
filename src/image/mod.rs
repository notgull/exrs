//! The `image` module is for interpreting the loaded file data.
//!


use smallvec::SmallVec;
use crate::file::meta::{Header, MetaData, compute_level_count, compute_level_size, TileIndices, Headers};
use crate::file::data::{Chunks, Block};
use crate::file::io::*;
use crate::file::meta::attributes::{PixelType, LevelMode, Kind};
use crate::file::data::compression::{ByteVec, Compression};
use crate::error::validity::{Invalid, Value, Required};
//use rayon::prelude::*;
use half::f16;
use crate::error::ReadResult;
use std::io::BufReader;


// TODO notes:
// Channels with an x or y sampling rate other than 1 are allowed only in flat, scan-line based images. If an image is deep or tiled, then the x and y sampling rates for all of its channels must be 1.
// Scan-line based images cannot be multi-resolution images.


pub mod meta {
    use crate::file::meta::MetaData;
    use std::io::{Read, BufReader};
    use crate::error::ReadResult;
    use crate::file::io::PeekRead;

    #[must_use]
    pub fn read_from_file(path: &::std::path::Path) -> ReadResult<MetaData> {
        read_from_unbuffered(::std::fs::File::open(path)?)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read) -> ReadResult<MetaData> {
        read_from_buffered(BufReader::new(unbuffered))
    }

    #[must_use]
    pub fn read_from_buffered(buffered: impl Read) -> ReadResult<MetaData> {
        MetaData::read_validated(&mut PeekRead::new(buffered))
    }
}




#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    pub parts: Parts
}

/// an exr image can store multiple parts (multiple bitmaps inside one image)
pub type Parts = SmallVec<[Part; 2]>;

#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub header: Header, // TODO dissolve header properties into part, and put a name into the channels?

    /// only the data for this single part,
    /// index can be computed from pixel location and block_kind.
    /// one part can only have one block_kind, not a different kind per block
    /// number of x and y levels can be computed using the header
    ///
    /// That Vec contains one entry per mip map level, or only one if it does not have any,
    /// or a row-major flattened vector of all rip maps like
    /// 1x1, 2x1, 4x1, 8x1, and then
    /// 1x2, 2x2, 4x2, 8x2, and then
    /// 1x4, 2x4, 4x4, 8x4, and then
    /// 1x8, 2x8, 4x8, 8x8.
    ///
    // FIXME should be descending and starting with full-res instead!
    pub levels: Levels

    // offset tables are already processed while loading 'data'
    // TODO skip reading offset tables if not required?
}

#[derive(Clone, PartialEq, Debug)]
pub enum Levels {
    Singular(PartData),
    Mip(LevelMaps),
    Rip(RipMaps),
}

// FIXME can each level of a mip map contain independent deep or flat data???!!?!
pub type LevelMaps = SmallVec<[PartData; 16]>;

#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps {
    pub maps: LevelMaps,
    pub level_count: (u32, u32),
}

/// one `type` per Part
#[derive(Clone, PartialEq, Debug)]
pub enum PartData {
    /// One single array containing all pixels, row major left to right, top to bottom
    /// same length as `Part.channels` field
    // TODO should store sampling_x/_y for simple accessors?
    Flat(Pixels<Array>),

    ///
    Deep(Pixels<DeepArray>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Pixels<T> {
    pub dimensions: (u32, u32),

    /// always sorted alphabetically
    pub channels: PerChannel<T>
}

pub type PerChannel<T> = SmallVec<[T; 5]>;

#[derive(Clone, Debug, PartialEq)]
pub enum Array {
    U32(Vec<u32>),

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction
    F16(Vec<f16>),

    F32(Vec<f32>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeepArray {
    // TODO
}

#[must_use]
pub fn read_from_file(path: &::std::path::Path, parallel: bool) -> ReadResult<Image> {
    read_from_unbuffered(::std::fs::File::open(path)?, parallel)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn read_from_unbuffered(unbuffered: impl Read, parallel: bool) -> ReadResult<Image> {
    read_from_buffered(BufReader::new(unbuffered), parallel)
}

// TODO use custom simple peek-read instead of seekable read?
#[must_use]
pub fn read_from_buffered(buffered_read: impl Read, parallel: bool) -> ReadResult<Image> {
    let (headers, chunks) = {
        let mut read = PeekRead::new(buffered_read);

        let MetaData { headers, offset_tables, requirements } = MetaData::read_validated(&mut read)?;
        let chunks = Chunks::read(&mut read, requirements.is_multipart(), &headers, offset_tables)?;
        (headers, chunks)
    };

    let mut image = Image::new(headers);

    if parallel {
        image.decompress_parallel(chunks)?;
    }
    else {
        image.decompress(chunks)?;
    }

    Ok(image)
}


impl Image {
    pub fn new(headers: Headers) -> Self {
        Image {
            parts: headers.into_iter().map(Part::new).collect()
        }
    }

    pub fn decompress(&mut self, chunks: Chunks) -> ReadResult<()> {
        let part_count = self.parts.len();

        for chunk in chunks.content {
            let part = self.parts.get_mut(chunk.part_number as usize)
                .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

            let (tile, data) = match chunk.block {
                Block::Tile(tile) => (part.header.get_tile_indices(tile.coordinates)?, tile.compressed_pixels),
                Block::ScanLine(block) => (part.header.get_scan_line_indices(block.y_coordinate)?, block.compressed_pixels),
                _ => unimplemented!()
            };

            let expected_byte_size = tile.size.0 * tile.size.1 * part.header.channels.bytes_per_pixel;
            let data = part.header.compression.decompress_bytes(data, expected_byte_size as usize)?;

            part.read_block(&mut data.as_slice(), tile)?;
        }

        Ok(())
    }

    pub fn decompress_parallel(&mut self, chunks: Chunks) -> ReadResult<()> {
        use threadpool::ThreadPool;
        use std::sync::mpsc::channel;

        #[derive(Clone, PartialEq, Debug)]
        struct DecompressibleBlock {
            part_index: usize,
            tile: TileIndices,
            data: ByteVec,
            compression: Compression,
            bytes_per_pixel: u32
        }

        let blocks: Vec<ReadResult<DecompressibleBlock>> = chunks.content.into_iter()
            .map(|chunk|{
                let part_count = self.parts.len();
                let part: &Part = self.parts.get(chunk.part_number as usize)
                    .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

                let (tile, data) = match chunk.block {
                    Block::Tile(tile) => (part.header.get_tile_indices(tile.coordinates)?, tile.compressed_pixels),
                    Block::ScanLine(block) => (part.header.get_scan_line_indices(block.y_coordinate)?, block.compressed_pixels),
                    _ => unimplemented!()
                };

                Ok(DecompressibleBlock {
                    compression: part.header.compression,
                    bytes_per_pixel: part.header.channels.bytes_per_pixel,
                    part_index: chunk.part_number as usize,
                    tile, data,
                })
            }).collect();

        let pool = ThreadPool::new(num_cpus::get());

        let receiver = {
            let (sender, receiver) = channel();

            for value in blocks {
                let sender = sender.clone();
                pool.execute(move || {
                    // decompress on hopefully multiple threads
                    let result = value.and_then(|block|{
                        let expected_byte_size = block.tile.size.0 * block.tile.size.1 * block.bytes_per_pixel;
                        let decompressed_bytes = block.compression.decompress_bytes(block.data, expected_byte_size as usize)?;
                        Ok(DecompressibleBlock { data: decompressed_bytes, .. block })
                    });

                    sender.send(result).expect("thread pool error");
                });
            }

            receiver
        };

        for result in receiver {
            let block = result?;
            let part_count = self.parts.len();
            let part = self.parts.get_mut(block.part_index)
                .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

            part.read_block(&mut block.data.as_slice(), block.tile)?;
        }

        pool.join();
        Ok(())
    }

}

impl Part {

    /// allocates all the memory necessary to hold the pixel data,
    /// zeroed out, ready to be filled with actual pixel data
    pub fn new(header: Header) -> Self {
        match header.kind {
            None | Some(Kind::ScanLine) | Some(Kind::Tile) => {
                let levels = {
                    let data_size = header.data_window.dimensions();

                    let part_data = |dimensions: (u32, u32)| {
                        let data = header.channels.list.iter()
                            .map(|channel| { match channel.pixel_type {
                                PixelType::F16 => Array::F16(vec![half::f16::ZERO; channel.subsampled_pixels(dimensions) as usize]),
                                PixelType::F32 => Array::F32(vec![0.0; channel.subsampled_pixels(dimensions) as usize]),
                                PixelType::U32 => Array::U32(vec![0; channel.subsampled_pixels(dimensions) as usize]),
                            }})
                            .collect();

                        PartData::Flat(Pixels { dimensions, channels: data })
                    };

                    if let Some(tiles) = &header.tiles {
                        debug_assert_eq!(header.kind, Some(Kind::Tile));

                        let round = tiles.rounding_mode;
                        let level_count = |full_res: u32| {
                            compute_level_count(round, full_res)
                        };

                        let level_size = |full_res: u32, level_index: u32| {
                            compute_level_size(round, full_res, level_index)
                        };

                        // TODO cache all these level values?? and reuse algorithm from crate::file::meta::compute_offset_table_sizes?

                        match tiles.level_mode {
                            LevelMode::Singular => Levels::Singular(part_data(data_size)),

                            LevelMode::MipMap => Levels::Mip(
                                (0..level_count(data_size.0.max(data_size.1)))
                                    .map(|level|{
                                        let width = level_size(data_size.0, level);
                                        let height = level_size(data_size.1, level);
                                        part_data((width, height))
                                    })
                                    .collect()
                            ),

                            // TODO put this into Levels::new(..) ?
                            LevelMode::RipMap => Levels::Rip({
                                let level_count = (level_count(data_size.0), level_count(data_size.1));

                                let maps = (0..level_count.0) // TODO test this
                                    .flat_map(|x_level|{ // TODO may swap y and x?
                                        (0..level_count.1).map(move |y_level| {
                                            let width = level_size(data_size.0, x_level);
                                            let height = level_size(data_size.1, y_level);
                                            part_data((width, height))
                                        })
                                    })
                                    .collect();

                                RipMaps { maps, level_count }
                            })
                        }
                    }

                    // scan line blocks never have mip maps? // TODO check if this is true
                    else {
                        Levels::Singular(part_data(data_size))
                    }
                };

                Part { levels, header }
            },

            Some(Kind::DeepScanLine) | Some(Kind::DeepTile) => unimplemented!("deep allocation"),
        }
    }


    pub fn read_block(&mut self, read: &mut impl Read, block: TileIndices) -> ReadResult<()> {
        match &mut self.levels {
            Levels::Singular(ref mut part) => {
                debug_assert_eq!(block.level, (0,0), "singular image cannot read leveled blocks");
                part.read_lines(read, block.position, block.size)?;
            },

            Levels::Mip(maps) => {
                debug_assert_eq!(block.level.0, block.level.1, "mip map levels must be equal on x and y");
                let max = maps.len();

                maps.get_mut(block.level.0 as usize)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .read_lines(read, block.position, block.size)?;
            },

            Levels::Rip(maps) => {
                let max = maps.maps.len();

                maps.get_by_level_mut(block.level)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .read_lines(read, block.position, block.size)?;
            }
        };

        Ok(())
    }
}


impl PartData {
    fn read_lines(&mut self, read: &mut impl Read, position: (u32, u32), block_size: (u32, u32)) -> ReadResult<()> {
        match self {
            PartData::Flat(ref mut pixels) => {
                let image_width = pixels.dimensions.0;

                for line_index in 0..block_size.1 {
                    let start_index = ((position.1 + line_index) * image_width) as usize;
                    let end_index = start_index + block_size.0 as usize;

                    for channel in &mut pixels.channels {
                        match channel {
                            Array::F16(ref mut target) =>
                                read_f16_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

                            Array::F32(ref mut target) =>
                                read_f32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

                            Array::U32(ref mut target) =>
                                read_u32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data
                        }
                    }
                }

                Ok(())
            },

            _ => unimplemented!("deep pixel accumulation")
        }
    }
}

impl RipMaps {
    pub fn get_level_index(&self, level: (u32,u32)) -> usize {
        (self.level_count.0 * level.1 + level.0) as usize  // TODO check this calculation (x vs y)
    }

    pub fn get_by_level(&self, level: (u32, u32)) -> Option<&PartData> {
        self.maps.get(self.get_level_index(level))
    }

    pub fn get_by_level_mut(&mut self, level: (u32, u32)) -> Option<&mut PartData> {
        let index = self.get_level_index(level);
        self.maps.get_mut(index)
    }
}

impl Levels {
    pub fn largest(&self) -> &PartData {
        match *self {
            Levels::Singular(ref data) => data,
            Levels::Mip(ref maps) => &maps[0], // TODO is this really the largest one?
            Levels::Rip(ref rip_map) => &rip_map.maps[0], // TODO test!
        }
    }
}
