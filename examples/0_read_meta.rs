

// exr imports
extern crate exr;

/// Print the custom meta data of a file, excluding technical encoding meta data.
/// Prints compression method and tile size, but not purely technical data like chunk count.
fn main() {
    use exr::prelude::*;

    let meta_data = MetaData::read_from_file(
        "tests/images/valid/custom/crowskull/crow_uncompressed.exr",
        false // do not throw an error for invalid or missing attributes, skipping them instead
    ).unwrap();

    for (layer_index, image_layer) in meta_data.headers.iter().enumerate() {
        println!(
            "custom meta data of layer #{}:\n{:#?}",
            layer_index, image_layer.own_attributes
        );
    }
}