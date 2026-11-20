//! Context packing helpers for LLM context window optimization.

mod budget;
mod manifest;
mod output;
mod render;
mod select;

pub(crate) use budget::parse_budget;
pub(crate) use manifest::write_bundle_directory;
pub(crate) use output::{
    append_context_log_record, determine_output_destination, write_to_destination,
};
pub(crate) use render::{
    CountingWriter, format_list_output, remove_completion_marker,
    write_context_bundle_output as write_bundle_output, write_handoff_bundle_output,
};
pub(crate) use select::{SelectOptions, SelectResult, select_files_with_options};

#[cfg(test)]
pub(crate) mod required_reads;
