#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::useless_transmute)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::missing_safety_doc)]

// --- 1. Manual Definitions MUST come first ---
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct IO_STATUS_BLOCK {
    pub Status: i32,
    pub Information: usize,
}
pub type PIO_STATUS_BLOCK = *mut IO_STATUS_BLOCK;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct IMAGE_TLS_DIRECTORY64 {
    pub StartAddressOfRawData: u64,
    pub EndAddressOfRawData: u64,
    pub AddressOfIndex: u64,
    pub AddressOfCallBacks: u64,
    pub SizeOfZeroFill: u32,
    pub Characteristics: u32,
}

#[allow(non_camel_case_types)]
pub type FILE_ACCESS_RIGHTS = u32;
#[allow(non_camel_case_types)]
pub type FILE_FLAGS_AND_ATTRIBUTES = u32;

// --- 2. Single Include at the end ---
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(feature = "docsrs")]
mod bindings;

#[cfg(feature = "docsrs")]
pub use bindings::*;
