
//! Read and write all supported aspects of an exr image, including deep data and multiresolution levels.
//! Use `exr::image::simple` if you do not need deep data or resolution levels.

use smallvec::SmallVec;
use half::f16;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::error::{Result, PassiveResult, Error};
use crate::math::*;
use std::io::{Seek, BufReader, BufWriter};
use crate::image::{Line, LineIndex};

// TODO dry this module with image::full?


/// Specify how to write an exr image.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {

    /// Enable multicore compression.
    pub parallel_compression: bool,
}



/// Specify how to read an exr image.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadOptions {

    /// Enable multicore decompression.
    pub parallel_decompression: bool,
}

// FIXME will allocate but not overwrite deep data contents????

/// An exr image.
///
/// Supports all possible exr image features.
/// An exr image may contain multiple image parts.
/// All meta data is encoded in this image,
/// including custom attributes.
#[derive(Clone, PartialEq, Debug)]
pub struct Image {

    /// All image parts contained in the image file
    pub parts: Parts,

    /// The rectangle positioned anywhere in the infinite 2D space that
    /// clips all contents of the file, limiting what should be rendered.
    pub display_window: IntRect,

    /// Aspect ratio of each pixel in this image part.
    pub pixel_aspect: f32,
}


pub type Parts = SmallVec<[Part; 3]>;


/// A single image part of an exr image.
/// Contains meta data and actual pixel information of the channels.
#[derive(Clone, PartialEq, Debug)]
pub struct Part {

    /// The name of the image part.
    /// This is optional for files with only one image part.
    pub name: Option<Text>,

    /// The remaining attributes which are not already in the `Part`.
    /// Includes custom attributes.
    pub attributes: Attributes,

    /// The rectangle that positions this image part
    /// within the global infinite 2D space of the file.
    pub data_window: IntRect,

    /// Part of the perspective projection. Default should be `(0, 0)`.
    pub screen_window_center: Vec2<f32>, // TODO use sensible defaults instead of returning an error on missing?

    /// Part of the perspective projection. Default should be `1`.
    pub screen_window_width: f32,

    /// In what order the tiles of this header occur in the file.
    pub line_order: LineOrder,

    /// How the pixel data of all channels in this image part is compressed. May be `Compression::Uncompressed`.
    pub compression: Compression,

    /// If this is some pair of numbers, the image is divided into tiles of that size.
    /// If this is none, the image is divided into scan line blocks, depending on the compression method.
    pub tiles: Option<Vec2<usize>>,

    /// List of channels in this image part.
    /// Contains the actual pixel data of the image.
    pub channels: Channels,
}


// TODO API use sorted set by name instead??
pub type Channels = SmallVec<[Channel; 5]>;


/// Contains an arbitrary list of pixel data.
/// Each channel can have a different pixel type,
/// either f16, f32, or u32.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {

    /// One of "R", "G", or "B" most of the time.
    pub name: Text,

    /// The actual pixel data. Contains a flattened vector of samples.
    /// The vector contains each row, one after another.
    /// The number of pixels depends on the resolution of the image part
    /// and the sampling rate of this channel.
    ///
    /// Thus, a specific pixel value can be found at the index
    /// `samples[(y_index / sampling_y) * width + (x_index / sampling_x)]`.
    pub samples: Samples,

    /// Are the samples in this channel in linear color space?
    pub is_linear: bool,

    /// How many of the samples are skipped compared to the other channels in this image part.
    ///
    /// Can be used for chroma subsampling for manual lossy data compression.
    /// Values other than 1 are allowed only in flat, scan-line based images.
    /// If an image is deep or tiled, x and y sampling rates for all of its channels must be 1.
    pub sampling: Vec2<usize>,
}

/// Actual pixel data in a channel. Is either one of f16, f32, or u32.
// TODO not require vec storage but also on-the-fly generation
#[derive(Clone, PartialEq)]
pub enum Samples {

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction.
    ///
    /// Currently this crate is using the `half` crate, which is an implementation of the IEEE 754-2008 standard, meeting that requirement.
    F16(Vec<f16>),

    /// 32-bit float samples.
    F32(Vec<f32>),

    /// 32-bit unsigned int samples.
    /// Used for segmentation of image parts.
    U32(Vec<u32>),
}


/*#[derive(Clone, PartialEq)] TODO
pub enum Samples {
    F16(SampleStorage<f16>),
    F32(SampleStorage<f32>),
    U32(SampleStorage<u32>),
}

pub trait SampleStorage<T> {
    fn sample(position: Vec2, resolution: Vec2) -> T,
    fn allocate() ???
}

impl SampleStorage<f16> for Vec<f16> { }
impl SampleStorage<f16> for Fn(Vec2) -> Iterator<Item=f16> { }*/



impl Default for WriteOptions {
    fn default() -> Self { Self::fast() }
}

impl Default for ReadOptions {
    fn default() -> Self { Self::fast() }
}


impl WriteOptions {
    pub fn fast() -> Self { WriteOptions { parallel_compression: true, } }
    pub fn low_memory() -> Self { WriteOptions { parallel_compression: false } }
    pub fn debug() -> Self { WriteOptions { parallel_compression: false, } }
}

impl ReadOptions {
    pub fn fast() -> Self { ReadOptions { parallel_decompression: true } }
    pub fn low_memory() -> Self { ReadOptions { parallel_decompression: false } }
    pub fn debug() -> Self { ReadOptions { parallel_decompression: false } }
}



/*#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ChannelSampler<'t, T: 't> {
    samples: &'t [T],
    subsampled_size: Vec2<usize>,
    subsampling_factor: Vec2<usize>,
}

impl<'t, T> ChannelSampler<'t, T> {
    pub fn sample(&self, pixel: Vec2<usize>) -> &'t T {
        let local_index = pixel / self.subsampling_factor;
        debug_assert!(local_index.0 < self.subsampled_size.0, "invalid x coordinate");
        debug_assert!(local_index.1 < self.subsampled_size.1, "invalid y coordinate");
        &self.samples[local_index.1 * self.subsampled_size.0 + local_index.0]
    }
}*/



impl Image {

    /// Create an image that is to be written to a file.
    /// Defined the `display_window` to define
    /// the area in the infinite 2D space that should be visible.
    ///
    /// Consider using `Image::from_parts` for more complex cases.
    /// Use the raw `Image { .. }` constructor for more complex cases.
    pub fn new_from_single_part(part: Part) -> Self { // TODO inline part parameters?
        Self {
            pixel_aspect: 1.0,
            display_window: part.data_window,
            parts: smallvec![ part ],
        }
    }

    /// Create an image that is to be written to a file.
    /// Defined the `display_window` to define
    /// the area in the infinite 2D space that should be visible.
    ///
    /// Consider using `Image::from_single_part` for simpler cases.
    /// Use the raw `Image { .. }` constructor for more complex cases.
    pub fn new_from_parts(parts: Parts, display_window: IntRect) -> Self {
        Self {
            parts,
            display_window,
            pixel_aspect: 1.0,
        }
    }


    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    /// Returns an empty image in case only deep data exists in the file.
    #[must_use]
    pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions) -> Result<Self> {
        Self::read_from_unbuffered(std::fs::File::open(path)?, options)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read + Send + Seek, options: ReadOptions) -> Result<Self> { // TODO not need be seek nor send
        Self::read_from_buffered(BufReader::new(unbuffered), options)
    }

    /// Read the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    #[must_use]
    pub fn read_from_buffered(read: impl Read + Send + Seek, options: ReadOptions) -> Result<Self> { // TODO not need be seek nor send
        // crate::image::read_all_lines(read, options.parallel_decompression, Image::allocate, Image::insert_line)
        crate::image::read_filtered_lines_from_buffered(
            read, options.parallel_decompression,
            |header, tile_index| {
                !header.deep && tile_index.location.level_index == Vec2(0,0)
            },
            Image::allocate, Image::insert_line
        )
    }

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    #[must_use]
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>, options: WriteOptions) -> PassiveResult {
        self.write_to_unbuffered(std::fs::File::create(path)?, options)
    }

    /// Buffer the reader and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[must_use]
    pub fn write_to_unbuffered(&self, unbuffered: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        self.write_to_buffered(BufWriter::new(unbuffered), options)
    }

    /// Write the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[must_use]
    pub fn write_to_buffered(&self, write: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        crate::image::write_all_lines_to_buffered(
            write, options.parallel_compression, self.infer_meta_data(),
            |location, write| {
                self.extract_line(location, write);
            }
        )
    }
}


impl Part {

    /// Create a new image part with all required fields.
    /// Uses scan line blocks, and no custom attributes.
    /// Panics if anything is invalid or missing.
    pub fn new(name: Text, compression: Compression, data_window: IntRect, mut channels: Channels) -> Self {
        assert!(!channels.is_empty(), "at least one channel is required");

        assert!(
            channels.iter().all(|chan|
                chan.samples.len() / (chan.sampling.0 * chan.sampling.1) == data_window.size.area()
            ),
            "channel data size must conform to data window size (scaled by channel sampling)"
        );

        channels.sort_by_key(|chan| chan.name.clone()); // TODO why clone?!

        Part {
            channels,
            data_window,
            name: Some(name),
            attributes: Vec::new(),
            compression,

            tiles: None,
            line_order: LineOrder::Unspecified, // non-parallel write will set this to increasing if possible
            screen_window_center: Vec2(0.0, 0.0),
            screen_window_width: 1.0,
        }
    }
}


impl Channel {

    // TODO do not debug print pixel values

    /// Create a Channel from name and samples.
    /// Set `is_linear` if the color space of the samples values is linear.
    /// Panics if anything is invalid or missing.
    pub fn new(name: Text, is_linear: bool, samples: Samples) -> Self {
        Self { name, samples, is_linear, sampling: Vec2(1, 1) }
    }

    /// Create a Channel from name and samples.
    /// Use this if the color space of the samples values is linear, otherwise, use `Channel::new`.
    /// Panics if anything is invalid or missing.
    pub fn new_linear(name: Text, samples: Samples) -> Self {
        Self::new(name, true, samples)
    }

    /*/// Computes the size as seen in the global infinite 2D space of the file.
    pub fn view_size(&self) -> Vec2<usize> {
        self.sample.resolution * self.sampling
    }*/

    /*/// Create a `SampleBlock` from resolution and sample vector.
    /// Panics if the resolution does not match the sample vector length.
    pub fn f16s(resolution: Vec2<usize>, samples: Vec<f16>) -> Self {
        assert_eq!(resolution.area(), samples.len());
        Self { resolution, samples: Samples::F16(samples) }
    }

    /// Create a `SampleBlock` from resolution and sample vector.
    /// Panics if the resolution does not match the sample vector length.
    pub fn f32s(resolution: Vec2<usize>, samples: Vec<f32>) -> Self {
        Samples::F32(samples)
    }

    /// Create a `SampleBlock` from resolution and sample vector.
    /// Panics if the resolution does not match the sample vector length.
    pub fn u32s(resolution: Vec2<usize>, samples: Vec<u32>) -> Self {
        assert_eq!(resolution.area(), samples.len());
        Samples::U32(samples)
    }*/
}

impl Samples {
    pub fn len(&self) -> usize {
        match self {
            Samples::F16(vec) => vec.len(),
            Samples::F32(vec) => vec.len(),
            Samples::U32(vec) => vec.len(),
        }
    }
}



impl Image {

    /// Allocate an image ready to be filled with pixel data.
    pub fn allocate(headers: &[Header]) -> Result<Self> {
        let display_window = headers.iter()
            .map(|header| header.display_window)
            .next().unwrap_or(IntRect::zero()); // default value if no headers are found

        let pixel_aspect = headers.iter()
            .map(|header| header.pixel_aspect)
            .next().unwrap_or(1.0); // default value if no headers are found

        let headers : Result<_> = headers.iter().map(Part::allocate).collect();

        Ok(Image {
            parts: headers?,
            display_window,
            pixel_aspect
        })
    }

    /// Insert one line of pixel data into this image.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert_ne!(line.location.width, 0);

        let part = self.parts.get_mut(line.location.part)
            .ok_or(Error::invalid("chunk part index"))?;

        part.insert_line(line)
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) {
        debug_assert_ne!(index.width, 0);

        let part = self.parts.get(index.part)
            .expect("invalid part index");

        part.extract_line(index, write)
    }

    /// Create the meta data that describes this image.
    pub fn infer_meta_data(&self) -> MetaData {
        let headers: Headers = self.parts.iter()
            .map(|part| part.infer_header(self.display_window, self.pixel_aspect))
            .collect();

        let has_tiles = headers.iter().any(|header| header.blocks.has_tiles());

        MetaData {
            requirements: Requirements::new(
                self.minimum_version(), headers.len() > 1, has_tiles,
                self.has_long_names(), false // TODO deep data
            ),

            headers
        }
    }

    /// Compute the version number that this image requires to be decoded.
    /// For simple images, this should return `1`.
    ///
    /// Currently always returns `2`.
    pub fn minimum_version(&self) -> u8 {
        2 // TODO pick lowest possible
    }

    /// Check if this file has long name strings.
    ///
    /// Currently always returns `true`.
    pub fn has_long_names(&self) -> bool {
        true // TODO check all name string lengths
    }
}


impl Part {

    /// Allocate an image part ready to be filled with pixel data.
    pub fn allocate(header: &Header) -> Result<Self> {
        Ok(Part {
            data_window: header.data_window,
            screen_window_center: header.screen_window_center,
            screen_window_width: header.screen_window_width,
            name: header.name.clone(),
            attributes: header.custom_attributes.clone(),
            channels: header.channels.list.iter().map(|channel| Channel::allocate(header, channel)).collect(),
            compression: header.compression,
            line_order: header.line_order,

            tiles: match header.blocks {
                Blocks::ScanLines => None,
                Blocks::Tiles(tiles) => Some(tiles.tile_size),
            }
        })
    }

    /// Insert one line of pixel data into this image part.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert!(line.location.position.0 + line.location.width <= self.data_window.size.0);
        debug_assert!(line.location.position.1 < self.data_window.size.1);

        self.channels.get_mut(line.location.channel)
            .expect("invalid channel index")
            .insert_line(line, self.data_window.size)
    }

    /// Read one line of pixel data from this image part.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) {
        debug_assert!(index.position.0 + index.width <= self.data_window.size.0);
        debug_assert!(index.position.1 < self.data_window.size.1);

        self.channels.get(index.channel)
            .expect("invalid channel index")
            .extract_line(index, self.data_window.size, write)
    }

    /// Create the meta data that describes this image part.
    pub fn infer_header(&self, display_window: IntRect, pixel_aspect: f32) -> Header {
        debug_assert_eq!( // TODO performance: use real is_sorted
            {
                let mut cloned = self.channels.clone();
                cloned.sort_by_key(|c| c.name.clone());
                cloned
            },
            self.channels,
            "channels must be sorted alphabetically"
        );

        let blocks = match self.tiles {
            Some(tiles) => Blocks::Tiles(TileDescription {
                tile_size: tiles,
                level_mode: LevelMode::Singular,
                rounding_mode: RoundingMode::Down
            }),

            None => Blocks::ScanLines,
        };

        let chunk_count = compute_chunk_count(
            self.compression, self.data_window, blocks
        );

        Header {
            chunk_count,

            name: self.name.clone(),
            data_window: self.data_window,
            screen_window_center: self.screen_window_center,
            screen_window_width: self.screen_window_width,
            compression: self.compression,
            channels: ChannelList::new(self.channels.iter().map(Channel::infer_channel_attribute).collect()), // FIXME this would sort channels, but line index would be mixed up if the original channels were not sorted
            line_order: self.line_order,
            custom_attributes: self.attributes.clone(),
            display_window, pixel_aspect,
            blocks,

            deep_data_version: None,
            max_samples_per_pixel: None,
            deep: false
        }
    }
}

impl Channel {

    /// Allocate a channel ready to be filled with pixel data.
    pub fn allocate(header: &Header, channel: &crate::meta::attributes::Channel) -> Self {
        Channel {
            name: channel.name.clone(),
            is_linear: channel.is_linear,
            sampling: channel.sampling,
            samples: Samples::allocate(header.data_window.size / channel.sampling, channel.pixel_type)
        }
    }

    /// Insert one line of pixel data into this channel.
    pub fn insert_line(&mut self, line: Line<'_>, resolution: Vec2<usize>) -> PassiveResult {
        assert_eq!(line.location.level, Vec2(0,0));
        self.samples.insert_line(resolution / self.sampling, line)
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, resolution: Vec2<usize>, write: &mut impl Write) {
        debug_assert_eq!(index.level, Vec2(0,0));
        self.samples.extract_line(index, resolution / self.sampling, write)
    }

    /// Create the meta data that describes this channel.
    pub fn infer_channel_attribute(&self) -> attributes::Channel {
        attributes::Channel {
            pixel_type: match self.samples {
                Samples::F16(_) => PixelType::F16,
                Samples::F32(_) => PixelType::F32,
                Samples::U32(_) => PixelType::U32,
            },

            name: self.name.clone(),
            is_linear: self.is_linear,
            sampling: self.sampling,
        }
    }
}


impl Samples {

    /// Allocate a sample block ready to be filled with pixel data.
    pub fn allocate(resolution: Vec2<usize>, pixel_type: PixelType) -> Self {
        let count = resolution.area();

        match pixel_type {
            PixelType::F16 => Samples::F16(vec![ f16::ZERO; count ] ),
            PixelType::F32 => Samples::F32(vec![ 0.0; count ] ),
            PixelType::U32 => Samples::U32(vec![ 0; count ] ),
        }
    }

    /// Insert one line of pixel data into this sample block.
    pub fn insert_line(&mut self, resolution: Vec2<usize>, line: Line<'_>) -> PassiveResult {
        debug_assert_ne!(line.location.width, 0);

        if line.location.position.0 + line.location.width > resolution.0 {
            return Err(Error::invalid("data block x coordinate"))
        }

        if line.location.position.1 > resolution.1 {
            return Err(Error::invalid("data block y coordinate"))
        }

        debug_assert_ne!(resolution.0, 0);
        debug_assert_ne!(line.location.width, 0);

        let start_index = line.location.position.1 * resolution.0 + line.location.position.0;
        let end_index = start_index + line.location.width;

        match self {
            Samples::F16(samples) => line.read_samples(&mut samples[start_index .. end_index]),
            Samples::F32(samples) => line.read_samples(&mut samples[start_index .. end_index]),
            Samples::U32(samples) => line.read_samples(&mut samples[start_index .. end_index]),
        }
    }

    /// Read one line of pixel data from this sample block.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, resolution: Vec2<usize>, write: &mut impl Write) {
        debug_assert!(index.position.0 + index.width <= resolution.0, "x max {} of width {}", index.position.0 + index.width, resolution.0); // TODO this should Err() instead
        debug_assert!(index.position.1 < resolution.1, "y: {}, height: {}", index.position.1, resolution.1);
        debug_assert_ne!(index.width, 0);

        debug_assert_ne!(resolution.0, 0);
        debug_assert_ne!(index.width, 0);

        let start_index = index.position.1 * resolution.0 + index.position.0;
        let end_index = start_index + index.width;

        match &self {
            Samples::F16(samples) =>
                LineIndex::write_samples(&samples[start_index .. end_index], write)
                .expect("writing line bytes failed"),

            Samples::F32(samples) =>
                LineIndex::write_samples(&samples[start_index .. end_index], write)
                .expect("writing line bytes failed"),

            Samples::U32(samples) =>
                LineIndex::write_samples(&samples[start_index .. end_index], write)
                .expect("writing line bytes failed"),
        }

        // LineIndex::write_samples(&self.samples[start_index .. end_index], write)

    }
}

impl std::fmt::Debug for Samples {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Samples::F16(vec) => write!(formatter, "[f16; {}]", vec.len()),
            Samples::F32(vec) => write!(formatter, "[f32; {}]", vec.len()),
            Samples::U32(vec) => write!(formatter, "[u32; {}]", vec.len()),
        }
    }
}
