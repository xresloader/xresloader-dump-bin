// When the `system-alloc` feature is used, use the System Allocator
#[cfg(feature = "system-alloc")]
use std::alloc::System;
#[cfg(feature = "system-alloc")]
#[global_allocator]
static GLOBAL: System = System;

extern crate xresloader_protocol;

extern crate clap;

#[macro_use]
extern crate log;
extern crate bytes;
extern crate env_logger;
extern crate json;
extern crate protobuf_json_mapping;

use clap::{ArgAction, Parser};
use std::io::Read;
use Vec;

use protobuf::{descriptor::FileDescriptorSet, Message, MessageFull};
// use xresloader_protocol::proto::Xresloader_datablocks;

mod file_descriptor_index;

use file_descriptor_index::FileDescriptorIndex;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
struct DumpOptions {
    /// pb file
    #[clap(short, long, value_parser, action = ArgAction::Append)]
    pb_file: Vec<String>,

    /// binary file generated by xresloader
    #[clap(short, long, value_parser, action = ArgAction::Append)]
    bin_file: Vec<String>,

    /// Debug mode
    #[clap(long, value_parser, default_value = "false")]
    debug: bool,

    /// Pretty mode
    #[clap(long, value_parser, default_value = "false")]
    pretty: bool,

    /// Plain mode
    #[clap(long, value_parser, default_value = "false")]
    plain: bool,
}

fn main() {
    let args = DumpOptions::parse();

    if args.debug {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .format_level(true)
            .format_module_path(false)
            .format_target(false)
            .parse_default_env()
            .init();
    } else {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .format_level(false)
            .format_module_path(false)
            .format_target(false)
            .format_timestamp(None)
            .parse_default_env()
            .init();
    }

    let mut desc_index = FileDescriptorIndex::new();

    for pb_file in args.pb_file {
        debug!("Load pb file: {}", pb_file);
        match std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(&pb_file)
        {
            Ok(mut f) => {
                let mut bin_data = Vec::new();
                let _ = f.read_to_end(&mut bin_data);
                match FileDescriptorSet::parse_from_bytes(&bin_data) {
                    Ok(pbs) => {
                        debug!("Parse pb file: {} success", pb_file);
                        for pb_file_unit in &pbs.file {
                            debug!(
                                "  Found proto file: {} has {} message(s) and {} enum(s)",
                                pb_file_unit.name(),
                                pb_file_unit.message_type.len(),
                                pb_file_unit.enum_type.len()
                            );
                            desc_index.add_file(&pb_file_unit, &pb_file);
                        }
                    }
                    Err(e) => {
                        error!("Parse pb file {} failed, {}, ignore this file", pb_file, e);
                    }
                }
            }
            Err(e) => {
                error!(
                    "Try to open file {} failed, {}, ignore this file",
                    pb_file, e
                );
            }
        }
    }

    let mut has_error = false;

    for bin_file in args.bin_file {
        debug!("Load xresloader output binary file: {}", bin_file);
        match std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(&bin_file)
        {
            Ok(mut f) => {
                let mut bin_data = Vec::new();
                let _ = f.read_to_end(&mut bin_data);
                match xresloader_protocol::proto::pb_header_v3::Xresloader_datablocks::parse_from_bytes(&bin_data) {
                    Ok(data_blocks) => {
                        if data_blocks.data_message_type.is_empty() {
                            has_error = true;
                            error!("File {} has no data_message_type, please use xresloader 2.6 or upper", &bin_file);
                            continue;
                        }
                        debug!("Parse {} from file: {} success, message type: {}",
                            xresloader_protocol::proto::pb_header_v3::Xresloader_datablocks::descriptor().full_name(), &bin_file,
                            &data_blocks.data_message_type
                        );

                        let message_descriptor = match desc_index.build_message_descriptor(&data_blocks.data_message_type) {
                            Ok(x) => x,
                            Err(_) => {
                                error!("Build message descriptor {} failed", &data_blocks.data_message_type);
                                has_error = true;
                                continue;
                            }
                        };


                        info!("======================== Header: {} ========================", &bin_file);
                        info!("xresloader version: {}", data_blocks.header.xres_ver);
                        info!("data version: {}", data_blocks.header.data_ver);
                        info!("data count: {}", data_blocks.header.count);
                        info!("hash code: {}", data_blocks.header.hash_code);
                        info!("description: {}", data_blocks.header.description);
                        for data_source in &data_blocks.header.data_source {
                            info!("  data source - file: {}, sheet: {}", data_source.file, data_source.sheet);
                        }
                        info!("======================== Body: {} -> {} ========================", &bin_file, &data_blocks.data_message_type);
                        let mut row_index = 0;
                        if !args.plain {
                            info!("[");
                        }
                        for row_data_block in &data_blocks.data_block {
                            row_index += 1;
                            match message_descriptor.parse_from_bytes(&row_data_block) {
                                Ok(message) => {
                                    if args.pretty {
                                        if args.plain {
                                            info!("  ------------ Row {} ------------\n{}", row_index, protobuf::text_format::print_to_string_pretty(message.as_ref()));
                                        }
                                        if let Ok(output) = protobuf_json_mapping::print_to_string(message.as_ref()) {
                                            info!("    {},",  json::stringify_pretty(json::parse(&output).unwrap(), 2));
                                        } else {
                                            info!("{}", protobuf::text_format::print_to_string_pretty(message.as_ref()));
                                        }
                                    } else {
                                        if args.plain {
                                            info!("{}", protobuf::text_format::print_to_string(message.as_ref()));
                                            continue;
                                        }
                                        if let Ok(output) = protobuf_json_mapping::print_to_string(message.as_ref()) {
                                            info!("    {},", output);
                                        } else {
                                            info!("{}", protobuf::text_format::print_to_string(message.as_ref()));
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Parse row {} to message {} failed, {}", row_index, &data_blocks.data_message_type, e);
                                    has_error = true;
                                    continue;
                                }
                            }
                        }
                        if !args.plain {
                            info!("]");
                        }
                    }
                    Err(e) => {
                        error!("Parse {} from file {} failed, {}, ignore this file", xresloader_protocol::proto::pb_header_v3::Xresloader_datablocks::descriptor().full_name(), bin_file, e);
                        has_error = true;
                    }
                }
            }
            Err(e) => {
                error!(
                    "Try to open file {} failed, {}, ignore this file",
                    &bin_file, e
                );
                has_error = true;
            }
        }

        // 2.6.0
    }

    if has_error {
        std::process::exit(1);
    }
}