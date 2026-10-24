//! NBT decoding with explicit resource limits.

mod decode;
mod error;
mod limits;
mod value;

pub use decode::{Compression, decode, detect_compression};
pub use error::{NbtError, NbtErrorCode};
pub use limits::DecodeLimits;
pub use value::{NbtCompound, NbtList, NbtRoot, NbtTagType, NbtValue};

pub const CRATE_NAME: &str = "litematic-nbt";
