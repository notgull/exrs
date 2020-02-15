# exrs (exr-rs)

This library is a 100% Rust and 100% safe code 
encoding and decoding library for the OpenEXR image file format.

[OpenEXR](http://www.openexr.com/) 
is the de-facto standard image format in animation, VFX, and 
other computer graphics pipelines, for it can represent an immense variety of pixel data with lossless compression. 

Features include:
- any number of images placed anywhere in 2d space
- any number of channels in an image (rgb, xyz, lab, depth, motion, mask, ...)
- any type of high dynamic range values (16bit float, 32bit float, 32bit unsigned integer) per channel
- any number of samples per pixel ("deep data")
- uncompressed pixel data for fast file access
- lossless compression for any image type 
- lossy compression for non-deep image types for very small files
- load specific sections of an image without processing the whole file
- compress and decompress images in parallel
- embed any kind of meta data, including custom bytes, with full backwards compatibility

### Current Status

This library is in an early stage of development. It only supports a few of all possible image types.
Currently, deep data and complex compression algorithms are not supported yet.

_Highly experimental!_

__Currently supported:__

- Supported OpenEXR Features
    - [x] custom attributes
    - [x] multi-part images
    - [x] multi-resolution images: mip maps, rip maps
    - [x] any line order
    - [x] extract meta data of any file, 
          including files with deep data and any compression format
    - [ ] channel subsampling
    - [ ] deep data
        
    - [ ] compression methods (help wanted)
        - [x] uncompressed
        - [x] zip line
        - [x] zip block
        - [x] rle
        - [ ] piz
        - [ ] pxr24
        - [ ] b44, b44a
        - [ ] dwaa, dwab

- Nice Things
    - [x] read meta data without having to load image data
    - [x] read all contents at once
        - [x] decompress image sections either 
              in parallel or with low memory overhead
    - [x] write all contents at once
        - [x] compress blocks in parallel
    - [x] read only some blocks dynamically
    - [ ] write blocks streams, one after another
    - [ ] progress callback
    - [ ] memory mapping
    
    
<!--
- [x] Inspecting Metadata
    - [x] Singlepart
        - [x] Tiles
        - [x] Scan lines
        - [x] Deep Tiles
        - [ ] Deep Scan Lines _(coded, but untested)_
    - [x] Multipart
        - [x] Tiles
        - [x] Scan lines
        - [ ] Deep Tiles _(coded, but untested)_
        - [x] Deep Scan Lines
    - [x] Multi Resolution
        - [x] Singular Resolution
        - [x] MipMaps
        - [x] RipMaps _(coded, but untested)_
    - [x] Non-Standard Attributes
        - [x] Reading those with known names and unknown names
        - [x] Reading those with known types
        - [x] Reading those with unknown types into a plain byte buffer
    - [ ] Nice API for preview attribute extraction
    
- [ ] Decompressing Pixel Data
    - [x] Any LineOrder
    - [x] Any Pixel Type (`f16`, `f32`, `u32`)
    - [x] Multipart
    - [ ] Deep Data
    - [x] Rip/Mip Maps  _(coded, but untested)_
    - [ ] Nice API for RGBA conversion and displaying other color spaces?
    - [ ] Compression Methods
        - [x] Uncompressed
        - [x] ZIPS
        - [x] ZIP
        - [x] RLE
        - [ ] PIZ
        - [ ] RXR24
        - [ ] B44, B44A
        - [ ] DWAA, DWAB

- [ ] Writing images
    - [x] Scan Lines
    - [x] Tiles
    - [x] Multipart
    - [ ] Deep Data
    - [ ] User supplied line order
    - [x] Rip/Mip Maps _(coded, but untested)_
    - [ ] 100% correct meta data
    - [x] Compression Methods
        - [x] Uncompressed
        - [x] ZIPS
        - [x] ZIP
        - [x] RLE
        - [ ] PIZ
        - [ ] RXR24
        - [ ] B44, B44A
        - [ ] DWAA, DWAB
    
- [x] Decompressing multiple blocks in parallel
- [ ] Compressing multiple blocks in parallel

- [ ] Profiling and real optimization
    - [ ] Memory Mapping?
- [ ] IO Progress callback?
- [ ] SIMD
- [ ] Detailed file validation
    - [x] Channels with an x or y sampling rate other than 1 are allowed only in flat, scan-line based images.
    - [x] If the headers include timeCode and chromaticities attributes, then the values of those attributes must also be the same for all parts of a file
    - [x] Scan-line based images cannot be multi-resolution images. (encoded in type system)
    - [x] Scan-line based images cannot have unspecified line order apparently?
    - [x] Image part name is required for multipart images
    - [x] Enforce minimum length of 1 for arrays
    - [x] [Validate data_window matches data size when writing images] is not required because one is inferred from the other
    - [x] Channel names and image part names must be unique
    
- [ ] Explore different APIs
    - [x] Let user decide how to store data
    - [x] Loading Metadata and specific tiles or blocks separately
-->
    

### Usage

Until we have a package on crates.io, include this git repository:

```toml
[dependencies]
exr = { git = "http://github.com/johannesvollmer/exrs" }
```

The master branch of this repository is always an up-to-date version.

### Example

Example: Write all image contents to an exr file at once.
```rust
fn main() {
    let size = Vec2(1024, 512);

    // create a channel containing 1024x512 f32 values
    let luma = Channel::new_linear(
        "Y".try_into().unwrap(), // OpenEXR only supports ascii, so this may fail
        Samples::F32(generate_f32_vector(size))
    );

    let layer = simple::Part::new(
        "test-image".try_into().unwrap(), // layer name
        IntRect::from_dimensions(size.to_u32()), // set position to (0,0) and size to 1025x512
        smallvec![ luma ], // include the one channel we created
    );
    
    // create an exr file from a single image part (an exr file can have multiple image parts)
    let image = Image::new_from_single_part(layer.with_compression(Compression::RLE));

    println!("writing image with meta data {:#?}", image);

    // write the image, compressing in parallel with all available cpus
    image.write_to_file("./testout/constructed.exr", WriteOptions::high()).unwrap();
}
```

See the examples folder for a full example.


### Cleanup Tasks Before Version 1.0
- [x] remove all calls to `Option::unwrap()` and `Result::unwrap()` (except for tests)
- [ ] remove all irrelevant print statements
- [ ] remove inappropriate `assert!` and `debug_assert!` calls,
        all `unimplemented!` calls,
        and use real Error handling instead
- [x] reduce all not required `pub` usages
- [ ] revisit all TODO items
- [x] revisit all `as` casts 
- [x] revisit all `[unchecked_index]` array accesses


### Motivation

Using the [Rust bindings to OpenEXR](https://github.com/cessen/openexr-rs) 
requires compiling multiple C++ Libraries 
and setting environment variables, 
which I didn't quite feel like to do, 
so I wrote this library instead.

Also, I really wanted to have a library 
which had an 'X' in its name in my git repositories.

### Goals

`exrs` aims to provide a safe and convenient 
interface to the OpenEXR file format.

This library does not try to be a general purpose image file or image processing library.
Therefore, color conversion, subsampling, and mip map generation are left to other crates for now.
As the original OpenEXR implementation supports those operations, this library may choose to support them later.
Furthermore, this implementation does not try to produce byte-exact file output
matching the original implementation, but only correct output.

#### Safety
This library uses no unsafe code. In fact, this crate is annotated with `#[forbid(unsafe_code)]`.
Its dependencies use unsafe code, though.

All information from a file is handled with caution.
Allocations have a safe maximum size that will not be exceeded at once.


### What I am proud of

-   Flexible API allows for custom parallelization
-   Difficult to misuse API
-   This is a pretty detailed README
-   (more to come)

### Specification

This library is modeled after the 
official [`OpenEXRFileLayout.pdf`](http://www.openexr.com/documentation.html)
document. Unspecified behavior is concluded from the C++ library.

__Things that are not as specified in the PDF file__ (Or were forgotten):

-   String Attributes don't store their length,
    because it can be inferred from the Attribute byte-size.
-   Chunk Part-Number is not u64, but i32.

### PRIORITIES
1. Publish version 1.0
1. Deep Data
1. Decode all compression formats
1. Simple rendering of common image formats
1. Profiling and other optimization
1. Tooling (Image Viewer App)
